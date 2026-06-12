# 0017 — TDD red-green replay: execute the new test at the merge-base, require red

- **Status:** 🟢 Accepted
- **Date:** 2026-06-13
- **Spec:** [tdd-replay](../specs/2026-06-13-tdd-replay.md) · ROADMAP Phase 15

## Context

The TDD gate's original check (`tdd.rs` heuristic) verifies only *commit sequence*: that a test-only
commit precedes the first production commit. It never executes the test. This is the demonstrated FM2
hole from the five-failure-modes audit — a commit of `#[test] fn x() { assert!(true); }` placed before
the production code satisfies the sequence and passes the gate, yet certifies nothing. The gate checked
the artifact's *shape*, not the system's *behavior*.

The ROADMAP labels this engine "ADR-0016", but 0016 was taken by the contract split; this is **0017**.

## Decision

Add a red-green **replay** that runs the new test where the production code does not yet exist and
requires it to fail there.

1. **`[tdd] mode = "history" | "replay"`** (default `history`) in `<artifacts_root>/config.toml`,
   parsed by `config.rs` mirroring `VerifyMode`. Optional `[tdd] replay_test_command` overrides the
   top-level `test_command`; the timeout reuses the existing `replay_timeout_secs` (no second knob).

2. **Replay algorithm** (`tdd.rs`), layered *after* the existing heuristic passes: resolve the
   merge-base `B` and the first test-only commit `T`; `git worktree add --detach <tmp> B`;
   `git -C <tmp> checkout T -- <T's test paths>`; run the test command in `<tmp>` via
   `verify::execute_step` (the single shared spawn/timeout path — no second executor). The verdict has
   three states: a command that **ran and exited nonzero ⟹ red at base ⟹ Pass**; **ran and exited zero
   ⟹ green at base ⟹ Fail** (`vacuous test: passed at merge-base`); anything that **could not establish
   red ⟹ Indeterminate** (see point 4). The `assert!(true)` choreography dies here: it compiles and
   passes at `B`, so the gate rejects it.

3. **Shadow-first rollout** (ADR-aligned with the verify gate). The replay verdict is emitted as one
   seven-field `SHADOW {gate:"tdd",check:"replay",…}` line. In `history` mode (default) the verdict is
   **logged only** and the heuristic verdict (0) is returned unchanged; in `replay` mode it is
   **enforced** (0 on red, 1 on vacuous). `configured` is `On` (replay), `ShadowEnv`
   (`GATEKEEPER_SHADOW=replay`), or `Default` (history). The default→enforce flip is **deferred** and
   gated on Phase 14 burn-in (<2% false-block, ≥50 evaluations) — never the calendar.

4. **Fail-closed on "cannot prove red" (`Indeterminate`).** A replay that cannot establish red is never
   treated as a pass. This covers: a test command that **could not run** (not in
   `allowed_command_prefixes`, a metachar/env-assignment prefix, or a spawn failure) and a **timeout**.
   In `replay` mode an `Indeterminate` verdict **fails closed → exit 2** (`cannot prove red (replay
   indeterminate)`); in `history` mode it is recorded as a `Skip` shadow line (never a phantom `pass`,
   so the burn-in log stays honest). Likewise `mode = "replay"` with no resolvable test command → exit 2
   (`replay mode requires a test_command`), and a malformed `config.toml` (e.g. bad `[tdd] mode`) →
   `ParseFailed` → exit 2 in the handler (strict `load_result`).

5. **Worktree hygiene.** An RAII `ReplayWorktree` guard removes the worktree (`git worktree remove
   --force` + directory removal + `git worktree prune`) on every exit path including panic. Worktrees
   nest under `temp_dir()/gatekeeper-replay/<feature>-<pid>` (not a flat `gatekeeper-replay-*`) so a
   sibling process's in-flight worktree never matches a cleanup scan of the parent. `doctor` gained an
   informational probe that reports orphaned replay worktrees.

## Consequences

- The TDD gate now has teeth: a test that cannot fail is rejected (in replay mode), and the would-be
  verdict is logged in history mode so burn-in data accrues toward the flip decision.
- **Documented soft spot (carried to Phase 17 mutation testing):** a test red-at-base only via a
  *compile error* (it references a not-yet-existing API but asserts nothing) passes replay while
  certifying nothing. Replay catches the vacuous-green case (`assert!(true)`), not the
  vacuous-compile-red case. This residual is stated, not engineered against here.
- Replay runs the full test suite at the merge-base per check (≈ seconds for this repo);
  `replay_test_command` scopes it for slow suites. Kill switch: `mode = "history"`.
- The gate stays **CLI-check-only** — no new hook/pre-commit wiring in this phase.
- A test command that cannot run (not allowlisted, metachar/env-prefix, spawn failure) or times out is
  **Indeterminate**, not red — the gate fails closed (exit 2 in `replay` mode; a `Skip` shadow record in
  `history` mode) rather than certify a vacuous test or poison the burn-in log with a phantom pass.
  Trade-off: a genuinely slow legitimate test under `replay` mode exits 2 (raise `replay_timeout_secs`
  or scope `replay_test_command`) rather than passing on a timeout — the conservative, sound default.
