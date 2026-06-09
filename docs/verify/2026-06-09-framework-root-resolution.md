# Verify — robust framework-root resolution

Evidence that the change in `gatekeeper/src/main.rs` meets
`docs/specs/2026-06-09-framework-root-resolution.md`. All commands re-runnable.

## Quality gates (from `gatekeeper/`)

```
$ cargo test
218 passed, 2 ignored (9 suites)
$ cargo fmt --check
(clean, exit 0)
$ cargo clippy -- -D warnings
No issues found
```

Five new `resolve_root` unit tests cover the acceptance criteria: hijack regression (stray `skills/`
without a marker → returns `start`), marked-root direct, nested-start walk-up, env-override-wins, and
invalid-override-ignored. One pre-existing integration fixture (`tests/cli_review.rs`) gained an
`AGENTS.md` marker — it builds a scratch framework root that must now be marked.

## Behavioural verification

**AC1/AC4 — repo still resolves to its own root:**
```
$ (cd <repo> && gatekeeper list | grep -c .)        → 12
$ (cd <repo> && gatekeeper doctor | grep 'skills/:') → skills/: ok
```

**AC1 — the `~/skills` hijack is gone.** From a scratch dir under `$HOME` (whose only ancestor with a
`skills/` dir is the unrelated `~/skills`, which has no Topology marker):
```
$ (cd ~/.tmp-gk-verify && gatekeeper list)
gatekeeper: no skills/ directory found        # falls back to cwd; before the fix it listed ~/skills
$ echo $? → 1
```

**AC3 — explicit override:**
```
$ (cd ~/.tmp-gk-verify && TOPOLOGY_ROOT=<repo> gatekeeper list | head -3)
  _getting-started   …
  brainstorm-design  …
  capture-gotcha     …                          # override resolves Topology from anywhere
```

## Result

All acceptance criteria met. No new dependencies; only `gatekeeper/src/main.rs` (+ one test fixture)
changed. The hooks and the `finish` gate were already immune (documented in the research note); this
change repairs manually-invoked framework-relative commands.
