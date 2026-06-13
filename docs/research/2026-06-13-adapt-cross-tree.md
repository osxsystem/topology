# Research: cross-tree dogfood generation (#54)

- **Date:** 2026-06-13
- **Feature slug:** adapt-cross-tree
- **Issue:** [#54](https://github.com/osxsystem/topology/issues/54) — builds on #50/#51 (PR #55, merged `7796b26`)

## The gap (#54)

#50/#51 made `adapt --harness claude` emit portable settings **only** when `read_root == write_root`
(`in_framework`). The originally-logged incident (`dogfood-settings-pinned-to-worktree`) was **not**
that case: the main clone's settings were pinned to the `topology-phase12` **sibling worktree**. That
arises when a `gatekeeper` binary built in worktree A adapts clone B → `framework_root → A`,
`project_root → B`, so `roots_differ == true` and adapt took the **governed** branch, baking A's
absolute paths into B. Delete A → B's hooks dangle. #50/#51 left that branch unchanged, so the
incident is still reachable.

## Current logic (post-#55, `adapt.rs`)

`cmd_adapt(args, read_root=framework_root(), write_root=project_root())`. In the claude branch:
- `roots_differ = canonicalize(read_root) != canonicalize(write_root)` (`adapt.rs:842`).
- `in_framework = !roots_differ` (`:849`) → `build_claude_hooks(read_root, in_framework)`; portable
  `${CLAUDE_PROJECT_DIR}/hooks/<name>` iff `in_framework`.
- `bin_opt = if roots_differ { Some(bin) } else { None }` (`:869`) → pinned `GATEKEEPER_BIN` only when
  `roots_differ`.

## Spike 1 — same-repo-different-worktree detection (git)

`git -C <path> rev-parse --path-format=absolute --git-common-dir` returns the shared `.git` for any
worktree. Verified across the two real worktrees:
- main clone → `/Users/hugues_mini/Codes/AgentTools/topology/.git`
- this worktree → `/Users/hugues_mini/Codes/AgentTools/topology/.git` (identical)
- a non-repo dir → `fatal: not a git repository`.

So same-repo worktrees ⟺ equal absolute common-dir. **But this is the wrong predicate** — see Spike 2.

## Spike 2 — why git-common-dir is UNSAFE here (the load-bearing finding)

`scripts/install.sh --project <path>` **vendors the payload at `<path>/.topology`** (install.sh:16).
So a governed project has `read_root = <project>/.topology`, `write_root = <project>` — **the same git
repo**. A git-common-dir check would classify vendored-governed as "cross-tree dogfood" and emit
`${CLAUDE_PROJECT_DIR}/hooks/<name>` = `<project>/hooks/<name>` — but the governed hooks live at
`<project>/.topology/hooks/<name>`. That path would not resolve → **git-common-dir breaks governed
projects.** Rejected.

## The correct invariant

The portable literal `${CLAUDE_PROJECT_DIR}/hooks/<name>.sh` is valid **iff `write_root` has
`hooks/<name>.sh` at its own root** — i.e. the project being governed *is itself a topology framework
clone*. This holds for:
- roots-equal dogfood (`write_root` is the topology repo) ✓
- cross-tree dogfood (`write_root` is a sibling topology clone — has `hooks/` at root) ✓
and is false for:
- vendored-governed (`write_root` = project; hooks at `<project>/.topology/hooks/`, not at root) ✓→absolute
- external/global governed (`write_root` = project, no `hooks/` at root) ✓→absolute

The hook filenames are topology-specific (`skill-activation.sh`, `security-scan.sh`); a non-topology
project having exactly those at its root is not a real scenario, and if it did it would be a topology
clone anyway. This filesystem check directly tests what the emitted path depends on — no git call.

## Test-fixture interactions (verified)

- `scratch_root` (roots-equal in-framework fixture, `cli_adapt.rs:11`) does **not** create `hooks/` →
  so the predicate must keep `read_root == write_root` as an independent portable trigger, or the
  merged `dogfood_settings_are_portable` test would regress. (Design keeps it.)
- `scratch_proj` (governed `write_root`, `:297`) is a bare `git init` with **no `hooks/`** → the
  governed guards (`ac4`, `ac5`, `adapt_writes_to_project_not_framework`) stay on the absolute branch
  unchanged.

## Scope

Pure-Rust change to `adapt.rs` (predicate computation + a small helper); `build_claude_hooks` /
`merge_claude_settings` signatures unchanged. No new deps. Generated output must still pass
`gatekeeper scan` (writes the protected `.claude/settings.json`). `adapt.rs` is not a protected path.
