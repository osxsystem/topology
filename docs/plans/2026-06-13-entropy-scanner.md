# Plan: entropy scanner (FM5) — implementation

- **Date:** 2026-06-13 · **Feature slug:** entropy-scanner
- **Design:** [docs/specs/2026-06-13-entropy-scanner.md](../specs/2026-06-13-entropy-scanner.md) (approved)
- **Research:** [docs/research/2026-06-13-entropy-scanner.md](../research/2026-06-13-entropy-scanner.md)

## Baseline (clean)

`env -u GATEKEEPER_BIN -u TOPOLOGY_ROOT cargo test --release` → 21 suites ok, 0 failed. The env scrub
is mandatory locally (a stale `GATEKEEPER_BIN` breaks the `cli_doctor` probe; CI is unaffected).
**Every cargo command below is prefixed `env -u GATEKEEPER_BIN -u TOPOLOGY_ROOT`.**

## Files to touch

| File | Responsibility | Protected? |
|------|----------------|-----------|
| `gatekeeper/src/scan.rs` | `Kind::Entropy` + fields; Shannon helper; entropy lane; `[scan]` parse + glob matcher; apply excludes on path lanes | no |
| `gatekeeper/src/version.rs` | accept schema 1 or 2; advertise 2 | no |
| `security/rules.toml` | `schema_version = 2`; 2 entropy rules (`warn`); `[scan] exclude_paths` | **yes** |
| `gatekeeper/tests/cli_scan_bench.rs` | flip `hex64-unlabeled`/`base64-unlabeled` to `in_scope`; assert negatives BLOCK-clean | no |
| `gatekeeper/tests/cli_scan_entropy.rs` | NEW: entropy detection, exclude_paths, warn-not-block, glob matcher | no |
| `scripts/sync-gitleaks-rules.sh` | NEW: fetch+translate gitleaks subset to a review file, provenance header, no auto-merge | no |
| `docs/adr/0018-entropy-scanning.md`, `CHANGELOG.md`, `docs/ROADMAP.md` | docs | no |

Only `security/rules.toml` is protected — its commit uses the scan-then-`--no-verify` override.

## Conventions
- No new crates (ADR-0007): Shannon entropy via `f64::log2`; glob matcher hand-rolled (`*` + trailing-`/` prefix).
- Reuse the existing `Finding` struct/emission and `Severity::Warn` (already exit-0 = shadow).
- Entropy is a separate lane from the `RegexSet` one-pass; it runs wherever content rules run; excludes apply only where a path exists.

## Tasks (TDD order — test first, watch red, implement, watch green)

### Task 1 — schema v2 acceptance (back-compat)
- **Test (test-engineer), `scan.rs`/`version.rs` tests:** `schema_version_2_accepted` (a v2 `rules.toml` string parses ok); `schema_version_1_still_accepted` (a v1 string parses — back-compat); `schema_version_3_rejected` (err names "expected 1 or 2"); keep `advertised_schema_is_accepted_by_parser` green.
- **Watch red** (`cargo test --release --bin gatekeeper schema`), **impl (feature-implementer):** `SCHEMA_VERSION = 2`; replace `!= 1` reject (`scan.rs:162`) with `!= 1 && != 2`. **Green + commit** `feat(scan): accept rules schema_version 1 or 2`.

