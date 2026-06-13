VERDICT: pass
HEAD: 37750758d7e7e39a6315ef85b1a2c362578f722c
BASE: 72c9155abe2dd8d76e898bee063ace51ff9c78a9

## Blocking findings

None.

## Criteria checked

### Spec/plan

The diff faithfully implements the approved design (`docs/specs/2026-06-13-entropy-scanner.md`) and the
nine-task plan (`docs/plans/2026-06-13-entropy-scanner.md`). Verified item by item against running code,
not just by reading assertions:

- **Schema v2, back-compat (spec §"Schema v2"; plan Task 1).** `scan.rs:17` `SCHEMA_VERSION = 2`;
  `scan.rs:240` accepts `!= 1 && != 2` (rejects others with "expected 1 or 2", `scan.rs:242`).
  `version.rs:17` delegates to `scan::schema_version()`; the round-trip guard
  `advertised_schema_is_accepted_by_parser` keeps them synced. Targeted run: `schema_version_1_still_accepted`,
  `schema_version_2_accepted`, `schema_version_3_rejected`, `rules_schema_delegates_to_scan` all `ok`
  (8 passed, 0 failed).

- **`shannon_entropy` is bits-per-char and guarded (spec acceptance; plan Task 2).** `scan.rs:366-383`
  computes `H = -Σ p·log2 p` over char counts; empty token short-circuits to `0.0` (`scan.rs:368`,
  no div-by-zero/NaN). Unit tests `shannon_entropy_uniform_hex_near_4` (≈4.0 within 0.2),
  `shannon_entropy_repetitive_is_low` (<0.1), `shannon_entropy_empty_is_zero` (`is_finite()` + `== 0.0`)
  pass. Confirmed end-to-end against the real shipped `rules.toml`: a uniform 64-hex token →
  `WARN hex-high-entropy ... (redacted: 9f86…<len=64>)`, exit 0; prose → no finding, exit 0.

- **`kind="entropy"` parsing + per-kind validation (spec §"kind=entropy"; plan Task 3).** `Kind::Entropy`
  (`scan.rs:80`); `RawRule.pattern/charset/min_length/threshold_bits_per_char` made `Option` (`scan.rs:65-72`);
  routed into `CompiledEntropyRule` (`scan.rs:181-188`, `scan.rs:281-296`). Content/command rules still
  *require* `pattern` (`scan.rs:260`, errors "requires a 'pattern'"); entropy rules require `charset`
  (`scan.rs:285`). `entropy_rule_parses` confirms one entropy rule lands in `r.entropy`, zero in `r.content`.
  Making `pattern` optional dropped no labeled rule — see the bench result below.

- **Entropy detection lane, charset runs (spec §"Entropy lane"; plan Task 4).** `scan_entropy`
  (`scan.rs:450-494`) walks maximal `in_charset` runs (`scan.rs:439-446`: hex = `is_ascii_hexdigit`,
  base64 = alnum + `+/=_-`), skips runs `< min_length`, flags `shannon_entropy >= threshold`, passes
  through `is_allowed`, emits a `warn` `Finding`. Tests `entropy_flags_unlabeled_hex64`,
  `entropy_flags_unlabeled_base64_40`, `entropy_ignores_low_entropy_text` pass.

- **warn-never-blocks (spec acceptance; plan Task 4).** `report()` (`scan.rs:530-550`) maps `Warn`→0,
  `Block`→1. `entropy_never_blocks` asserts a *firing* entropy rule still exits 0. **MUTATION (decisive):**
  raising the threshold compare to `< 99.0` (no-op detector) turned `entropy_flags_unlabeled_hex64`,
  `_base64_40`, `entropy_never_blocks`, `exclude_paths_not_applied_on_content` RED and dropped the bench
  floor to 9/11 — proving detection (not a vacuous assertion) is what makes the green. Restored to clean
  HEAD (`git diff --stat` empty).

- **`[scan] exclude_paths` scopes entropy only, path-lanes only (spec §"[scan] exclude_paths"; plan Task 5).**
  `ScanConfig.exclude_paths` (`scan.rs:51-53`); applied on `--staged` (`scan.rs:1037`) and `--hook`
  Write/Edit (`scan.rs:1380`, `1442`) by gating *only* the `scan_entropy` call — the regex `scan_with`
  call runs unconditionally above it. `--content` (`scan.rs:689`) and `--cmd` (`scan.rs:719`) pass no path
  and never consult excludes. `exclude_paths_suppresses_entropy_on_staged`,
  `_does_not_suppress_regex_rules` (a planted AWS key in `Cargo.lock` still BLOCKs, exit 1),
  `_not_applied_on_content` all pass. **MUTATION (decisive):** wrapping the staged regex `scan_with` inside
  the same `!exclude` guard turned `exclude_paths_does_not_suppress_regex_rules` RED (the labeled key
  stopped blocking) — proving that test is non-vacuous. Restored.

