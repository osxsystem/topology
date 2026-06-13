# Design: cross-tree dogfood generation (#54)

- **Date:** 2026-06-13
- **Feature slug:** adapt-cross-tree
- **Status:** approved
- **Research:** [docs/research/2026-06-13-adapt-cross-tree.md](../research/2026-06-13-adapt-cross-tree.md)
- **Issue:** [#54](https://github.com/osxsystem/topology/issues/54) (follows #50/#51, PR #55 merged)

## Problem

When a `gatekeeper` binary built in worktree A adapts clone B (`framework_root → A`,
`project_root → B`, `roots_differ == true`), `adapt` treats it as *governed* and bakes A's absolute
paths into B's `.claude/settings.json`. Delete A → B's hooks dangle. This **is** the originally-logged
incident (main clone pinned to `topology-phase12`), which #50/#51 did not close (they only handle
`read_root == write_root`).

Success: a cross-tree generation — binary in one topology worktree, `cwd` in another topology clone —
produces a **portable** `settings.json` (`${CLAUDE_PROJECT_DIR}/hooks/<name>.sh`, no pinned
`GATEKEEPER_BIN`), so it survives deletion of the generating worktree. Vendored/external **governed**
projects stay on absolute paths + pinned bin, unchanged.

## Constraints

- **Must not regress governed** (vendored-`.topology` or external). The portable form is only valid
  when `write_root` has `hooks/<name>.sh` at its own root.
- **git-common-dir is unsafe** (research Spike 2): vendored-governed (`<project>/.topology` +
  `<project>`) is the *same git repo*, so a same-repo check would misclassify it as cross-tree and
  break it. Rejected.
- Surgical: `build_claude_hooks` / `merge_claude_settings` signatures unchanged; only the predicate
  feeding them widens. No new deps. Generated output passes `gatekeeper scan`.
