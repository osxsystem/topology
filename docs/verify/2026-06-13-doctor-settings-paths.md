# Verify: doctor probe for stale/dangling settings.json paths

- **Date:** 2026-06-13
- **Feature slug:** doctor-settings-paths
- **Design:** docs/specs/2026-06-13-doctor-settings-paths.md
- **Plan:** docs/plans/2026-06-13-doctor-settings-paths.md
- **Code:** commits `06ad155` (helper), `87987d2` (probe + tests), built `--release` for the runs below.

## Acceptance criteria (from the spec) and evidence

### Reproduce-then-resolve (end-to-end CLI)

Script `/tmp/verify52.sh` builds a minimal healthy marked root, then runs `gatekeeper doctor`
three ways. Output (grepped to the relevant line + exit code):

```
===== CASE A: no .claude/settings.json (baseline) =====
settings.json paths: n/a (no .claude/settings.json)
EXIT=0

===== CASE B: stale hook path + stale GATEKEEPER_BIN (reproduce symptom) =====
settings.json paths: WARN: hook command path does not exist: /gone/hooks/security-scan.sh (stale clone/worktree — reinstall the framework or re-run 'gatekeeper adapt --harness claude' to repoint)
settings.json paths: WARN: GATEKEEPER_BIN path does not exist: /gone/target/release/gatekeeper (security-scan.sh will fall back to a repo/PATH build; re-run 'gatekeeper adapt --harness claude' to repoint)
EXIT=0

===== CASE C: resolvable ${CLAUDE_PROJECT_DIR} portable path (no false positive) =====
settings.json paths: ok
EXIT=0
```

| Acceptance criterion | Evidence |
|---|---|
| **AC1** — WARN names the offending hook `command` *and* `GATEKEEPER_BIN` path when missing | CASE B: both WARN lines naming `/gone/hooks/security-scan.sh` and `/gone/target/release/gatekeeper`. |
| **AC2** — a valid `${CLAUDE_PROJECT_DIR}`-relative path that resolves produces no warning | CASE C: `settings.json paths: ok`, no WARN. |
| **AC-advisory** — the warning never changes doctor's exit code | CASE A/B/C all `EXIT=0`; the WARN in CASE B does not flip the exit. |

### No false positive on the live repo

The repo's real `.claude/settings.json` uses absolute paths that currently exist on disk:

```
$ gatekeeper doctor | grep "settings.json paths"
settings.json paths: ok
```

Confirms the probe does not cry wolf on a healthy absolute-path install.

### Automated tests (reproduce-then-resolve, encoded)

- **AC3** — `resolve_claude_project_dir` unit test: watched RED (`not yet implemented` panic at
  `src/doctor.rs:687`), then GREEN.
- **AC4** — three `cli_doctor.rs` integration tests (`doctor_warns_on_stale_settings_hook_path`,
  `doctor_no_warn_on_resolvable_portable_hook_path`, `doctor_warns_on_stale_gatekeeper_bin`):
  watched RED against the no-op stub, then GREEN after implementing the probe.

### Full suite + quality gates

```
$ cargo fmt --check        # silent, exit 0
$ cargo clippy --all-targets -- -D warnings
cargo clippy: No issues found
$ cargo test --quiet       # TOPOLOGY_ROOT unset
cargo test: 551 passed, 5 ignored (22 suites, 12.53s)
```

Baseline was 547 passed at `1fc7f8a`; +4 new tests = 551, 0 failed.

## Conclusion

Every acceptance criterion is demonstrated with a re-runnable command and its actual output. The
probe converts the worktree-portability failure (a cryptic mid-session `PreToolUse hook error`)
into an upfront, advisory, fix-naming doctor warning, with no false positive on portable or healthy
absolute-path settings.
