# Plan: Replay-allowlist portability fix (slice #3, P0)

- **Date:** 2026-06-14
- **Feature slug:** hardened-config-layer
- **Design:** `docs/specs/2026-06-14-hardened-config-layer.md`
- **Branch:** `fix/replay-allowlist-portability` (off `main` at `0ee07ca`)

All changes land in **unprotected** files (`gatekeeper/src/config.rs`, `gatekeeper/src/verify.rs`) via
the Edit tool. No edits to `main.rs`/`scan.rs`/`rules.toml`/hooks/`Cargo.*`. No new config key. Run cargo
with `TOPOLOGY_ROOT` unset.

## Task 1 — TDD red: unit tests for `effective_allowed_prefixes`

In `gatekeeper/src/config.rs` `#[cfg(test)] mod tests`, add 6 tests driving a not-yet-existing method
`ProjectConfig::effective_allowed_prefixes(&self) -> Vec<String>`:

1. `effective_includes_test_command` — `ProjectConfig { test_command: Some("swift test"), ..default() }`
   → result contains `"swift test"` AND still contains `"cargo test"` (add-only; defaults preserved).
2. `effective_includes_tdd_replay_command` — `tdd_replay_test_command: Some("pytest -q")` → contains
   `"pytest -q"`.
3. `effective_dedupes_existing` — `test_command: Some("cargo test")` (already a default) → `"cargo test"`
   appears exactly once.
4. `effective_skips_empty` — `test_command: Some("")`, `tdd_replay_test_command: Some("   ")` → neither
   added; result equals `allowed_command_prefixes`.
5. `effective_is_identity_when_unset` — both test commands `None` → result equals
   `allowed_command_prefixes` exactly (same order, same length).
6. `effective_unblocks_via_is_command_allowed` — with `test_command: Some("swift test")`:
   `verify::is_command_allowed(&["swift","test"], &cfg.effective_allowed_prefixes())` is `true`, while
   `verify::is_command_allowed(&["swift","test"], &cfg.allowed_command_prefixes)` is `false`.
   (Import `crate::verify::is_command_allowed`; it is `pub`.)

Run `cargo test --manifest-path gatekeeper/Cargo.toml --bin gatekeeper effective_` and **watch it fail to
compile** (method absent). Record the red.

## Task 2 — TDD green: implement the method

Add to `impl ProjectConfig` (near `load`, in `config.rs`):

```rust
/// The replay command allowlist, extended with the project's own configured test commands.
/// Add-only: appends `test_command` and `[tdd] replay_test_command` (verbatim, deduped, empties
/// skipped) so a project that declares its test command does not ALSO have to duplicate it in
/// `[verify] allowed_command_prefixes`. Grants nothing beyond the existing allowlist knob (same
/// config.toml source); the security scanner still vetoes dangerous commands independently.
pub fn effective_allowed_prefixes(&self) -> Vec<String> {
    let mut prefixes = self.allowed_command_prefixes.clone();
    for cmd in [self.test_command.as_deref(), self.tdd_replay_test_command.as_deref()]
        .into_iter()
        .flatten()
    {
        let trimmed = cmd.trim();
        if !trimmed.is_empty() && !prefixes.iter().any(|p| p == trimmed) {
            prefixes.push(trimmed.to_string());
        }
    }
    prefixes
}
```

Re-run the Task 1 filter → all 6 green.

## Task 3 — wire the three verify.rs call sites

In `gatekeeper/src/verify.rs`, replace `&cfg.allowed_command_prefixes` with
`&cfg.effective_allowed_prefixes()` at the three `is_command_allowed(&argv, …)` call sites:
`verify.rs:471` (`execute_step` — also covers tdd-replay via `tdd.rs:307`), `verify.rs:666` (evidence
static analysis), `verify.rs:980` (legacy path). At 666, if the check sits inside a loop over commands,
hoist `let allowed = cfg.effective_allowed_prefixes();` above the loop and pass `&allowed` (avoid
recomputing per iteration); 471 and 980 are single-shot, inline is fine.

Update the two rejection messages that name the field so they stay accurate (they currently say
"not in allowed_command_prefixes" — leave the message wording, it is still the effective list rooted in
that field; do NOT churn message text unless a test asserts it).

## Task 4 — finish-gate prep

With `TOPOLOGY_ROOT` unset:
- `cargo fmt --manifest-path gatekeeper/Cargo.toml` then `… -- --check`.
- `cargo clippy --manifest-path gatekeeper/Cargo.toml --all-targets -- -D warnings`.
- `cargo test --manifest-path gatekeeper/Cargo.toml` — full suite green; note the +6 delta; confirm the
  existing replay integration tests (verify/tdd) still pass (the call-site swap must not regress them).

## Task 5 — verify gate (reproduce-then-resolve)

Record at `docs/verify/2026-06-14-hardened-config-layer.md`:
- **Reproduce:** in a temp repo with `[verify] mode = "replay"` and `test_command = "swift test"` (a
  command not in the default allowlist), show the pre-fix behavior is rejection — assert
  `is_command_allowed(["swift","test"], default_allowed_prefixes())` is `false` (the rejection path), and
  reference that a replay step would map to Indeterminate.
- **Resolve:** show `is_command_allowed(["swift","test"], effective_allowed_prefixes())` is `true`, i.e.
  the configured command is now accepted; and that with no `test_command`, the effective list is
  identical to the default (no behavior change for Rust projects).

## Task 6 — code-review gate

Fresh-context critic panel (correctness, simplicity, **threat-model** — verify the "no new trust
boundary / scanner-orthogonal / add-only" claims adversarially) on the clean `HEAD` bound to merge-base.
Record at `docs/reviews/2026-06-14-hardened-config-layer.md`. Address blocking findings before finish.

## Task 7 — finish + commit

`gatekeeper check finish -- cargo test --manifest-path gatekeeper/Cargo.toml`. Commit `config.rs` +
`verify.rs` + artifacts (no `--no-verify` — unprotected). Do not push without maintainer confirmation.

## Rollback

Additive: revert by deleting `effective_allowed_prefixes`, its tests, and reverting the three call sites
to `&cfg.allowed_command_prefixes`. No data migration, no protected-file state.
