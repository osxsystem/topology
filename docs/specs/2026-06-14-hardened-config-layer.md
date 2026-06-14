# Design: Replay-allowlist portability fix (slice #3, P0)

- **Date:** 2026-06-14
- **Feature slug:** hardened-config-layer
- **Status:** approved
- **Research:** `docs/research/2026-06-14-hardened-config-layer.md`

> **Scope decision (maintainer, via AskUserQuestion).** The research falsified most of the pitched
> config layer (`fmt`/`lint` have no consumer; `test_success_markers` duplicates
> `finish_extra_count_patterns`; `test_globs` parameterizes an already-polyglot classifier). The
> maintainer chose **"P0 replay-allowlist fix only"** — the one real, enforcement-backed monoculture tax.
> Approval recorded per the standing autonomy grant; this repo is `[design] approval = "status-line"`
> (provenance shadow).

## Problem

`default_allowed_prefixes` (`config.rs:180-190`) ships a cargo/just allowlist
(`cargo test`, `cargo run`, `just`, `git diff/log/show/status`). It fail-closes the replay command gate
`is_command_allowed` (`verify.rs:82`) at three sites — `execute_step` (`verify.rs:471`), evidence-block
static analysis (`verify.rs:666`), and the legacy path (`verify.rs:980`) — and the **TDD-replay** gate
routes through `execute_step` (`tdd.rs:307`). A non-Rust user who enables `[verify] mode=replay` or
`[tdd] mode=replay` has their test command (`swift test`, `xcodebuild`, `pytest`, …) **silently rejected
→ Indeterminate** (`verify.rs:310`) unless they discover and duplicate it into
`[verify] allowed_command_prefixes` — and the tdd replay command resolves to `tdd_replay_test_command`
or `test_command` (`tdd.rs:459-462`), so even the command they *configured* gets rejected. That is the
real monoculture tax: **you told gatekeeper your test command, and it then refuses to run it.**

## Decision

Add `ProjectConfig::effective_allowed_prefixes(&self) -> Vec<String>` — the replay allowlist **extended
with the project's own configured test commands** (`test_command` and `tdd_replay_test_command`, verbatim,
deduped, empties skipped). Replace the three `&cfg.allowed_command_prefixes` reads in `verify.rs`
(471/666/980) with `&cfg.effective_allowed_prefixes()`. Because tdd-replay routes through `execute_step`,
the single change at the `execute_step` check fixes **both** replay gates.

Behavior: a user who sets `test_command = "swift test"` (or `[tdd] replay_test_command`) gets it accepted
by the replay harness automatically — no separate allowlist entry needed. The token-boundary prefix
matcher (`verify.rs:82-96`) means `"swift test"` admits `swift test --filter Foo`, etc.

### Why this is a hardening-safe change (the crux — for the review gate's threat-model lens)

This *extends* a fail-closed allowlist, so it must be shown not to widen the trust boundary:

1. **No new capability vs the existing knob.** Anyone who can set `test_command` can already set
   `[verify] allowed_command_prefixes` — both live in the same `config.toml`. Auto-include grants
   *nothing* that the existing allowlist knob doesn't already grant from the same source; it only removes
   the duplication. The trust boundary is unchanged.
2. **The security scanner is orthogonal and still applies.** `is_command_allowed` is the replay-sandbox
   command-shape gate, not the security floor. A dangerous command (`rm -rf …`, `git commit --no-verify`)
   is vetoed by `gatekeeper scan`'s deny-rules regardless of any allowlist membership. Allowlisting ≠
   bypassing the scan.
3. **Add-only.** `effective_allowed_prefixes` only *appends* the configured commands; it never removes a
   default prefix and never weakens any gate. With no `test_command` configured, the effective list is
   byte-identical to `allowed_command_prefixes` (no behavior change for existing Rust projects).
4. **Threat model.** The floor is for "mistakes, not a determined evader." This targets the honest
   non-Rust user surprised that their configured test command is rejected — squarely the mistake class.

### Files (all unprotected — no `--no-verify`)

- `gatekeeper/src/config.rs`: new `effective_allowed_prefixes` method + unit tests. (Not protected.)
- `gatekeeper/src/verify.rs`: three call-site swaps. (Not protected.)
- No new config key (reuses `test_command` / `tdd_replay_test_command`), so `KNOWN_*_KEYS` and doctor are
  untouched. No edits to `main.rs`/`scan.rs`/`rules.toml`/hooks/`Cargo.*`.

