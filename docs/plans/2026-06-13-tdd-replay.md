# Plan: TDD red-green replay engine (Phase 15)

- **Date:** 2026-06-13 · **Feature slug:** tdd-replay
- **Design:** [docs/specs/2026-06-13-tdd-replay.md](../specs/2026-06-13-tdd-replay.md) (approved)
- **Research:** [docs/research/2026-06-13-tdd-replay.md](../research/2026-06-13-tdd-replay.md)

## Baseline (clean)

`env -u GATEKEEPER_BIN -u TOPOLOGY_ROOT cargo test --release` → **285 passed, 0 failed** (4 ignored:
the `#[ignore]` hollow fixtures + 2 lib). The env scrub is mandatory locally: a stale inherited
`GATEKEEPER_BIN` (pointing at the deleted `topology-phase12/bin/gatekeeper`) makes the `cli_doctor`
probe exit 1. CI has no such var; this is a local-shell artifact, not code. **Every test command in
this plan is prefixed `env -u GATEKEEPER_BIN -u TOPOLOGY_ROOT`.**

## Files to touch

| File | Responsibility |
|------|----------------|
| `gatekeeper/src/config.rs` | Add `[tdd]` config: `TddMode{History,Replay}`, `tdd_mode`, `tdd_replay_test_command`. |
| `gatekeeper/src/tdd.rs` | The replay engine: extract test paths from commit `T`, worktree at `B`, checkout, run, require red. RAII cleanup. Mode-gated wiring into `gate_tdd`. |
| `gatekeeper/src/main.rs` | `handle_check_tdd`: strict config load (exit 2 on `ParseFailed`), pass `cfg` through. |
| `gatekeeper/src/doctor.rs` | Probe: warn on orphaned `gatekeeper-replay-*` worktrees; warn if `mode=replay` but no test command. |
| `gatekeeper/tests/cli_hollow.rs` | Un-ignore `hollow_c_assert_true_red_commit`; make its scratch project set `[tdd] mode=replay`. |
| `gatekeeper/tests/cli_tdd_replay.rs` | NEW: genuine-red-first test passes replay; vacuous test fails; history-mode unchanged; cleanup. |
| `docs/adr/0017-tdd-red-green-replay.md` | NEW ADR. |
| `docs/ROADMAP.md`, `CHANGELOG.md` | Status row + Unreleased note. |

## Conventions

- Shell to `git` via `std::process::Command` as `tdd.rs` already does (`tdd.rs:187-197`). No new crates.
- Mirror `verify.rs` for shadow: `emit_shadow("tdd","replay",configured,artifact,Some(cmd),result,detail)`;
  `configured` = `Default` (history, no env), `ShadowEnv` (history + `GATEKEEPER_SHADOW=replay`), `On`
  (replay mode). Shadow verdict never changes the exit code.
- Worktree path: `std::env::temp_dir().join(format!("gatekeeper-replay-{feature}-{}", std::process::id()))`.
- RAII: a `struct ReplayWorktree(PathBuf)` whose `Drop` runs `git worktree remove --force <path>` (and
  `git worktree prune`), so every exit path (return, `?`, panic) cleans up.

## Tasks (TDD order — test first, watch red, implement, watch green)

### Task 1 — `[tdd]` config parsing
- **Test (test-engineer-tdd), `config.rs` tests module:** `tdd_mode_replay_parsed` (TOML `[tdd]\nmode="replay"`
  → `tdd_mode==TddMode::Replay`); `tdd_mode_invalid_returns_error` (`mode="bogus"` → `Err`);
  `tdd_mode_defaults_history` (absent → `History`); `tdd_replay_test_command_parsed`.
- **Watch red:** `env -u GATEKEEPER_BIN -u TOPOLOGY_ROOT cargo test --release --lib config` → new tests fail to compile/assert.
- **Impl (feature-implementer), `config.rs`:** add `pub enum TddMode { History, Replay }` (with
  `Default=History`, `from_str` accepting `"history"|"replay"`, else `Err`); fields `pub tdd_mode: TddMode`
  and `pub tdd_replay_test_command: Option<String>` on `ProjectConfig`; defaults in the `Default` impl;
  parse a `[tdd]` table (mirror the `[verify]` block at `config.rs:240-261`), strict on bad `mode`.
- **Green + commit:** `…cargo test --release --lib config` green. Commit `feat(tdd): [tdd] mode config (history|replay)`.

### Task 2 — replay engine (red at base) + RAII cleanup
- **Test (test-engineer-tdd), NEW `gatekeeper/tests/cli_tdd_replay.rs`** (reuse the `cli_hollow.rs:23-66`
  git-scratch harness — `scratch_root/run/git/head_sha`):
  - `replay_rejects_vacuous_test`: repo with base on `main`; feature commit 1 = `tests/x_test.rs` with
    `#[test] fn x(){ assert!(true); }`, commit 2 = `src/x.rs`; scratch `docs/config.toml` = `[tdd]\nmode="replay"`
    and top-level `test_command="cargo test"` (the scratch project is a tiny cargo crate so the command runs);
    `check tdd --feature x --base <base>` → exit ≠ 0, stderr/stdout names `merge-base`/`vacuous`.
  - `replay_accepts_genuine_red_first`: commit 1 = a test asserting on a function `answer()` that does NOT
    exist at base (so it fails to compile/asserts red at base), commit 2 adds `pub fn answer()->i32{42}`;
    `mode="replay"` → gate exit 0.
  - `history_mode_skips_replay`: same vacuous repo but `mode="history"` (or no config) → exit 0 (today's
    behavior preserved), and a `SHADOW` line with `"gate":"tdd","check":"replay"` is emitted on stderr.
  - `replay_cleans_up_worktree`: after a replay run, assert no `gatekeeper-replay-*` dir remains in temp.
