# Design: TDD red-green replay engine (Phase 15)

- **Date:** 2026-06-13
- **Feature slug:** tdd-replay
- **Status:** approved
- **Research:** [docs/research/2026-06-13-tdd-replay.md](../research/2026-06-13-tdd-replay.md) · ROADMAP Phase 15 · plan `2026-06-11-five-failure-modes-roadmap.md:111`

## Problem

The TDD gate (`tdd.rs:172-273`) is a **commit-sequence heuristic**: it confirms a test-only commit
precedes the first production commit, but never *runs* the test. The demonstrated FM2 hole: an
`assert!(true)` committed before the code satisfies the sequence and passes the gate. A test that
asserts nothing certifies nothing. We want the gate to *execute* the new test where the production
code does not yet exist and require it to fail **red** there — a vacuous test is green at the
merge-base, so the gate rejects it.

Success: `tests/cli_hollow.rs::hollow_c_assert_true_red_commit()` (today `#[ignore]`'d) un-ignored and
green, i.e. the gate rejects the `assert!(true)` choreography; a genuine red-first test still passes.

## Constraints

- **Shadow-first (Track 3 doctrine).** Default `[tdd] mode = "history"` preserves today's behavior
  exactly; `mode = "replay"` is opt-in. The default→enforce *flip* is **out of scope** for this phase
  — it is gated on Phase 14 burn-in (<2% false-block, ≥50 evals) and flips on data, not the calendar.
  This phase builds the engine and ships it shadow-logging.
- **Reuse, don't reinvent.** Classifier (`tdd.rs:24-117`), merge-base + commit walk (`tdd.rs:132-197`),
  shadow machinery (`verify.rs:100-206`), and the test-command execution path are reused. Net-new code
  is bounded to the worktree lifecycle + `[tdd]` config + the shadow decision wiring.
- **Offline, no new deps** (four-dep constraint). Worktrees via the `git` CLI, as the rest of the gate
  already shells to git.
- **Three-language-lanes.** All enforcement is Rust; no logic added to Bash hooks.
- **Non-goals (explicitly NOT doing):** (1) the default enforce flip; (2) hook/pre-commit wiring — the
  gate stays CLI-check-only as today; (3) detecting compile-error-red vacuous tests — that residual is
  documented and carried to Phase 17 mutation testing; (4) test *quality* beyond red-at-base.

## Approaches considered

1. **Worktree replay (chosen).** `git worktree add --detach <tmp> B`; `git -C <tmp> checkout T -- <test
   paths from T's diff>`; run the resolved test command in `<tmp>` with a timeout; require nonzero exit.
   Trade-offs: real execution gives a true red-green signal and generalizes across languages; cost is one
   worktree + one test run per check (≈ seconds for this repo; `[tdd] replay_test_command` scopes slow
   suites). Fully reversible via `mode = "history"`. Worktree-leak risk is mitigated by an RAII cleanup
   guard + a `doctor` orphan probe.
2. **In-place checkout / stash.** Check out `B` in the working tree, run, restore. Rejected: mutates the
   user's working tree, is unsafe with uncommitted changes, and is not concurrent-safe. A worktree is the
   purpose-built isolation primitive.
3. **Static analysis of the test body** (pattern-match `assert!(true)`). Rejected: brittle,
   language-specific, an endless arms race, and contrary to the plan's explicit principle — *execute*
   claims, don't grep for them.

## Decision

**Approach 1**, structured to mirror the verify gate's shadow-first shape exactly.

**Algorithm:**
1. Resolve merge-base `B` (from `--base`, else `cfg.base_branch`, else `main`) — reuse `tdd.rs:187-197`.
2. Walk `B..HEAD`; find the first test-only commit `T` and the test paths in `T`'s diff — reuse the
   classifier + `parse_log_output`. The existing sequence heuristic runs unchanged and its FAILs
   (no red commit, production-before-test) are preserved.
3. **`mode = "history"` (default):** run the heuristic only. If a replay *would* change the verdict,
   emit a `SHADOW` line (log-only) — never alters the exit code.
4. **`mode = "replay"`:** after the heuristic passes, run the replay. Create the worktree, check out
   `T`'s test paths onto `B`, run the resolved test command with the timeout, and require **nonzero
   exit (red)**. Green at base → FAIL (`vacuous test: passed at merge-base`). Clean up the worktree
   unconditionally.
5. **Shadow fields:** `gate="tdd"`, `check="replay"`, `configured` per the verify pattern
   (`Default`/`ShadowEnv` in history, `On` in replay), `result` ∈ {pass,fail,skip,static}, with the
   command and a one-line detail.

**Config (`[tdd]` in `<artifacts_root>/config.toml`):**
- `mode = "history" | "replay"` — default `history`.
- `replay_test_command` — optional; falls back to the top-level `test_command`.
- Timeout: **reuse the existing `replay_timeout_secs`** (no second knob — simplicity).

**Fail-closed edge:** `mode = "replay"` with no resolvable test command → exit 2 with
`replay mode requires a test_command` (you cannot prove red without running). In `history` mode,
absence simply means no shadow replay is attempted.

## Risks & open questions

- **Compile-error-red soft spot.** A test red-at-base only because it references a not-yet-existing API
  (compile error, not assertion failure) passes replay while asserting nothing. Documented, carried to
  Phase 17. Stated, not engineered against.
- **Worktree leaks.** Mitigated by an RAII guard that removes the worktree on every exit path (including
  panic/early-return) + a `doctor` probe that warns on orphaned `gatekeeper-replay-*` worktrees.
- **Test imports pre-existing production code.** If `T`'s test compiles and passes at `B` because the
  behavior already existed, the gate FAILs — which is correct (the test wasn't red-first).
- **Wall-clock.** Full suite at base per check; acceptable here. `replay_test_command` scopes it.

## Acceptance criteria

- `hollow_c_assert_true_red_commit()` un-ignored and **green** under `mode = "replay"` (gate exit ≠ 0
  on the `assert!(true)` fixture).
- A genuine red-first test (real assertion on not-yet-built behavior) replays **red** at base → gate
  **PASS** under `mode = "replay"`.
- `mode = "history"` (default) reproduces today's behavior — all existing `tdd.rs` gate tests stay green.
- No `gatekeeper-replay-*` worktree remains after any run (success, fail, or panic).
- `doctor` warns on an orphaned replay worktree.
- One `SHADOW` line per replay-relevant check with the seven pinned fields; shadow mode never changes
  the exit code.
- `mode = "replay"` with no test command → exit 2, clear message.
- `cargo test` / `clippy` / `fmt` green; **ADR-0017** records the decision (roadmap's "ADR-0016" is a
  stale forward-reference — 0016 is contract-split).
