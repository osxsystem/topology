# Verify — portable adapt-generated settings.json (#50 + #51)

- **Date:** 2026-06-13
- **Spec:** `docs/specs/2026-06-13-adapt-portable-settings.md` · **Plan:** `docs/plans/2026-06-13-adapt-portable-settings.md`
- **Binary:** `gatekeeper 0.10.0`

## Symptom (before)

`gatekeeper adapt --harness claude` baked the **generating clone's absolute paths** into
`.claude/settings.json` even in the in-framework (dogfood) case (`read_root == write_root`): hook
`command` strings rooted at the clone, and `env.GATEKEEPER_BIN = <clone>/bin/gatekeeper`. When that
clone/worktree was deleted (routine phase cleanup), the dangling hook path made every Bash/Write/Edit
in other sessions throw `PreToolUse … No such file or directory`, and the stale `GATEKEEPER_BIN` made
`gatekeeper doctor` exit 1 (recorded in memory `dogfood-settings-pinned-to-worktree`).

The two e2e tests, run against the pre-fix code this session, reproduced it exactly:

- `dogfood_settings_are_portable` — **FAILED**:
  `left: "/private/var/folders/…/topo_adapt_cli_portable_84449/hooks/security-scan.sh"` vs
  `right: "${CLAUDE_PROJECT_DIR}/hooks/security-scan.sh"` — an absolute clone path was baked in.
- `readapt_removes_stale_gatekeeper_bin` — **FAILED** (`panicked: re-adapt clears the stale pin`) — a
  re-`adapt` left a pre-existing stale `GATEKEEPER_BIN` untouched.

## Resolution (after)

In the in-framework case (keyed on the existing `roots_differ` predicate, `adapt.rs`), `adapt` now
emits `${CLAUDE_PROJECT_DIR}/hooks/<name>.sh` (which Claude Code expands in hook `command` strings)
and **omits** `env.GATEKEEPER_BIN`, actively removing any pre-existing pin (the hook's own 6-tier
fallback resolves the binary). Governed downstream projects (`read_root != write_root`) keep the
absolute paths + pinned bin — unchanged. The same two tests are now green.

### Reproduce-then-resolve evidence

```evidence
$ env -u GATEKEEPER_BIN -u TOPOLOGY_ROOT cargo test --manifest-path gatekeeper/Cargo.toml --test cli_adapt -- dogfood_settings_are_portable readapt_removes_stale_gatekeeper_bin
# expect: 2 passed
# expect: 0 failed
```

```evidence
$ env -u GATEKEEPER_BIN -u TOPOLOGY_ROOT cargo test --manifest-path gatekeeper/Cargo.toml --bin gatekeeper -- merge_settings build_claude_hooks claude_wires
# expect: 10 passed
# expect: 0 failed
```

- **`dogfood_settings_are_portable`** — generated `settings.json` hook command ==
  `${CLAUDE_PROJECT_DIR}/hooks/security-scan.sh`, no absolute scratch path present, and
  `env.GATEKEEPER_BIN` absent. (Before: absolute clone path + pin.)
- **`readapt_removes_stale_gatekeeper_bin`** — seed a stale absolute `GATEKEEPER_BIN`, run `adapt`
  (roots-equal) → the key is gone. (Before: untouched.)
- **`merge_settings_none_bin_removes_gatekeeper_bin`** / `…_absent_env_stays_absent` — `None` bin
  removes the key, preserves other env keys, doesn't fabricate an `env` object.
- **`build_claude_hooks_in_framework_uses_project_dir_var`** / `…_governed_uses_absolute` — the
  `in_framework` flag selects the var form vs the absolute form.
- **No governed regression:** `adapt_writes_to_project_not_framework`, `ac4_settings_no_clobber`,
  `ac5_gatekeeper_bin_value`, `claude_writes_hook_settings` unchanged and green.

> Local note: all `cargo` runs in this repo require `env -u GATEKEEPER_BIN -u TOPOLOGY_ROOT` prefixes
> — a stale inherited `GATEKEEPER_BIN` otherwise perturbs the `cli_doctor` probe. CI has no such var.

## Full suite

```evidence
$ env -u GATEKEEPER_BIN -u TOPOLOGY_ROOT cargo test --manifest-path gatekeeper/Cargo.toml
# expect: test result: ok
```

286 bin-unit tests + integration suites green, 0 failed (2 `#[ignore]`d hollow fixtures unchanged).

## Scope honesty

The diff (`origin/main..HEAD`) touches only `gatekeeper/src/adapt.rs` (the two functions, the
`cmd_adapt` claude branch, the `disk_ok` closure, the `merge_claude_settings` doc comment, and the
in-file unit tests), `gatekeeper/tests/cli_adapt.rs` (two e2e tests), `CHANGELOG.md`, and the gate
artifacts. **Deferred, by design:** cross-tree dogfood generation (#54), the doctor stale-path warning
(#52), committing the dogfood settings.json (#53). Governed-downstream behavior is unchanged. No
version bump / release tag in this PR.

## Gate status

research ✓ · design ✓ (PASS) · plan ✓ (PASS, baseline green) · tdd ✓ (PASS — failing-test-first
history confirmed; every behavior watched red before green this session) · verify (this doc) · review
→ next · finish → full suite green.
