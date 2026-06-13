# Research: doctor probe for stale/dangling settings.json paths

- **Date:** 2026-06-13
- **Feature slug:** doctor-settings-paths
- **Issue:** #52

## Question

Add a `gatekeeper doctor` probe that warns when `.claude/settings.json` references a
hook-script `command` or `GATEKEEPER_BIN` path that does not exist on disk — catching the
worktree-portability failure *before* it surfaces as a runtime `PreToolUse hook error`. It must
understand `${CLAUDE_PROJECT_DIR}`-relative paths and not false-flag a valid portable path.

Exploration was done in-loop (light scope — three files); top-risk facts cross-checked against the
real `.claude/settings.json` rather than only the templates.

## Findings (cited)

### Where settings.json lives and its shape
- The harness reads `.claude/settings.json` at the **project root** (the repo the developer works
  in). Real shape (`/Users/hugues_mini/Codes/AgentTools/topology/.claude/settings.json:1-30`):
  - `hooks.PreToolUse[].hooks[].command` and `hooks.UserPromptSubmit[].hooks[].command` — each a
    path to a hook script (e.g. `…/hooks/security-scan.sh`).
  - `env.GATEKEEPER_BIN` — a path to the gatekeeper binary.
  - Today these are **absolute** paths (the worktree-portability incident: a deleted clone left
    dangling absolute paths here — recorded in memory `dogfood-settings-pinned-to-worktree`).

### Path formats the probe must grok
- `gatekeeper/src/adapt.rs:536-548` (`build_claude_hooks`): the portable form is the literal
  string `${CLAUDE_PROJECT_DIR}/hooks/<name>.sh`; the non-portable form is the hook path joined
  onto `framework_root` (absolute). So a path token may begin with the literal
  `${CLAUDE_PROJECT_DIR}` which Claude Code expands to the project dir at runtime.
- To avoid a false positive on a valid portable path, the probe must substitute
  `${CLAUDE_PROJECT_DIR}` → project root **before** the existence check.

### How doctor probes are structured (the pattern to match)
- `gatekeeper/src/doctor.rs:79-387` — `cmd_doctor(root, source)` prints one line per probe and
  accumulates a `failures` count; exit 1 iff any FAIL.
- Helper probes follow `fn probe_x(dir: &Path) -> usize` returning a failure count
  (`probe_hooks` :658, `probe_instincts` :591, `probe_skills` :630).
- **Advisory** (non-failing) probes already exist and print without touching `failures`:
  `probe_config_unknown_keys` :413 and `probe_orphaned_replay_worktrees` :476. This is the exact
  shape issue #52 asks for ("advisory … not a hard gate failure").
- An existing probe at `doctor.rs:188-203` checks `GATEKEEPER_BIN` but reads the **runtime
  environment variable** (`std::env::var`), *not* the value written in settings.json. The new probe
  is distinct: it reads the file, so it catches a stale settings.json value even when the env var is
  absent/scrubbed (precisely the failure mode in the dogfood incident).
- `project_root()` is already available inside `cmd_doctor` (used at :110, :289).

### Dependencies on hand
- `serde_json` is a dependency (used throughout `adapt.rs`, e.g. :157). No new crate needed.
- Test convention: temp-dir fixtures written and removed in `#[cfg(test)]`
  (`doctor.rs:691-829`) — directly reusable for a fixture with a missing vs. resolvable path.

## Risks / unknowns
- A hook `command` could in principle carry arguments (`script.sh --flag`). Topology emits bare
  path commands, but the probe should defensively treat the **first whitespace-delimited token** as
  the path so an argument never causes a false "missing" warning. (Design decision.)
- `${CLAUDE_PROJECT_DIR}` is the only documented variable in topology's emitted settings; other
  env-var expansions (e.g. `$HOME`) are out of scope — flag-as-missing is acceptable there because
  topology never emits them, and the warning is advisory anyway.