- **Non-goals:** doctor stale-path warning (#52); committing the dogfood settings.json (#53);
  changing governed behavior; the warn→block flips elsewhere.
- **Other harnesses are unaffected (verified, spec review L1).** Only the `claude` harness embeds
  absolute paths into `settings.json`. `build_codex` writes a static `.codex/config.toml`,
  `build_cursor` writes `.cursor/rules/*.mdc`, `build_opencode` writes `opencode.json` + skills —
  none wire hooks or a pinned `GATEKEEPER_BIN`, so they have no cross-tree dangling. No follow-up
  needed (the reviewer's L1 premise does not hold against the code).

## Approaches considered

1. **`write_root`-is-a-topology-clone → portable (chosen).** Emit the portable form when
   `read_root == write_root` **OR** `write_root` has both `hooks/skill-activation.sh` and
   `hooks/security-scan.sh` at its root. The second disjunct adds the cross-tree case. Directly tests
   the invariant the emitted path depends on; needs no git subprocess; provably leaves governed
   (no root `hooks/`) on the absolute branch.
2. **git-common-dir equality → portable.** Rejected — misclassifies vendored-governed (same repo) and
   breaks it (research Spike 2).
3. **Warn-only (don't change the output).** Rejected — a warning doesn't *prevent* the stale paths; the
   incident still happens. Doesn't close #54.
4. **Portable + warn.** Rejected as redundant — with the portable form the output is *correct*, so
   there is nothing to warn about. (No new lane in Bash; enforcement stays in Rust either way.)

## Design (chosen)

In the claude branch of `cmd_adapt`, widen the portable predicate (currently `in_framework =
!roots_differ`, `adapt.rs:849`):

```rust
/// True when the portable `${CLAUDE_PROJECT_DIR}/hooks/<name>` form resolves correctly — i.e. the
/// project root is itself a topology framework clone (hooks live at its root). Distinguishes
/// cross-tree dogfood (sibling clone, has root hooks/) from vendored/external governed (hooks under
/// .topology or elsewhere — no root hooks/).
fn project_has_root_hooks(write_root: &Path) -> bool {
    write_root.join("hooks/skill-activation.sh").exists()
        && write_root.join("hooks/security-scan.sh").exists()
}
```

Then:

```rust
let roots_differ = /* unchanged */;
let use_portable = !roots_differ || project_has_root_hooks(write_root);
// build the hooks with the portable form when use_portable:
let hooks = build_claude_hooks(read_root, use_portable)?;
// drop the pinned bin in every portable case (incl. cross-tree):
let bin_opt: Option<&str> = if use_portable { None } else { Some(bin.as_str()) };
```

`build_claude_hooks(framework_root, in_framework: bool)` and `merge_claude_settings` are unchanged —
they already key off the bool / `Option`. The `disk_ok` closure is unchanged (it keys off `bin_opt`).
Net diff: one new helper, the `use_portable` line replacing `in_framework`, and the `bin_opt`
condition switching from `roots_differ` to `!use_portable`.

### Why this closes the incident

Binary in worktree A adapts clone B: `roots_differ == true`, but B is a topology clone →
`project_has_root_hooks(B) == true` → `use_portable` → B gets `${CLAUDE_PROJECT_DIR}/hooks/<name>`
(= `B/hooks/<name>`, which exists) and no pinned bin → survives deletion of A.

## Test strategy (TDD targets, red first)

New unit tests (`adapt.rs`):
- `project_has_root_hooks_true_for_clone` (both hook scripts present → true);
  `_false_without_hooks` (no `hooks/` → false);
  **`_false_with_partial_hooks`** (only `skill-activation.sh` present, `security-scan.sh` missing →
  false — locks the AND so a future `&&`→`||` slip or a partially-populated clone is caught; spec
  review M1).

New e2e test (`cli_adapt.rs`) — the core #54 proof, using a NEW fixture where `write_root` is a
topology clone (has `hooks/skill-activation.sh` + `hooks/security-scan.sh` + `AGENTS.md` + `skills/` +
`git init`) and `read_root` (`TOPOLOGY_ROOT`) is a **different** framework dir (`roots_differ`):
- `cross_tree_dogfood_settings_are_portable` — generated `settings.json` hook command ==
  `${CLAUDE_PROJECT_DIR}/hooks/security-scan.sh`, no absolute `read_root` path, `GATEKEEPER_BIN` absent
  — even though `roots_differ`.

Optional e2e (spec review M1, cheap): `cross_tree_partial_hooks_stays_absolute` — roots-differ +
`write_root` has only one of the two root hook scripts → absolute + pinned (the AND held end-to-end).

No-regression guards (unchanged, must stay green): governed `ac4_settings_no_clobber`,
`ac5_gatekeeper_bin_value`, `adapt_writes_to_project_not_framework` (their `scratch_proj` write_root
has no root `hooks/` → absolute + pinned); roots-equal `dogfood_settings_are_portable`,
`claude_writes_hook_settings` (`read_root == write_root` → portable, via the first disjunct).

## Decisions (resolved in spec review)

- **M1 — AND-locking test:** add the partial-hooks unit (+ optional e2e) above. Accepted.
- **L1 — other-harness cross-tree dangling:** moot. Verified codex/cursor/opencode embed no absolute
  paths / no pinned bin (see Constraints). No follow-up issue.
- **M2 — `--check` drift on a pre-fix cross-tree `settings.json`:** post-fix `adapt --check` against a
  B generated by old code (absolute + pinned) correctly reports DRIFT (exit 1) → self-heals on the
  next `adapt` write. This is desired; no CI runs `adapt --check` on such an artifact (the only known
  cross-tree case, the main clone, is now roots-equal already-portable). One-line note, no code.
- **Predicate breadth / param name:** keep the two-hook-file check (not git-repo, not full
  clone-signature); helper param named `write_root` (matches `cmd_adapt`).

## Verify-gate symptom

Reproduce: in a scratch cross-tree layout (framework dir A ≠ project clone B with root hooks/),
pre-fix `adapt` bakes A's absolute hook path into B's settings (would dangle if A is deleted).
Resolve: post-fix B's settings use `${CLAUDE_PROJECT_DIR}/hooks/<name>` and carry no `GATEKEEPER_BIN`.