## Scope / non-goals

- **In:** auto-include configured test commands in the effective replay allowlist; both replay gates.
- **Out:** a separate `[tdd]`-scoped allowlist (the cross-gate-coupling wart) — auto-including
  `tdd_replay_test_command` already removes the pain for tdd-replay; a new `[tdd]` list is net config
  surface for a marginal case. `fmt_command`/`lint_command`, `test_success_markers`, `test_globs`,
  per-language profiles — all deferred/dropped per the research (no consumer / redundant / polyglot).
- **Out:** changing the cargo-centric *default* itself (removing `cargo test` from the default would be a
  loosening-by-removal that breaks Rust projects; the fix is additive, not default-changing).

## Test strategy (TDD)

Unit tests on `effective_allowed_prefixes` in `config.rs` `#[cfg(test)] mod tests`:

1. `effective_includes_test_command` — `test_command = "swift test"`, default prefixes → result contains
   `"swift test"` AND still contains `"cargo test"` (add-only).
2. `effective_includes_tdd_replay_command` — `tdd_replay_test_command = "pytest -q"` → present.
3. `effective_dedupes_existing` — `test_command = "cargo test"` (already a default) → appears once.
4. `effective_skips_empty` — `test_command = Some("")` / whitespace → not added.
5. `effective_is_identity_when_unset` — no test commands → equals `allowed_command_prefixes` exactly.
6. **Behavioral cross-check:** `is_command_allowed(["swift","test"], &cfg.effective_allowed_prefixes())`
   is `true` when `test_command = "swift test"`, while
   `is_command_allowed(["swift","test"], &cfg.allowed_command_prefixes)` is `false` — proving the fix
   removes the rejection.

The three `verify.rs` call-site swaps are exercised by the verify gate's reproduce-then-resolve and by a
new positive integration test (below).

## Addendum: FM2 soundness interaction (discovered at the TDD gate; maintainer decision)

Implementing the fix surfaced a real interaction the design above missed. Two existing TDD-replay tests
(`cli_tdd_replay.rs`) deliberately set `test_command = "make test"` (a non-allowlisted command) and
assert the gate **fails closed** — guarding FM2 ("a replay command that never established red must not
PASS; logging a phantom pass poisons the burn-in"). The allowlist rejection was doing **double duty**:
portability friction *and* an FM2 soundness backstop (`execute_step` `Err` → `replay_red_green`
`Indeterminate` → fail-closed, `tdd.rs:307-310`). Auto-including the configured command removes the
rejection, so `make test` now *spawns*, errors (no Makefile), and its nonzero exit is read as a genuine
red → PASS — re-creating the exact FM2 symptom.

**Maintainer decision (AskUserQuestion): "Auto-include both + re-point the FM2 tests."** The FM2 property
is preserved, not erased: the genuine "never executed" path after the fix is a **spawn failure** — a
binary that cannot launch returns `Ok(StepResult{detail:"failed to spawn"})`, which `replay_red_green`
maps to `Indeterminate` (`tdd.rs:317-321`) → fail-closed. The two guards are re-pointed to a non-existent
binary (`topology-no-such-test-runner-xyzzy`), which exercises that path — a **more faithful** test of
"a command that never executed must not pass" than the old `make test` (which did execute). They now
guard the soundness property independent of the allowlist mechanism (and pass both pre- and post-fix).

The residual — a *configured, runnable* command that errors at merge-base for non-test reasons is read as
red — is **identical to the pre-existing behavior for `cargo test`** (an allowlisted command that errors
is already read as red); the operator declared the command as their test runner, so treating its nonzero
exit as red is the same trust already extended to cargo. This is within the "mistakes, not adversaries"
threat model.

### Revised test strategy

- Re-point `replay_nonallowlisted_command_fails_closed` → `replay_unrunnable_command_fails_closed` and
  `history_nonallowlisted_logs_skip_not_pass` → `history_unrunnable_command_logs_skip_not_pass`: use a
  non-existent binary so the FM2 fail-closed property is guarded via the spawn-failure path.
- **New positive guard** `replay_autoincluded_command_runs_and_establishes_red`: `test_command = "false"`
  (exists, exits nonzero, NOT in the default allowlist) → auto-included → RUNS → red → `Pass` → exit 0.
  This FAILS pre-fix (rejected → Indeterminate → exit 2), so it locks the `execute_step` wiring of
  `effective_allowed_prefixes` (proven by a temporary revert: exit 2 without the swap, exit 0 with it).