- **Watch red:** `env -u GATEKEEPER_BIN -u TOPOLOGY_ROOT cargo test --release --test cli_tdd_replay` → all fail (engine absent).
- **Impl (feature-implementer), `tdd.rs`:** add `fn test_paths_in_commit(git_root,&sha)->Vec<String>`
  (run `git show --name-only --format= <sha>`, keep paths where `classify().0`); add `struct ReplayWorktree`
  with `Drop`; add `fn replay_red_green(git_root,base,commit_t,&[test_paths],test_cmd,timeout)->ReplayOutcome`
  (worktree add `--detach <tmp> <base>`; `git -C <tmp> checkout <commit_t> -- <paths>`; run `test_cmd` in
  `<tmp>` with `timeout`; red ⟺ nonzero exit ⟹ `Pass`, zero exit ⟹ `Fail("vacuous test: passed at merge-base")`);
  extend `gate_tdd` to accept `cfg: &ProjectConfig`, and after the existing heuristic passes, branch on
  `cfg.tdd_mode`: `Replay` → run replay and use its verdict; `History` → keep heuristic verdict, and if a
  replay *would* differ, emit a shadow line.
- **Green + commit:** `…cargo test --release --test cli_tdd_replay` green. Commit `feat(tdd): worktree red-green replay engine`.

### Task 3 — shadow wiring (exact 7-field parity)
- **Test (test-engineer-tdd), `cli_tdd_replay.rs`:** `shadow_line_has_seven_fields` — in history mode on the
  vacuous repo, capture stderr, assert one `SHADOW ` line whose JSON has exactly keys
  `gate,check,configured,artifact,command,result,detail` with `gate=="tdd"`, `check=="replay"`.
- **Watch red → impl (feature-implementer):** route all replay verdicts through `verify::emit_shadow` with
  `configured` per the design; in `Replay` mode `configured=On`; in `History` `Default` (or `ShadowEnv` when
  `GATEKEEPER_SHADOW=replay`). **Green + commit** `feat(tdd): shadow-log replay verdicts (7-field parity)`.

### Task 4 — strict config in the handler
- **Test (test-engineer-tdd), `cli_hollow.rs` or `cli_tdd_replay.rs`:** `replay_mode_without_test_command_exits_2`
  (`[tdd] mode="replay"`, no `test_command` anywhere) → exit 2 with message `replay mode requires a test_command`;
  `tdd_parse_failed_exits_2` (malformed `config.toml`) → exit 2.
- **Watch red → impl (feature-implementer), `main.rs:837-849`:** switch `handle_check_tdd` to
  `config::ProjectConfig::load_result(&artifacts_root())`, exit 2 on `ParseFailed` (mirror `main.rs:806-818`),
  pass `cfg` to `gate_tdd`; in `tdd.rs`, `Replay` + no resolvable command → return 2 with the message.
- **Green + commit** `fix(tdd): strict config load + fail-closed replay without test_command`.

### Task 5 — doctor orphan probe
- **Test (test-engineer-tdd), `cli_doctor.rs`:** `doctor_warns_on_orphaned_replay_worktree` — pre-create a
  `gatekeeper-replay-*` dir in temp, run `doctor`, assert an informational line names it (exit unaffected:
  informational, not a probe failure).
- **Watch red → impl (feature-implementer), `doctor.rs`:** add a probe scanning `std::env::temp_dir()` for
  `gatekeeper-replay-*` entries, printing `replay worktrees: <n> orphaned (informational)` (0 → `ok`).
  **Green + commit** `feat(doctor): warn on orphaned replay worktrees`.

### Task 6 — un-ignore the hollow_c acceptance fixture
- **Impl (feature-implementer), `cli_hollow.rs`:** remove the `#[ignore]` on `hollow_c_assert_true_red_commit`;
  ensure its scratch project writes `docs/config.toml` with `[tdd]\nmode="replay"` and a runnable
  `test_command`, so replay actually fires. (If the fixture's repo isn't a cargo crate, give it a minimal
  `Cargo.toml`+`src/lib.rs` so `cargo test` runs — match `replay_rejects_vacuous_test`.)
- **Green:** `env -u GATEKEEPER_BIN -u TOPOLOGY_ROOT cargo test --release --test cli_hollow hollow_c` green
  (gate rejects `assert!(true)`). Commit `test(tdd): un-ignore hollow_c — replay kills assert!(true)`.

### Task 7 — ADR-0017 + docs
- **Impl (main loop):** write `docs/adr/0017-tdd-red-green-replay.md` (Status: Accepted; the worktree-replay
  decision, the shadow-first rollout, the documented compile-error-red soft spot → Phase 17); add a Phase 15
  status note to `docs/ROADMAP.md` (engine delivered, flip deferred) and a `CHANGELOG.md` Unreleased entry.
  Commit `docs(adr): ADR-0017 TDD red-green replay; ROADMAP/CHANGELOG`.

## Done when

`env -u GATEKEEPER_BIN -u TOPOLOGY_ROOT cargo test --release` all green with `hollow_c` no longer ignored;
`cargo clippy --release -- -D warnings` and `cargo fmt --check` clean; verify artifact records the
red→green replay evidence; fresh-context review passes bound to HEAD; full suite green at the finish gate.
