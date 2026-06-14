# Verify: Replay-allowlist portability fix (slice #3, P0)

- **Date:** 2026-06-14
- **Feature slug:** hardened-config-layer
- **Plan:** `docs/plans/2026-06-14-hardened-config-layer.md`

## Symptom (reproduced)

`default_allowed_prefixes` (`config.rs:180-190`) is cargo/just-centric. A non-Rust user who enables
replay mode and configures their own test command has it **rejected** by `is_command_allowed`
(`verify.rs:471`), routed through to both verify-replay and TDD-replay (`tdd.rs:307`) → `Err` →
`Indeterminate` → fail-closed. They told gatekeeper their test command, and it refuses to run it.

**Reproduced** via a temporary revert of the `execute_step` swap (back to `&cfg.allowed_command_prefixes`),
running the new positive test with `test_command = "false"` (a real command not in the default allowlist):

```
test replay_autoincluded_command_runs_and_establishes_red ... FAILED
  got exit 2; out: PASS tdd gate: failing-test-first history confirmed
  (i.e. `false` was rejected → Indeterminate → exit 2)
```

## Resolution

`ProjectConfig::effective_allowed_prefixes()` extends the allowlist with the configured `test_command` /
`tdd_replay_test_command` (add-only, deduped, empties skipped). The three `is_command_allowed` sites in
`verify.rs` (471/666/980) consume it. After the fix, the same test:

```
test replay_autoincluded_command_runs_and_establishes_red ... ok
  (false auto-included → RUNS → nonzero = red at base → Pass → exit 0, SHADOW result:"pass")
```

So the temporary revert proves the test FAILS pre-fix (exit 2) and PASSES post-fix (exit 0) — the wiring
is genuinely exercised, not vacuous.

### Behavioral matrix (verified)

| Scenario | Behavior | Evidence |
|---|---|---|
| Configured non-default command, runnable (`false`) | **runs → red → PASS** (was: rejected → exit 2) | `replay_autoincluded_command_runs_and_establishes_red` |
| No `test_command` configured | effective list == `allowed_command_prefixes` (identity) | `effective_is_identity_when_unset` |
| `test_command` already a default (`cargo test`) | appears once (deduped) | `effective_dedupes_existing` |
| Empty/whitespace `test_command` | not added | `effective_skips_empty` |
| Allowlist accepts configured cmd via effective list | `is_command_allowed(["swift","test"], effective)`=true; vs raw=false | `effective_unblocks_via_is_command_allowed` |

### FM2 soundness preserved (the key check)

The replay-allowlist rejection was also an FM2 backstop. After the fix, the "command never established
red → fail closed" property is guarded via the **spawn-failure** path (a binary that cannot launch →
`Ok(detail:"failed to spawn")` → `Indeterminate` → fail-closed). Both re-pointed guards pass:

```
test replay_unrunnable_command_fails_closed ... ok          (exit 2, fail-closed)
test history_unrunnable_command_logs_skip_not_pass ... ok    (SHADOW result:"skip", not "pass")
```

## Gate evidence

- **TDD:** 6 config unit tests + 1 positive integration test written/observed against the implementation;
  the positive test proven to fail pre-fix (temporary revert) and pass post-fix.
- **FM2:** two soundness guards re-pointed to the spawn-failure path (more faithful than the old
  `make test`), still green.
- **fmt:** `cargo fmt -- --check` clean. **clippy:** `--all-targets -D warnings` clean.
- **Full suite:** 592 passed / 0 failed (baseline 585; +6 config unit, +1 positive integration; the 2
  FM2 tests renamed, net 0).
- **No protected files touched:** `config.rs` + `verify.rs` + `cli_tdd_replay.rs` are all unprotected; no
  `--no-verify` required.
