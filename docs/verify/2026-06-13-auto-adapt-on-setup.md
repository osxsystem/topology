# Verify: idempotent setup-time `gatekeeper adapt` (auto-wire fresh clones/worktrees)

- **Date:** 2026-06-13
- **Feature slug:** auto-adapt-on-setup
- **Design:** docs/specs/2026-06-13-auto-adapt-on-setup.md
- **Plan:** docs/plans/2026-06-13-auto-adapt-on-setup.md
- **Code:** commits `test(adapt)` (cli_adapt.rs), `feat(setup)` (justfile), `docs(setup)` (DEVELOPMENT.md + CHANGELOG).

## End-to-end: `just setup` in a fresh worktree (reproduce-then-resolve)

A throwaway worktree at HEAD models a fresh clone. Before: **no `.claude/` directory**
(`ls /tmp/verify58_wt/.claude` → "No such file or directory"). Then `just setup`:

```
$ git worktree add /tmp/verify58_wt HEAD     # fresh tree, no .claude/
$ cd /tmp/verify58_wt && just setup
setup: updated <git-hooks>/pre-commit
setup: building gatekeeper (release) and wiring .claude/settings.json…
   Compiling gatekeeper v0.10.0 (/private/tmp/verify58_wt/gatekeeper)
    Finished `release` profile [optimized] target(s) in 15.47s
./gatekeeper/target/release/gatekeeper adapt --harness claude
wrote .claude/settings.json
```

The produced `/tmp/verify58_wt/.claude/settings.json` is **portable** — no manual `adapt`:

```json
{
  "hooks": {
    "PreToolUse": [ { "hooks": [ { "command": "${CLAUDE_PROJECT_DIR}/hooks/security-scan.sh", ... } ], "matcher": "Bash|Write|Edit|MultiEdit" } ],
    "UserPromptSubmit": [ { "hooks": [ { "command": "${CLAUDE_PROJECT_DIR}/hooks/skill-activation.sh", ... } ] } ]
  }
}
```

`${CLAUDE_PROJECT_DIR}` hook paths, and **no `env.GATEKEEPER_BIN`** — exactly the portable form
(invariant 2: the hooks resolve `gatekeeper/target/release/gatekeeper`, which the build just produced).

### Re-run is a settings.json no-op

```
$ cd /tmp/verify58_wt && just setup      # second run
setup: building gatekeeper (release) and wiring .claude/settings.json…
    Finished `release` profile [optimized] target(s) in 0.06s
./gatekeeper/target/release/gatekeeper adapt --harness claude
                                         # <-- no "wrote .claude/settings.json" line
```

The incremental build is a 0.06s no-op and `adapt` writes nothing (settings already correct) — the
write-on-drift no-op (`adapt.rs:930`).

## Acceptance criteria

| Criterion | Evidence |
|---|---|
| **AC1** — fresh tree auto-wires portable settings, no manual `adapt` | `just setup` in the fresh worktree produced the portable settings.json above (no prior `.claude/`). |
| **AC2** — re-run is a no-op on settings.json | Second `just setup`: `adapt` printed no `wrote` line. |
| **AC3** — self-governed claude apply-rerun-noop characterization (single-root harness, explicit no-op) | `dogfood_settings_claude_apply_rerun_is_noop` in `cli_adapt.rs` (passes). |
| **AC4** — trigger points decided/wired; post-checkout declined | `justfile` setup recipe (new) + `install.sh` (already) + spec/DEVELOPMENT.md (post-checkout declined with rationale). |
| **AC5** — DEVELOPMENT.md links ADR-0019 + states build coupling | `docs/DEVELOPMENT.md` "Bootstrapping a fresh clone or worktree" section. |
| **AC6** — Unreleased CHANGELOG entry | `CHANGELOG.md` `### Changed`. |

## Full suite + quality gates

```
$ cargo fmt --manifest-path gatekeeper/Cargo.toml --check     # FMT clean
$ cargo clippy --manifest-path gatekeeper/Cargo.toml --all-targets -- -D warnings
cargo clippy: No issues found
$ cargo test --manifest-path gatekeeper/Cargo.toml --quiet    # TOPOLOGY_ROOT unset
cargo test: 552 passed, 5 ignored (22 suites)
```

Baseline 551 at `0677053` → +1 characterization test = 552, 0 failed.

## Conclusion

`just setup` now wires a fresh clone/worktree's portable `.claude/settings.json` with zero manual
steps, and re-running changes nothing on settings.json — the corrective complement to the #52
detective warning, per the ADR-0019 generated-only decision. Every acceptance criterion is backed by
a re-runnable command and its output.