### Task 2 — Shannon entropy helper
- **Test (test-engineer), `scan.rs` tests:** `shannon_entropy_uniform_hex_near_4` (a 64-char uniform-hex token → bits/char ≈ 4.0, asserted within 0.2); `shannon_entropy_repetitive_low` (`"aaaa…"` → near 0); `shannon_entropy_handles_short`/empty.
- **Watch red, impl:** `fn shannon_entropy(token: &str) -> f64` returning bits per character (`H = -Σ p_i·log2 p_i` over the token's chars). **Green + commit** `feat(scan): shannon_entropy helper (bits per char)`.

### Task 3 — `kind = "entropy"` parsing
- **Test (test-engineer):** `entropy_rule_parses` — a `[[rule]] kind="entropy" charset="hex" min_length=32 threshold_bits_per_char=3.0` parses into the compiled entropy-rule form with those fields; an entropy rule missing `charset` errs with a clear message.
- **Watch red, impl:** add `Kind::Entropy`; add optional `charset`/`min_length`/`threshold_bits_per_char` to `RawRule`; build a `CompiledEntropyRule { id, severity, description, charset, min_length, threshold }`; route `kind="entropy"` rules into an entropy-rule vector (not a `RegexSet`). **Green + commit** `feat(scan): parse kind="entropy" rules`.

### Task 4 — entropy lane (detection)
- **Test (test-engineer), NEW `gatekeeper/tests/cli_scan_entropy.rs`:** `entropy_flags_unlabeled_hex64` (pipe a bare 64-hex string to `scan --content` with a v2 rules file carrying the hex entropy rule → a `WARN ` line names the entropy rule, exit 0); `entropy_flags_unlabeled_base64_40`; `entropy_ignores_low_entropy_prose` (an English sentence → no entropy WARN); `entropy_never_blocks` (entropy rule at `warn` → exit 0 even when it fires).
- **Watch red, impl:** in `scan_with` (or a sibling pass invoked alongside it), for each entropy rule walk `[A-Za-z0-9+/=_-]{min_length,}` runs, compute `shannon_entropy`, normalize/compare to `threshold_bits_per_char`, emit a `Finding` (warn) per flagged run (through `is_allowed`). **Green + commit** `feat(scan): entropy detection lane (warn)`.

### Task 5 — `[scan] exclude_paths` + glob matcher
- **Test (test-engineer), `scan.rs` + `cli_scan_entropy.rs`:** `glob_match` unit cases (`*.lock`→`Cargo.lock` true, `tests/fixtures/`→`tests/fixtures/x.txt` true, `*.rs`→`a.lock` false); `exclude_paths_suppresses_entropy_on_staged` (a `*.lock`-matching path with a high-entropy token → no entropy WARN) and `exclude_paths_does_not_suppress_regex_rules` (a labeled secret in a `*.lock` path still BLOCKs); `exclude_paths_not_applied_on_content` (no path → entropy still fires).
- **Watch red, impl:** add `ScanConfig{exclude_paths}` deserialized from a `[scan]` table (default empty); carry it on the `Rules` struct; add `fn glob_match(path, glob) -> bool` (dep-free); on path-bearing lanes, skip the entropy lane (only) when the path matches any glob. **Green + commit** `feat(scan): [scan] exclude_paths suppresses entropy on path lanes`.

### Task 6 — rules.toml schema v2 + entropy rules + [scan] (PROTECTED)
- **Impl (feature-implementer):** set `schema_version = 2`; add `hex-high-entropy` (hex, min_length 32, 3.0, warn) and `base64-high-entropy` (base64, min_length 20, 4.5, warn); add `[scan] exclude_paths = ["*.lock","*.svg","*.min.js","tests/fixtures/"]`.
- **Verify:** `cargo test --release` green. **Commit (protected override):** `gatekeeper scan --staged` → confirm ONLY the protected-path BLOCK on `security/rules.toml` (no real secret) → `git commit --no-verify` documenting the override: `feat(rules): schema v2 + entropy rules (warn) + [scan] exclude_paths`.

### Task 7 — bench update (acceptance)
- **Test (test-engineer), `cli_scan_bench.rs`:** flip `hex64-unlabeled` and `base64-unlabeled` to `in_scope = true` with `expect_rules` naming the entropy rules; raise the floor accordingly; ensure the negative assertion checks **no `BLOCK`** on negatives (warns permitted — reconcile with the file's existing "path-aware lane" comment). **Green:** `cargo test --release --test cli_scan_bench` (≥10/11 in-scope detected). Commit `test(scan): bench — entropy classes in-scope, negatives block-clean`.

### Task 8 — gitleaks sync script
- **Impl (feature-implementer):** `scripts/sync-gitleaks-rules.sh` — fetch a pinned gitleaks `gitleaks.toml` (offline-guarded: no-op/skip if unreachable or `--offline`), translate a curated provider-prefix subset into `[[rule]]` schema, write to `security/rules.gitleaks-review.toml` (a review file, NOT `rules.toml`) with a `# synced-from: gitleaks@<sha>` header, print a diff. `set -euo pipefail`; shellcheck-clean. Add a `just sync-gitleaks` recipe (no CI network). Commit `feat(scan): sync-gitleaks-rules.sh (review-only, never auto-merge)`.

### Task 9 — ADR-0018 + docs
- **Impl (main loop):** `docs/adr/0018-entropy-scanning.md` (Accepted: schema v2, entropy detection, shadow-first warn + excludes + burn-in, the indistinguishability limit, the gitleaks-sync constraint-compatible substitute); `CHANGELOG.md` Unreleased entry; `docs/ROADMAP.md` Phase 15 status note (entropy delivered; routing + flips still open). Commit `docs(adr): ADR-0018 entropy scanning; CHANGELOG + ROADMAP`.

## Done when
`env -u GATEKEEPER_BIN -u TOPOLOGY_ROOT cargo test --release` all green; `cargo clippy --release --bin gatekeeper -- -D warnings` and `cargo fmt --check` clean; `shellcheck scripts/sync-gitleaks-rules.sh` clean; bench ≥10/11 in-scope with negatives block-clean; verify artifact records the unlabeled-secret reproduce→resolve; fresh-context review passes bound to HEAD; finish gate green.
