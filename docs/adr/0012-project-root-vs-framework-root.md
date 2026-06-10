# 0012 — Project root vs framework root: artifacts move to `.claude/topology/` in governed projects

- **Status:** 🟢 Accepted
- **Date:** 2026-06-10

## Context

The first live cross-project install (v0.2.0 → `react-weather-app`) showed that `gatekeeper`
conflates two directories that differ in every cross-project setup: the **framework root** (where
`skills/`, `security/rules.toml`, `instincts/` live — `~/.topology` or a vendored copy) and the
**project root** (the repo being governed). Every gate anchored artifacts to `framework_root()`,
so a wired project (`TOPOLOGY_ROOT` set) would read its specs from — and `adapt` would write its
settings into — the framework checkout; unwired, the fallback dumped gate artifacts into the
governed project's root `docs/`, polluting it. The operator asked for the artifacts to live under
`.claude/topology/` instead, and for the installer to ask about harness and install scope.
Full analysis: [research](../research/2026-06-10-installer-v2.md).

## Decisions

1. **Two named roots.** `framework_root()` keeps its meaning (env override + marked walk-up,
   [spec 0009/0011 lineage]) and continues to anchor framework-owned state: skills, rules,
   instincts, the learn ledger, memory artifacts, and the `check docs` lint. A new
   `project_root()` — nearest `.git` ancestor of CWD (dir or file, so worktrees count), CWD
   fallback — anchors everything that belongs to the governed repo: gate artifacts, the review
   gate's git commands, and `adapt`'s generated files.

2. **The artifacts root is conditional, not configurable.** `artifacts_root()` =
   `project/docs` when `project_root() == framework_root()` (the framework repo governs itself —
   its CI, docs lint, and history depend on that layout), else `project/.claude/topology`. No env
   knob and no legacy fallback to a governed project's `docs/`: one deterministic rule, one
   `git mv` to migrate the single existing test bed. `.claude/` is already the per-project agent
   home, and the path is the operator's explicit choice; Codex/Cursor/OpenCode projects use the
   same path (documented), because the artifacts belong to Topology, not to the harness.

3. **`adapt` reads framework, writes project.** Skills/instincts/AGENTS.md come from
   `framework_root()`; generated configs land relative to `project_root()`, with hook command
   paths pointing into the framework. This is what makes "wire this project" a one-command,
   run-anywhere operation instead of a cd-into-the-framework ritual.

4. **The installer asks, with flag twins for every prompt.** Under `curl | bash`, stdin is the
   script, so prompts read from `/dev/tty` and only when it is genuinely open; otherwise defaults
   apply (global scope, claude harness, warn-only PATH repair) and are printed. Scope choices:
   **global** (`~/.topology`, shared) or **local** (framework vendored at `<project>/.topology`,
   pinned per project, gitignored). Harness choice drives the existing `adapt` generator — the
   installer adds no second config writer.

5. **Stale PATH binaries are repaired with consent, never silently.** A PATH `gatekeeper` outside
   the new install with a differing `--version` gets an interactive overwrite offer (`cp` over the
   same path — the fix lands where the skew lives); non-interactive runs warn and name both
   versions. `doctor` states the skew informationally. Deleting another installer's file without a
   yes is out of scope by principle.

## Consequences

- Governed projects keep their `docs/` for themselves; everything Topology writes or reads in a
  project sits under `.claude/topology/` (artifacts) and the harness config dir (wiring).
- The framework repo's own gates, CI, and docs lint are byte-for-byte unaffected.
- `review` and `adapt` become correct in wired projects for the first time (previously they
  silently targeted the framework checkout when `TOPOLOGY_ROOT` was set).
- One more resolver to keep honest: `doctor` must always print both roots and the artifacts root,
  so a mis-resolution is observable in one command.
- Existing external artifacts (the test bed's root `docs/`) need a one-time `git mv`; recorded in
  the USER-GUIDE, accepted over a compat fallback that would make artifact location ambiguous
  forever.
