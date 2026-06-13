# Design: doctor probe for stale/dangling settings.json paths

- **Date:** 2026-06-13
- **Feature slug:** doctor-settings-paths
- **Status:** approved
- **Issue:** #52

## Problem

When a framework clone/worktree moves or is deleted, `.claude/settings.json` is left pointing at
hook scripts and a `GATEKEEPER_BIN` that no longer exist. The first symptom a user sees is a cryptic
runtime `PreToolUse hook error` mid-session (the dogfood incident in memory
`dogfood-settings-pinned-to-worktree`). `gatekeeper doctor` should catch this *ahead of time*: if a
hook `command` or `GATEKEEPER_BIN` path in settings.json does not exist on disk, surface a clear,
advisory warning naming the offending path — without false-flagging a valid `${CLAUDE_PROJECT_DIR}`
portable path.

**Primary beneficiary (post PRs #55/#56).** `adapt` now emits *portable* dogfood settings —
`${CLAUDE_PROJECT_DIR}/hooks/…` commands and **no** `GATEKEEPER_BIN` (`adapt.rs:856` `use_portable`,
`:876` `bin = None`). So freshly-adapted dogfood settings can no longer dangle. This probe is a
diagnostic backstop that complements that generative fix; its primary beneficiary is now:

1. **Governed installs** (framework outside the project) — absolute hook paths + absolute
   `GATEKEEPER_BIN` are by-design (CONTRACT.md). If that framework moves, they dangle. ← main value.
2. **Legacy/stale dogfood settings** not yet re-adapted (the absolute-form file currently on disk in
   this repo). ← the recorded incident.
3. Post-fix portable dogfood settings → the probe correctly stays silent.

Success: running `gatekeeper doctor` against a settings.json with a dangling path prints a `WARN`
line naming that path; against a settings.json whose portable path resolves, it prints no warning.

## Constraints

- **Advisory, not a gate.** The probe must not increment doctor's `failures` count — doctor still
  exits 0 when only this warning fires (acceptance criterion 4). Matches the existing
  `probe_config_unknown_keys` / `probe_orphaned_replay_worktrees` precedent.
- **Three-language lanes.** All logic lives in Rust (`doctor.rs`); no behavior added to Bash/MD.
- **No new dependency.** Reuse `serde_json` (already in `Cargo.toml`).
- **Grok `${CLAUDE_PROJECT_DIR}`.** Substitute it against the project root before the existence
  check — **on hook commands only** (M2). The `env` block has no documented interpolation; post-fix
  `GATEKEEPER_BIN` is either absent or an absolute path, never `${CLAUDE_PROJECT_DIR}`-relative, so
  it is checked as-is.
- **Surgical.** Add one probe + one helper + tests; do not touch the existing runtime-env
  `GATEKEEPER_BIN` probe (distinct source: env var vs. file).
- **Non-goals.** Not auto-fixing settings.json; not expanding arbitrary env vars beyond
  `${CLAUDE_PROJECT_DIR}`; not changing exit codes; not reading any harness's settings other than
  Claude's `.claude/settings.json` (codex/cursor/opencode coverage is a clean later phase — L2);
  not replicating the hook's 6-tier binary fallback inside the probe (M1c — over-built).

## Approaches considered

1. **Pure probe over the project's `.claude/settings.json` (recommended).**
   Add `fn probe_settings_paths(project_root: &Path)` that reads
   `<project_root>/.claude/settings.json`, parses JSON, collects every hook `command` plus
   `env.GATEKEEPER_BIN`, substitutes `${CLAUDE_PROJECT_DIR}` → `project_root` (hook commands only),
   and prints a `WARN:` line for each that does not exist (one `ok` line when all resolve / `n/a`
   when the file is absent). Returns nothing / prints only — never feeds `failures`. A small pure
   helper `resolve_claude_project_dir(raw, project_root) -> PathBuf` does the substitution and is
   unit-tested directly.
   *Trade-offs:* mirrors the existing advisory-probe shape exactly; trivially testable with the
   established fixture pattern; low risk; reversible (one self-contained block).

2. **Extend the existing `GATEKEEPER_BIN` probe to also read settings.json.**
   Fold the file-path check into the current env-var probe at `doctor.rs:188-203`.
   *Trade-offs:* conflates two distinct sources (runtime env vs. file) into one line, muddying the
   output and the existing failure semantics (that probe *does* increment `failures`). Higher risk
   of regressing the current behavior. Rejected.

3. **Generic settings.json schema validator.**
   A broader validator over all settings keys.
   *Trade-offs:* over-built for a single-symptom diagnostic; a staff engineer would call it scope
   creep against issue #52. Rejected (Simplicity).

## Decision

**Approach 1.** It is the minimal change that satisfies every acceptance criterion, matches the
advisory-probe precedent already in `doctor.rs`, adds no dependency, and isolates the new behavior
in one testable block. The `${CLAUDE_PROJECT_DIR}` substitution lives in a pure helper so the
"no-false-positive" criterion is exercised by a direct unit test, not only end-to-end.

### Locked decisions (from design review)

- **B1 — path extraction: whole-string.** The entire `command` string (after substitution) is the
  path. Topology never emits arguments in hook commands (`build_claude_hooks`, `adapt.rs:538-548` —
  always a bare path), so a first-token split would only ever truncate a real path that contains a
  space (e.g. `/Users/My Name/…`) and misreport *why* it's missing. Whole-string is correct here.
- **M1 — `GATEKEEPER_BIN`: kept, with fallback-aware wording.** Issue #52 AC1 explicitly names
  `GATEKEEPER_BIN`, so the probe checks it. But `security-scan.sh` has a 6-tier fallback
  (env → prebuilt `bin/` → plugin-data → repo `release` → `debug` → PATH), so a missing absolute
  `GATEKEEPER_BIN` (e.g. a `target/release` path on a debug-only clone) is *not* necessarily broken.
  The WARN therefore names the fallback rather than asserting breakage — true and non-cry-wolf.
- **M3 — output token: a pinned `WARN:` literal.** Doctor today speaks `ok` / `FAIL:` / `n/a` /
  prose-informational. A dangling hook path is a latent breakage (more than the benign
  unknown-key/orphan-worktree prose cases), so it earns a distinct, greppable `WARN:` tier — adopted
  deliberately as a third vocabulary level, and **pinned** (AC1 + tests grep the literal). It stays
  advisory: `WARN:` never increments `failures`.
- **M4 — remediation hint broadened.** The hint must cover the governed case (where the framework
  may be gone and `gatekeeper` may not even be runnable), not only dogfood-repoint.

### Shape

```rust
// pure, unit-tested: "${CLAUDE_PROJECT_DIR}/hooks/x.sh" + root -> root/hooks/x.sh
fn resolve_claude_project_dir(raw: &str, project_root: &Path) -> PathBuf

// reads <project_root>/.claude/settings.json, prints WARN per missing path; never fails doctor
fn probe_settings_paths(project_root: &Path)
```

Output lines (advisory; literals pinned):
- file absent → `settings.json paths: n/a (no .claude/settings.json)`
- all resolve → `settings.json paths: ok`
- a dangling hook command (after `${CLAUDE_PROJECT_DIR}` substitution) →
  `settings.json paths: WARN: hook command path does not exist: <path> (stale clone/worktree — reinstall the framework or re-run 'gatekeeper adapt --harness claude' to repoint)`
- a dangling `GATEKEEPER_BIN` (checked as-is) →
  `settings.json paths: WARN: GATEKEEPER_BIN path does not exist: <path> (security-scan.sh will fall back to a repo/PATH build; re-run 'gatekeeper adapt --harness claude' to repoint)`

Wired into `cmd_doctor` between `probe_config_unknown_keys` (`doctor.rs:371`) and the summary
(`:380`), called with `crate::project_root()` (`.claude/settings.json` lives at the project root —
correct for governed installs too).

## Risks & open questions

- **GATEKEEPER_BIN cry-wolf (M1).** Mitigated by fallback-aware wording: the WARN names the
  fallback instead of asserting the setup is broken.
- **Symlinked / relative paths.** Existence is checked via `Path::exists()` which follows symlinks;
  a relative path in settings.json (topology never emits one) would resolve against the doctor
  process CWD. Acceptable — advisory only, and out of scope.
- **`project_root()` resolution (L3).** It is the nearest `.git`-ancestor of cwd (`main.rs:455`),
  which in the normal case equals the harness's `$CLAUDE_PROJECT_DIR` → the probe inspects the same
  file the harness loads. Run from outside a repo, or a monorepo where `.claude` is not at the git
  root, it reads a different/absent file → `n/a`. Acceptable; documented.
- **Claude-only scope (L2).** `adapt` also targets codex/cursor/opencode, whose settings can carry
  stale paths too. Claude-only is a deliberate v1 cut; the others are a clean later phase.

## Acceptance criteria

- [ ] `gatekeeper doctor` emits a `WARN:` line naming the offending path when a settings.json hook
      `command` or `GATEKEEPER_BIN` path is missing on disk.
- [ ] A valid `${CLAUDE_PROJECT_DIR}`-relative hook command that resolves on disk produces no
      warning (no false positive).
- [ ] Unit test (in `doctor.rs`): `resolve_claude_project_dir` substitutes the literal correctly.
- [ ] Integration test (in `gatekeeper/tests/cli_doctor.rs`, scratch-root + subprocess stdout,
      following `doctor_warns_on_orphaned_replay_worktree`): a settings.json with a nonexistent hook
      path triggers the WARN; a fixture with a resolvable `${CLAUDE_PROJECT_DIR}` hook path does not.
- [ ] The warning is advisory — doctor's exit code / `failures` count is unchanged by it (asserted
      in the integration test: exit 0 with the WARN present).
