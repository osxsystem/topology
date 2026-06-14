VERDICT: pass
HEAD: 2d3fb8c660bfc6ce78c2fb6b41ab2cadba525f01
BASE: 0ee07ca9fb48abbc78dec7b8d5422de99cc50b64

# Review: hardened-config-layer / replay-allowlist portability fix (2026-06-14)

Fresh-context critic panel (no memory of authoring), three lenses — **correctness**, **simplicity**,
**threat-model/FM2** — each inspecting `git diff 0ee07ca…<head>` of `config.rs`, `verify.rs`,
`cli_tdd_replay.rs` plus the design/verify artifacts. The panel reviewed commit `7d90aa0`; the sole
post-review delta to this `HEAD` (`2d3fb8c`) is a **non-executable test-comment accuracy fix** (the
panel's own minor finding — see below). `cargo` produces byte-identical behavior; fmt + the three
re-pointed/positive tests stay green, so the panel's verdict carries.

## Summary

No blocking, major, or even minor *code* defects. The threat-model/FM2 lens independently verified both
load-bearing claims against the code — including re-running the swap-revert experiment and confirming the
vacuous-test backstop (`replay_rejects_vacuous_test`) still rejects post-fix:

- **No new capability / no floor loosening.** `effective_allowed_prefixes` (`config.rs:232-249`) is
  strictly add-only (clone → append `test_command`/`tdd_replay_test_command` verbatim, trim, dedup,
  empty-skip; never removes a default). `config.toml` is unprotected — but an actor who can write
  `test_command` could already write the identical string into `[verify] allowed_command_prefixes` in the
  same file, so the trust boundary is literally unchanged. The security scanner is orthogonal (`verify.rs`
  has zero scan/deny refs; `gatekeeper scan` covers `config.toml` writes independently via PreToolUse +
  pre-commit).
- **FM2 soundness preserved.** Spawn-failure → `Ok(detail:"failed to spawn")` → `Indeterminate` →
  fail-closed (`verify.rs:478-488`, `tdd.rs:316-321`). The two re-pointed guards exercise that genuine
  "never executed" path; `replay_rejects_vacuous_test` (real `cargo test`, exit 0 at base) still
  Fail("vacuous"). The positive test FAILS pre-fix (exit 2) / PASSES post-fix (exit 0), independently
  reproduced.
- All three replay-gating `is_command_allowed` sites consume the effective list (471/667/981); a repo-wide
  grep found no other replay-command reader of the raw field.

## Blocking findings
None.

## Non-blocking notes (all consciously accepted)

- **Minor (threat-model), FIXED at this HEAD:** test #9's comment called `false`'s nonzero exit a
  "genuine red," but `false` exits nonzero unconditionally — a degenerate always-red stand-in that proves
  the command *runs* (locking the wiring), not that a test was exercised. Comment tightened + a NOTE added
  pointing at `replay_rejects_vacuous_test` / `replay_accepts_genuine_red_first` as the genuine-red
  contract. Non-executable; the test's logic and assertions are unchanged.
- **Nit (correctness + simplicity):** the legacy site (`verify.rs:981`) recomputes
  `effective_allowed_prefixes()` per loop iteration, whereas the static-analysis site hoists it
  (`verify.rs:659`). Both correct (`cfg` is immutable); the allowlist is ~7 entries and dwarfed by the
  child spawn that follows. Accepted as-is per both reviewers ("leave as-is / defer if churn unwanted") —
  a micro-alloc consistency nit, not worth re-bind churn.
- **Nit (correctness):** the legacy path checks the effective list twice (skip-decision at 981, hard
  reject inside `execute_step` at 471); both use the identical list so they cannot diverge. Pre-existing
  shape, not a regression.
- **Nit (simplicity):** test #6 (`effective_unblocks_via_is_command_allowed`) shares fixture setup with
  test #1 but adds the load-bearing raw-vs-effective before/after contrast through the real matcher —
  intentional, earns its place.

## Criteria checked
### Spec/plan
- **P0 scope honored** (auto-include configured test commands; both replay gates via `execute_step`; no
  new config key; protected files untouched): PASS — diff is exactly `config.rs` + 3 `verify.rs` swaps +
  tests.
- **Hardening-safe (add-only, no new capability, scanner-orthogonal):** PASS — independently verified by
  the threat-model lens against `rules.toml` protected_paths and `verify.rs`.
- **FM2 interaction resolved per maintainer decision** (re-point guards to spawn-failure + positive test):
  PASS — guards exercise the genuine never-executed path; positive test locks the wiring (fails pre-fix).
- **Reproduce-then-resolve (verify gate):** PASS — swap-revert shows exit 2 → exit 0.

### Standards
- **Correctness:** PASS — union/dedupe/empty-skip correct; all 3 sites switched; hoist semantics-preserving;
  token-boundary match intact for multi-word commands.
- **Simplicity:** PASS — minimal, idiomatic, add-only; no speculative surface (falsified fmt/lint/globs
  knobs correctly absent); tests proportionate.
- **Threat-model / soundness:** PASS — no floor loosening, FM2 preserved, no false-pass path beyond the
  documented cargo-equivalent residual (a runnable command that errors at base is read as red — same trust
  already extended to `cargo test`; within "mistakes, not adversaries").
- **Tests / lint:** PASS — 592 passed / 0 failed; `fmt --check` clean; `clippy --all-targets -D warnings`
  clean.
