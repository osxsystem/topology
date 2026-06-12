# Research — TDD red-green replay engine (Phase 15)

- **Date:** 2026-06-13 · **Feature slug:** `tdd-replay`
- **Source of truth:** `docs/plans/2026-06-11-five-failure-modes-roadmap.md:111` (FM2 center of gravity) and `docs/ROADMAP.md` Phase 15.
- **Method:** fan-out code map by a research subagent (grep/Read primary; context-engine index may be wiped). Every claim below carries a `file:line` citation; the top-risk claims are cross-checked against the tree.

## The problem (one line)

The TDD gate today is a **commit-sequence heuristic**: it checks that a test-only commit precedes the first production commit (`tdd.rs:172-273`), but it never *runs* the test. An `assert!(true)` test committed before the code satisfies the sequence and passes — the demonstrated FM2 hole. The fix: replay the new test at the merge-base and require it to fail **red** there. A vacuous test (`assert!(true)`) is green at base → the gate rejects it.

## Sub-questions and findings

### Q1 — What already exists and is directly reusable?

- **Path classifier** — `tdd.rs:24-117`: `is_test_path()` (8 conventions: `tests/`,`test/`,`__tests__/`,`spec/` dirs; `*_test.*`,`*Test*`,`*.test.*`,`*.spec.*`,`test_*.py`), `is_artifact_path()` (`docs/`,`.claude/`,`.github/`,`*.md`,`.gitignore`), `classify() -> (test, production)`. **Reuse verbatim** to extract the test paths from commit `T`'s diff.
- **Merge-base + commit walk** — `tdd.rs:187-197` resolves merge-base from `--base` (default `main`); `parse_log_output()` (`tdd.rs:132-167`) walks `merge_base..HEAD` and already tags each commit test/prod-touching. **Reuse** to find the first test-only commit `T` and the test paths it introduced.
- **Dispatch entry** — `main.rs:124` (`check tdd`, flags `--feature`/`--base`, handler `handle_check_tdd` at `main.rs:837-849`). No table change needed.
- **Shadow-first machinery** — `verify.rs:152-206` `emit_shadow(gate, check, configured, artifact, command, result, detail)`; `ShadowConfigured{Default,Off,On,ShadowEnv}` (`verify.rs:100-119`); `ShadowResult{Pass,Fail,Skip,Static}` (`verify.rs:122-139`); seven pinned JSON fields → stderr `SHADOW {…}` + best-effort append to `<artifacts_root>/logs/shadow.jsonl` (`verify.rs:198,204`, fail-silent). Verify's decision logic (presence default → `Default`/`ShadowEnv`; replay → `On`; shadow verdict logged but never demotes the exit code) is the exact template to mirror.
- **Config plumbing** — `config.rs:55-88` `ProjectConfig` with per-gate fields; `[verify]` parse at `config.rs:240-261` is the pattern; `load_result()` returns `Missing|ParseFailed|Ok` and gates exit 2 on `ParseFailed` (`main.rs:806-818`). Config file: `<artifacts_root>/config.toml` (framework: `docs/config.toml`).
- **Red-first fixture already written** — `tests/cli_hollow.rs` `hollow_c_assert_true_red_commit()` (lines ~151-212), currently `#[ignore = "red until Phase 15 red-green replay checks test quality"]`. Builds a git repo: base on `main`, feature commit 1 = `tests/hollow_c_test.rs` with `assert!(true)`, commit 2 = `src/hollow_c.rs`; runs `check tdd --base <sha>` and asserts exit ≠ 0. **This is the TDD-gate test I watch fail, then un-ignore.** Harness helpers `scratch_root/run/git/head_sha` (`cli_hollow.rs:23-66`) are reusable.

### Q2 — What must be built new?

- **Worktree create + cleanup** — no `git worktree add/remove` exists anywhere in `gatekeeper/src` (confirmed by sweep). Must build: `git worktree add <tmp> <base>`, `git -C <tmp> checkout T -- <test paths>`, run the test command, **unconditional cleanup** (RAII guard / drop), and a `doctor` probe for orphaned `gatekeeper-replay-*` worktrees (`doctor.rs` has no such probe today).
- **Test-command execution with timeout** — verify.rs already executes allowlisted commands with `replay_timeout_secs`; reuse that execution helper rather than inventing a second one (three-language-lanes: one execution path).
- **`[tdd]` config** — add `tdd_mode: TddMode{History(default),Replay}`, and timeout/test-command resolution (default: reuse top-level `test_command` + `replay_timeout_secs`; allow `[tdd]` overrides).
- **ADR** — next free number is **0017** (`0016-contract-split.md` is the highest; the roadmap's "ADR-0016" reference at plan:111 is a stale forward-reference and must be corrected to 0017).

### Q3 — Constraints & risks (from the canonical plan)

- **Replay must ship shadow-first** (`mode = "history"` default, log-only `replay` verdict) and flip on burn-in data, never the calendar — Track 3 deployment doctrine (`ROADMAP.md` Track 3 intro). Phase 15's default *flip* is gated on Phase 14 burn-in (<2% false-block, ≥50 evals); the **engine itself is buildable now**, the flip is not part of this phase.
- **Documented soft spot** (plan:111): a test red-at-base via *compile error* (references a not-yet-existing API) passes replay while asserting nothing — carried to Phase 17 mutation testing. Must be stated, not hidden.
- **Worktree-leak risk** (plan:125): cleanup guard + `doctor` orphan detection are required, not optional.
- **Replay wall-clock**: full suite at base per check; acceptable here (`cargo test` ≈ seconds). `[tdd] replay_test_command` allows a scoped runner for slow suites. Kill switch: `mode = "history"`.

### Q4 — Enforcement surface

The TDD gate is **CLI-check-only** today — not wired into any hook or pre-commit (`scan.rs` protected-path list is gate-agnostic; only `check finish` blocks). This phase keeps it CLI-only (+ shadow log); no new hook wiring. Hook enforcement is out of scope.

## Open decisions carried into design

1. **Config inheritance** — does `[tdd]` reuse top-level `test_command` + `[verify] replay_timeout_secs`, or own its keys? (lean: reuse with optional override.)
2. **Default `[tdd] mode`** — `history` (shadow-first) per doctrine; the enforce flip is explicitly deferred to a later burn-in step.
3. **What "red at base" requires** — nonzero exit of the replayed test command at `B` with only `T`'s test files checked out. Compile-error-red is accepted-but-documented (Phase 17 target), not engineered against here.

## Readiness

Classifier, merge-base/commit-walk, dispatch, config pattern, shadow machinery, and the red-first fixture **all exist**. New work is bounded: worktree lifecycle (create/checkout/run/cleanup), `[tdd]` config, the shadow decision wiring, a `doctor` orphan probe, and ADR-0017. No architectural unknowns remain.
