VERDICT: pass
HEAD: 6b54141d2321007862e00f4c576c8a4ec48b90bd
BASE: bb92ea2b4c3a3b8c3bdd8a7efef0ea332df40026

## Blocking findings

None.

## Criteria checked

### Spec/plan

- **FM2 hole closed (the originating goal).** Spec `docs/specs/2026-06-13-tdd-replay.md:17` and ADR-0017
  context (`docs/adr/0017-tdd-red-green-replay.md:9-13`) define success as the `assert!(true)`
  choreography being rejected. `gatekeeper/tests/cli_hollow.rs::hollow_c_assert_true_red_commit` is
  un-ignored and green: `cli_hollow.rs` ran `5 passed; 0 failed; 2 ignored` (only `hollow_d`/`hollow_f`
  remain ignored, carried to Phase 17 — matches verify artifact line 69).
- **Round-1 finding #1 closed (the `Err(_)=>Pass` soundness hole), re-verified by reading.**
  `tdd.rs:307-327`: `verify::execute_step` `Err(e)` maps to `ReplayVerdict::Indeterminate(e)` (line 310);
  the spawn/wait-failure `Ok(passed:false)` detail maps to `Indeterminate` (lines 317-321); the only
  `=> Pass` path (line 324) is the `else` reached solely for a command that RAN and exited nonzero.
  Guard tests pass: `replay_nonallowlisted_command_fails_closed` (replay mode, `make test` → exit 2,
  not a phantom pass) and `history_nonallowlisted_logs_skip_not_pass` (history mode → exit 0 with a
  `SHADOW` line whose `result` is asserted `skip`, NOT `pass`). `cli_tdd_replay.rs` ran `8 passed`.
- **Round-1 finding #2 closed (vacuous cleanup test).** `replay_cleans_up_worktree` and
  `replay_sleeping_step_times_out_and_no_orphan` green in the run; RAII guard at `tdd.rs:199-212`
  removes worktree + dir + prunes on every exit path.
- **Fail-closed-on-no-command + bad config (spec line 75, AC line 101).**
  `replay_mode_without_test_command_exits_2` and `tdd_parse_failed_exits_2` green.
- **Shadow-first / 7-field contract intact (spec lines 66, 99).** `ShadowResult` enum
  (`verify.rs:123-136`) carries Pass/Fail/Skip/Static; `shadow_lines_have_exact_field_set`
  (`cli_verify_replay.rs:339`) asserts the pinned field set and is green; history mode never alters the
  exit code (`shadow_env_replay_does_not_change_exit_code` green).
- **Scope surgical, no drive-by.** `git diff --stat bb92ea2 6b54141` touches only the Phase 15 surface:
  `tdd.rs`, `config.rs`, `doctor.rs`, `main.rs` (strict-load wiring), the new `cli_tdd_replay.rs`,
  `cli_doctor.rs`/`cli_hollow.rs` test additions, and the docs (ADR/spec/plan/research/verify/ROADMAP).
  No unrelated refactors.

### Standards

- **Doc coherence — the Round-2 failure — is resolved.** The reversed-doctrine phrases are gone from the
  reconciled docs: `grep -n "timeout ⟹ Pass\|lenient\|6 passed" docs/adr/0017-tdd-red-green-replay.md
  docs/verify/2026-06-13-tdd-replay.md` returned **0 matches** (exit 1). ADR decision point 2
  (`0017:29-33`) now states the three-state verdict (ran-nonzero⟹Pass / ran-zero⟹Fail /
  cannot-prove-red⟹Indeterminate); decision point 4 (`0017:42-49`) documents fail-closed-on-Indeterminate
  (exit 2 in replay, Skip in history); consequences (`0017:68-72`) replace the old "treated as red
  (lenient)... acceptable" with the Indeterminate/fail-closed trade-off. The verify artifact says
  `# expect: 8 passed` (`verify:33`) and documents both fail-closed cases (`verify:47-53`). Remaining
  "6 passed"/"lenient"/"treated as red" hits elsewhere in the repo are in *unrelated* prior docs
  (code-review-gate, security-scanning, distribution-payload) and in this review file's prior Round-2
  content (now overwritten) — none in the reconciled ADR/verify.
- **Spec/ADR/code/verify describe the same behavior.** Three-state verdict and fail-closed semantics
  match across `tdd.rs` (`ReplayVerdict`, `replay_after_heuristic` exit codes 0/1/2), ADR-0017 points
  2 & 4, and the verify evidence list. The spec predates the explicit `Indeterminate` naming but is
  consistent (its fail-closed-on-no-command rule and "execute claims, don't grep" principle); the ADR is
  the reconciled decision of record.
- **Suite + lints clean (`env -u GATEKEEPER_BIN -u TOPOLOGY_ROOT`).** `cargo test --release`: all suites
  green (bin unittests `270 passed; 2 ignored`; `cli_tdd_replay 8 passed`; `cli_verify_replay 21 passed`;
  `cli_hollow 5 passed; 2 ignored`; every other suite `0 failed`). `cargo clippy --release --bin
  gatekeeper -- -D warnings` clean; `cargo fmt --check` clean (exit 0).
- **Simplicity.** Timeout reuses the existing `replay_timeout_secs` (no second knob); the three-state
  enum is the minimal shape that distinguishes red / vacuous-green / cannot-prove-red. A staff engineer
  would not call this overcomplicated.
- **Documented residual (not a defect).** Compile-error-red vacuous tests pass replay; stated in spec
  risks and ADR consequences (`0017:61-64`), carried to Phase 17. The worktree-path deviation from the
  literal spec path is documented inline (`tdd.rs:251-259`) with a concurrency rationale.
