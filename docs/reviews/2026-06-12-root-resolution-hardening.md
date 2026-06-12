VERDICT: pass
HEAD: 64b08c7c6338c7b2ff9b6e82732782f92be1eb4d
BASE: 831aa03cd5f67eac14c90c2e111b607c40a27f5f

# Code review — root-resolution-hardening (Phase 11, v0.6.0)

Branch: `feat/root-resolution-hardening`, reviewed 2026-06-12.
Reviewer: orchestrator pass (Fable 5 main loop) over the delegated implementation
(Sonnet subagent), per the standing review focus on fabricated interfaces and
overclaimed guarantees.

## Blocking findings

None.

## Criteria checked

### Spec/plan

Spec `docs/specs/2026-06-12-root-resolution-hardening.md`, plan
`docs/plans/2026-06-12-root-resolution-hardening.md`:

- Precedence chain implemented exactly as specced: env override → self-governed project →
  binary-adjacent (`current_exe` walk) → `<project>/.topology` → `~/.topology` → cwd
  fallback. The bare cwd marker walk and per-ancestor `.topology` probe are gone from
  `resolve_root`; the only cwd influence left is the nearest-`.git` project walk and the
  identity fallback. ✔
- Pure-function refactor: `resolve_root(start, env_override, exe_path, home)` reads no
  process state; provenance carried via `RootSource`/`ResolvedRoot`; `framework_root()` /
  `resolved_root()` are thin wrappers. Ten tempdir unit tests cover spec AC 1–7. ✔
- Doctor: `resolved by:` line; F1 (unmarked resolved root) and F2 (project == framework ∧
  `VERSION` present) exit non-zero; dev checkout (no `VERSION`) stays 0; `version_skew`
  logic and tests untouched. ✔
- Five `#[ignore]`-then-un-ignored integration fixtures; tdd gate confirms the test-only
  red commit `005390f` precedes all production commits. ✔
- Version 0.6.0 bumped in **all four** files (Cargo.toml, Cargo.lock, plugin.json,
  marketplace.json) — the v0.5.1 release-guard failure mode is closed. ✔
- Both ROADMAP deviations (`$TOPOLOGY_ROOT` kept first; new self-governed step) are
  recorded in the spec and flagged in the PR description for maintainer ratification. ✔

### Standards

- `just check` green at the verify commit: fmt-check, clippy `-D warnings`, 460 passed /
  6 ignored (up from 452/6 on main), shellcheck, typos, docs lint. ✔
- Verify gate static PASS and full `GATEKEEPER_SHADOW=replay` PASS (4/4 evidence commands
  executed green). ✔
- Delegated-output review findings, fixed in follow-up commits before this review:
  1. **Spec AC-7 violation** (`251e5e6`): the fallback warning printed 3× per doctor run
     (handler duplicate + two un-guarded `framework_root()` calls) while the report claimed
     "no deviations" and the fixture only asserted `contains()`. Fixed with a `Once` guard,
     handler de-duplication, and an exact-count assertion.
  2. **Contradictory F1 comment** (`2ad3c34`): comment claimed env pins were exempt from
     the marker check; the code (correctly, per spec) exempts nothing.
- Accepted trade-off: integration helpers across seven test files now pin
  `TOPOLOGY_ROOT=canonicalize(cwd)` so the dev repo can't win via binary-adjacent in
  scratch fixtures; the unpinned paths are exercised by the dedicated
  `cli_root_resolution` fixtures, which install the binary into isolated layouts. No
  assertions were weakened (diff-audited). ✔
- Protected-path commits carry the documented `--no-verify` override line; no new
  dependencies (ADR-0007). ✔
