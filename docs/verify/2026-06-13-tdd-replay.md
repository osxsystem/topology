# Verify — TDD red-green replay engine (Phase 15)

- **Date:** 2026-06-13
- **Spec:** `docs/specs/2026-06-13-tdd-replay.md` · **Plan:** `docs/plans/2026-06-13-tdd-replay.md` ·
  **ADR:** `docs/adr/0017-tdd-red-green-replay.md`
- **Binary:** `gatekeeper 0.9.0`

## Symptom (before)

The TDD gate checked only commit *sequence* — a test-only commit before the first production commit —
never executing the test. FM2 demonstrated the hole: a commit of `#[test] fn x() { assert!(true); }`
before the production code satisfied the sequence and the gate returned **PASS**. The scoreboard
fixture `gatekeeper/tests/cli_hollow.rs::hollow_c_assert_true_red_commit` encoded exactly this and was
`#[ignore]`'d as a known gap (`HOLLOW PASS`).

## Resolution (after)

A red-green **replay** layered after the heuristic: in `[tdd] mode = "replay"`, the gate checks out
the first test-only commit's test files onto the merge-base in a detached worktree and runs the test
command there. A test that **passes at the base** (production code absent) is vacuous → the gate
**FAILS**. `assert!(true)` passes at the base, so it is now rejected. The default `mode = "history"`
preserves today's behavior and shadow-logs the would-be replay verdict for burn-in.

### Reproduce-then-resolve evidence

The vacuous test that *passed* before is now rejected, and a genuine red-first test is accepted —
both proven by the replay test suite, and the previously-ignored `hollow_c` scoreboard fixture is now
caught (no longer `#[ignore]`'d):

```evidence
$ cargo test --release --test cli_tdd_replay
# expect: 8 passed
```

```evidence
$ cargo test --release --test cli_hollow hollow_c
# expect: 1 passed
```

- **`replay_rejects_vacuous_test`** — `[tdd] mode="replay"`, `assert!(true)` test-only commit → gate
  exit ≠ 0, output names `vacuous`/`merge-base`. (Before: exit 0.)
- **`replay_accepts_genuine_red_first`** — a test asserting on a not-yet-existing API is red at base →
  gate exit 0.
- **`history_mode_skips_replay`** — same vacuous repo in `history` mode → exit 0 (unchanged) **and** a
  `SHADOW {"gate":"tdd","check":"replay",…}` line is emitted.
- **`replay_cleans_up_worktree`** — no `gatekeeper-replay/*` worktree remains after a run.
- **fail-closed (no command / bad config)** — `replay` mode without a `test_command` → exit 2; malformed
  `[tdd]` config → exit 2.
- **fail-closed (cannot prove red)** — `replay_nonallowlisted_command_fails_closed`: `replay` mode with a
  non-allowlisted `test_command` (e.g. `make test`) → exit 2 (`cannot prove red`), not a phantom pass.
  `history_nonallowlisted_logs_skip_not_pass`: same in `history` mode → exit 0 with a `SHADOW` line whose
  `result` is `skip`, never `pass` — the burn-in log is not poisoned. (Indeterminate covers
  not-allowlisted / metachar / env-prefix / spawn failure / timeout.)
- **`hollow_c_assert_true_red_commit`** — the FM2 scoreboard fixture, rebuilt as a real cargo crate
  with `mode=replay`, now **green** (the gate rejects the hollow test).

> Local note: all `cargo test` runs in this repo require `env -u GATEKEEPER_BIN -u TOPOLOGY_ROOT`
> prefixes — a stale inherited `GATEKEEPER_BIN` otherwise breaks the `cli_doctor` probe. CI has no such
> var. This is a local-shell artifact, not a code defect.

## Full suite + lints

```evidence
$ cargo test --release
# expect: test result: ok
```

`cargo clippy --release --bin gatekeeper -- -D warnings` clean; `cargo fmt --check` clean. The whole
suite is green with `hollow_c` no longer ignored (only `hollow_d`/`hollow_f` remain `#[ignore]`'d —
prose/plan substance, carried to Phase 17).

## Scope honesty (AC)

The Phase 15 diff (`bb92ea2..HEAD`) adds the replay engine to `tdd.rs`, the `[tdd]` config to
`config.rs`, the strict-load handler wiring in `main.rs`, the `doctor` orphan probe, the
`cli_tdd_replay.rs` suite, the `hollow_c` fixture rebuild, and the docs (ADR-0017, CHANGELOG, ROADMAP).
**Deferred, by design:** the default→enforce flip (gated on Phase 14 burn-in), hook/pre-commit wiring
(stays CLI-only), and compile-error-red detection (Phase 17). No version bump / release tag in this PR.

## Gate status

research ✓ · design ✓ (PASS) · plan ✓ (PASS, baseline 285 green) · tdd ✓ (every behavior watched red
before green; `hollow_c` un-ignored) · finish ✓ (full suite green) · clippy/fmt clean.
