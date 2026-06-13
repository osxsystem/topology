# Verify — entropy scanner (FM5: unlabeled-secret class fix)

- **Date:** 2026-06-13
- **Spec:** `docs/specs/2026-06-13-entropy-scanner.md` · **Plan:** `docs/plans/2026-06-13-entropy-scanner.md` ·
  **ADR:** `docs/adr/0018-entropy-scanning.md`
- **Binary:** `gatekeeper 0.9.0`

## Symptom (before)

The scanner was prefix/label-anchored. An **unlabeled** high-entropy value — a bare 64-hex token or a
40-char base64 blob with no `key=`/`AKIA`/`eyJ` anchor — matched no rule and committed clean (FM5).
The secrets bench encoded exactly this: `hex64-unlabeled` and `base64-unlabeled` were
`in_scope = false` with `expect_rules = []` (known gap), and `bench_negatives_stay_clean` treated any
finding as a failure.

## Resolution (after)

Rules **schema v2** adds `kind = "entropy"`: detection walks maximal charset runs and flags those whose
per-character **Shannon entropy** meets the threshold. The two unlabeled tokens are now detected (as
`WARN`, shadow), while labeled/regex detection is unchanged. `[scan] exclude_paths` bounds entropy FPs
on path-bearing lanes; entropy ships `severity = "warn"` so it never blocks (promotion to `block` is
deferred to a burn-in FP measurement).

### Reproduce-then-resolve evidence

The new detection lane and the reconciled bench prove the class is now caught (a bare 64-hex / 40-char
base64 token → `WARN`, exit 0), that entropy never blocks, that `exclude_paths` suppresses the entropy
lane on path-bearing lanes only (not labeled rules, not `--content`), and that the two previously-clean
unlabeled bench positives are now in-scope and detected:

```evidence
$ cargo test --release --test cli_scan_entropy
# expect: 7 passed
```

```evidence
$ cargo test --release --test cli_scan_bench
# expect: 3 passed
```

- **`entropy_flags_unlabeled_hex64` / `entropy_flags_unlabeled_base64_40`** — a bare 64-hex / 40-char
  base64 token → `WARN hex-high-entropy` / `WARN base64-high-entropy`, exit 0. (Before: no rule fired.)
- **`entropy_never_blocks`** — an entropy hit keeps exit 0 (shadow).
- **`entropy_ignores_low_entropy_text`** — ordinary prose → no entropy WARN.
- **`exclude_paths_suppresses_entropy_on_staged`** / **`_does_not_suppress_regex_rules`** /
  **`_not_applied_on_content`** — excludes scope the entropy lane on `--staged` only; a labeled secret
  in an excluded path still BLOCKs; `--content` (no path) still scans.
- **bench**: `hex64-unlabeled`/`base64-unlabeled` now `in_scope` (detected via WARN), floor 11/11;
  `bench_negatives_produce_no_block` asserts negatives produce no BLOCK (shadow warns on benign
  high-entropy content are expected), with a runtime-assembled PEM non-vacuity guard.

### Schema back-compat + foundation

```evidence
$ cargo test --release --bin gatekeeper
# expect: test result: ok
```

`schema_version_1_still_accepted` (back-compat), `schema_version_2_accepted`, `schema_version_3_rejected`,
the `shannon_entropy` unit tests, and `glob_match` unit tests are all green.

## Full suite + lints

`env -u GATEKEEPER_BIN -u TOPOLOGY_ROOT cargo test --release` → all 22 suites green;
`cargo clippy --release --bin gatekeeper -- -D warnings` and `cargo fmt --check` clean;
`shellcheck scripts/sync-gitleaks-rules.sh` clean.

> Local note: `cargo` runs require the `env -u GATEKEEPER_BIN -u TOPOLOGY_ROOT` prefix (a stale local
> var breaks the `cli_doctor` probe; CI is unaffected).

## Known shadow FP (documented, non-blocking)

`base64-high-entropy` WARNs on the canonical AWS-docs example secret embedded inside an allowlist
*pattern* in `rules.toml` (the allowlist entry is rule-scoped to `aws-secret-access-key`, so it does not
suppress the entropy rule). This is exactly the kind of FP that burn-in measures before any `block`
promotion — left as documented shadow noise, not a real secret.

## Scope honesty

The Phase diff (`bb92ea2..HEAD`) adds the entropy lane to `scan.rs`, `[tdd]`-style `[scan]` config, the
schema-2 acceptance, the two entropy rules + `[scan]` in `rules.toml`, the bench reconcile, the gitleaks
review-file script, and docs (ADR-0018, CHANGELOG, ROADMAP). **Deferred by design:** promotion to
`block` (burn-in), path-triggered routing + router eval (later Phase 15 deliverables). No version bump.

## Gate status

research ✓ · design ✓ (PASS) · plan ✓ (PASS, baseline 22 suites) · tdd ✓ (every behavior red→green;
two unlabeled bench positives flipped in-scope) · finish ✓ (full suite green) · clippy/fmt/shellcheck clean.