- **glob_match edges (plan Task 5).** `glob_match` (`scan.rs:498-527`) handled every probed case correctly
  in a standalone harness (no panic, no backtracking): `*.lock`→`Cargo.lock` T, `*.lock`→`a.rs` F,
  `*.lock`→`lock.txt` F, `*.lock`→`Cargo.lockx` F, `tests/fixtures/`→`tests/fixtures/x` T,
  `tests/fixtures/`→`tests/fixturesX` F, `*.min.js`→`app.min.js` T, `""`→`*.lock` F, `*`→arbitrary T.

- **rules.toml schema v2 + two warn entropy rules + [scan] (plan Task 6).** `security/rules.toml`:
  `schema_version = 2`; `hex-high-entropy` (hex, 32, 3.0, warn); `base64-high-entropy` (base64, 20, 4.5,
  warn); `[scan] exclude_paths = ["*.lock","*.svg","*.min.js","tests/fixtures/"]`.

- **Bench acceptance + honesty (spec acceptance; plan Task 7).** `cli_scan_bench` 3 passed. Two formerly
  out-of-scope classes flipped to `in_scope=true` with `expect_rules=["hex-high-entropy"]` /
  `["base64-high-entropy"]` (floor 11/11, rule-attributed). `bench_negatives_produce_no_block` asserts on
  BLOCK-severity only (warns are documented shadow), and `blocked_rules_signal_is_non_vacuous` proves the
  BLOCK signal is live (a runtime-assembled PEM header → exit 1, `private-key-block`). The PEM guard is
  assembled at runtime (`{pem_word}` split), so no committable secret literal. Confirmed `--content` does
  WARN on benign high-entropy negatives, by design (no path → no exclude).

- **gitleaks sync = review-only glue (spec §"gitleaks sync"; plan Task 8; ADR-0018 §5).**
  `scripts/sync-gitleaks-rules.sh`: `$RULES` (rules.toml) is only ever *read* (`grep -q ... "$RULES"`,
  lines 77/85); the only write redirect is `>"$REVIEW"` to `rules.gitleaks-review.toml`. Provenance header
  `# synced-from: gitleaks@${GITLEAKS_SHA}` emitted. Pin is an honest placeholder
  (`PIN_A_REAL_GITLEAKS_COMMIT_SHA`), not a fake-real SHA. Offline/CI runs no-op exit 0
  (`--offline` and `CI=true` both verified; no review file written). Never auto-merges.

### Standards

- **Build + full suite green.** `env -u GATEKEEPER_BIN -u TOPOLOGY_ROOT cargo test --release` → every
  suite `test result: ok` (0 failed across 22 suites, incl. 281 unit + 7 entropy + 3 bench + 23 cli_scan).
- **Lints clean.** `cargo clippy --release --bin gatekeeper -- -D warnings` → `Finished` (no warnings);
  `cargo fmt --check` → exit 0; `shellcheck scripts/sync-gitleaks-rules.sh` → exit 0.
- **No new dependency (ADR-0007).** Shannon entropy uses `f64::log2` + `std::collections::HashMap`; the
  glob matcher is hand-rolled `str` splitting. No Cargo.toml change in range.
- **Diff traceability.** `git diff --stat 72c9155 37750758` touches exactly the planned files
  (`scan.rs`, `rules.toml`, the two test files, the sync script, justfile recipe, and docs). `version.rs`
  needed no change (already delegates). No drive-by edits found in scan.rs outside the entropy/schema/
  exclude clauses.
- **Simplicity.** scan_entropy is a single O(n) pass; glob_match is ~30 lines for the two documented
  syntaxes (`*`, trailing-`/`); no speculative config knobs or second abstraction. A staff engineer would
  not call this overcomplicated.
- **3-lane discipline (markdown=truth, Rust enforces, Bash glues).** ADR-0018 + spec are the contract;
  scan.rs enforces; the sync script is review-only glue (no enforcement logic), consistent with `rules.toml`
  being a protected path.
- **AWS-docs self-FP is acceptable as warn.** Confirmed by scanning the shipped `rules.toml` itself:
  `WARN base64-high-entropy ... (redacted: wJal…<len=40>)`, exit 0 — the canonical AWS-docs example secret
  embedded in an allowlist *pattern*. It is shadow noise (warn, never blocks), exactly what burn-in measures
  before any block promotion, and is honestly disclosed in ADR-0018 §Consequences and the verify doc. Not a
  defect; correctly NOT a gate blocker.
