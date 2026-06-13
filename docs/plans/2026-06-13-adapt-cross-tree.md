# Plan: cross-tree dogfood generation (#54)

- **Date:** 2026-06-13 · **Feature slug:** adapt-cross-tree
- **Design:** [docs/specs/2026-06-13-adapt-cross-tree.md](../specs/2026-06-13-adapt-cross-tree.md) (approved)
- **Research:** [docs/research/2026-06-13-adapt-cross-tree.md](../research/2026-06-13-adapt-cross-tree.md)

## Baseline (clean)

`env -u GATEKEEPER_BIN -u TOPOLOGY_ROOT cargo test` (worktree, debug) → all suites pass, 0 failed
(3 `#[ignore]`d hollow fixtures). The `env -u` scrub is mandatory locally (stale `GATEKEEPER_BIN`
perturbs `cli_doctor`); CI has no such var. Unit tests via `--bin gatekeeper`, e2e via `--test cli_adapt`.

## Files to touch

| File | Responsibility |
|------|----------------|
| `gatekeeper/src/adapt.rs` | NEW `project_has_root_hooks(write_root)` helper; in the claude branch, `use_portable = !roots_differ \|\| project_has_root_hooks(write_root)`, feed it to `build_claude_hooks` and switch `bin_opt` from `roots_differ` to `!use_portable`; in-file unit tests for the helper. |
| `gatekeeper/tests/cli_adapt.rs` | NEW fixture `scratch_clone_as_project`; NEW e2e `cross_tree_dogfood_settings_are_portable` + `cross_tree_partial_hooks_stays_absolute`. |
| `CHANGELOG.md` | Unreleased note (#54). |

No ADR (bug fix, not architecture). No new deps; pure-Rust; lanes preserved. `build_claude_hooks` and
`merge_claude_settings` signatures unchanged.

## Delegation

Tests via `test-engineer-tdd`, code via `feature-implementer` (fallback: `general-purpose` on Opus
reading the agent `.md`, never Sonnet). Main loop watches red/green, runs fmt+clippy **before** the
review gate (per `finish-gate-needs-fmt-clippy`), commits serially (tests-only commit first for the
TDD gate, since in-file `src/` unit tests are production-classified — the e2e in `tests/` is the
test-only commit).

## Tasks (TDD order — test first, watch red, implement, watch green)

### Task 1 — `project_has_root_hooks` helper + unit tests

- **Test (test-engineer-tdd), `adapt.rs` `#[cfg(test)]` module** (use the existing `fixture(tag)` helper,
  which creates `AGENTS.md`/`skills/`/`instincts/` but NOT `hooks/`; add hooks per case):
  - `project_has_root_hooks_true_for_clone`: `let root = fixture("phrt_true");` then
    `fs::create_dir_all(root.join("hooks")).unwrap();` and write both `hooks/skill-activation.sh` and
    `hooks/security-scan.sh` (any content) → `assert!(project_has_root_hooks(&root))`. Cleanup.
  - `project_has_root_hooks_false_without_hooks`: `let root = fixture("phrt_none");` → no `hooks/` →
    `assert!(!project_has_root_hooks(&root))`. Cleanup.
  - `project_has_root_hooks_false_with_partial_hooks` (M1): `fixture("phrt_partial")` + create `hooks/`
    with ONLY `skill-activation.sh` (no `security-scan.sh`) → `assert!(!project_has_root_hooks(&root))`.
    Cleanup.
- **Watch red:** `env -u GATEKEEPER_BIN -u TOPOLOGY_ROOT cargo test --bin gatekeeper project_has_root_hooks`
  → fails to compile (helper doesn't exist).
- **Impl (feature-implementer), `adapt.rs`** (place the fn near `build_claude_hooks`):
  ```rust
  /// True when the portable `${CLAUDE_PROJECT_DIR}/hooks/<name>` form resolves correctly — i.e. the
  /// project root (`write_root`) is itself a topology framework clone, with the hook scripts at its
  /// own root. Distinguishes cross-tree dogfood (sibling clone — has root `hooks/`) from
  /// vendored/external governed (hooks under `.topology/` or elsewhere — no root `hooks/`).
  fn project_has_root_hooks(write_root: &Path) -> bool {
      write_root.join("hooks/skill-activation.sh").exists()
          && write_root.join("hooks/security-scan.sh").exists()
  }
  ```
- **Watch green:** same command → 3 pass.
- **Commit:** test-only commit lands in Task 3's tests-first commit; impl in Task 3's code commit
  (see Delegation). For clarity here: helper impl ships with the Task 3 production commit.

### Task 2 — cross-tree e2e tests (the #54 proof)

- **Test (test-engineer-tdd), `gatekeeper/tests/cli_adapt.rs`:**
  - NEW fixture `scratch_clone_as_project(tag) -> PathBuf`: `git init -q -b main` a scratch dir, then
    populate it as a topology clone — `AGENTS.md`, `skills/brainstorm-design/SKILL.md`, `instincts/`,
    and **`hooks/skill-activation.sh` + `hooks/security-scan.sh`** at its root (mirror `scratch_root`
    + the two hook files + `git init`). Returns the path.
  - NEW fixture/framework dir for `read_root`: reuse `scratch_fw_with_template(tag)` (a separate
    framework dir with `AGENTS.md` so `require_agents_md` passes for `read_root`). Distinct path →
    `roots_differ`.
  - `cross_tree_dogfood_settings_are_portable`: run via `run_proj(&fw, &clone_proj, &["adapt",
    "--harness", "claude"])` (pins `TOPOLOGY_ROOT=fw`, cwd=clone_proj → `roots_differ` true). Assert:
    `v["hooks"]["PreToolUse"][0]["hooks"][0]["command"] == "${CLAUDE_PROJECT_DIR}/hooks/security-scan.sh"`,
    settings string does NOT contain the absolute `fw` path, and `v["env"].get("GATEKEEPER_BIN").is_none()`
    — *even though `roots_differ`*.
  - `cross_tree_partial_hooks_stays_absolute` (M1): same setup but the project clone has ONLY
    `hooks/skill-activation.sh` (no `security-scan.sh`) → assert the hook command CONTAINS the absolute
    `fw` path and `GATEKEEPER_BIN` IS present (governed/absolute branch held end-to-end).
- **Watch red:** `env -u GATEKEEPER_BIN -u TOPOLOGY_ROOT cargo test --test cli_adapt -- cross_tree`
  → `cross_tree_dogfood_settings_are_portable` fails (today's code bakes the absolute fw path +
  pins bin); `cross_tree_partial_hooks_stays_absolute` passes already (today everything roots-differ
  is absolute) — that's fine, it's a guard that must stay green after the fix.
- **Impl:** delivered by Task 3 (the predicate change makes the portable test pass while keeping the
  partial test absolute).
- **Commit:** e2e tests are the **tests-only commit** (Task 3 step).

### Task 3 — wire `use_portable` in `cmd_adapt` + commits

- **Impl (feature-implementer), `adapt.rs` claude branch** (currently `in_framework = !roots_differ`
  at ~`:849`, `bin_opt` gated on `roots_differ` at ~`:869`):
  1. Add the `project_has_root_hooks` helper (Task 1 impl).
  2. Replace `let in_framework = !roots_differ;` with
     `let use_portable = !roots_differ || project_has_root_hooks(write_root);`
  3. Change the hooks build call to `build_claude_hooks(read_root, use_portable)`.
  4. Change `let bin_opt: Option<&str> = if roots_differ { Some(bin.as_str()) } else { None };` to
     `if use_portable { None } else { Some(bin.as_str()) }`.
  5. `disk_ok` closure unchanged (keys off `bin_opt`).
- **Commit sequencing (TDD gate needs a test-only commit first):**
  - Commit A (tests-only, red): `gatekeeper/tests/cli_adapt.rs` (the two e2e + fixture). `src/adapt.rs`
    in-file unit tests are `src/`-classified, so they ride with the code commit.
  - Commit B (green): `gatekeeper/src/adapt.rs` (helper + `use_portable` + `bin_opt` + the unit tests).
- **Watch green:** `env -u GATEKEEPER_BIN -u TOPOLOGY_ROOT cargo test --bin gatekeeper project_has_root_hooks`
  and `… --test cli_adapt -- cross_tree` → all pass; regression set green:
  `… --test cli_adapt -- ac4 ac5 adapt_writes_to_project_not_framework dogfood_settings_are_portable
  claude_writes_hook_settings readapt_removes_stale_gatekeeper_bin check_mode_is_idempotent`.
- **Commit B message:** `fix(adapt): cross-tree dogfood emits portable settings (#54)`

### Task 4 — full-suite green + hygiene + CHANGELOG

- **Impl (main loop), `CHANGELOG.md`** under `## [Unreleased]` → `### Fixed`: "adapt now emits portable
  `.claude/settings.json` not only when `read_root == write_root` but whenever the project root is
  itself a topology clone (`hooks/` at its root) — closing the cross-tree case where a `gatekeeper`
  built in one worktree adapts another clone and previously baked the generating worktree's absolute
  paths (the original sibling-worktree incident). Vendored/external governed projects (no root
  `hooks/`) are unchanged. (#54)"
- **Hygiene (BEFORE the review gate):** `env -u GATEKEEPER_BIN -u TOPOLOGY_ROOT cargo fmt --check` and
  `… cargo clippy --bin gatekeeper -- -D warnings` → both clean.
- **Finish:** `env -u GATEKEEPER_BIN -u TOPOLOGY_ROOT cargo test` → all green.
- **Commit:** `docs(changelog): cross-tree portable adapt settings (#54)`

## Gate exits after the loop

- **Verify gate:** `docs/verify/2026-06-13-adapt-cross-tree.md` — reproduce (pre-fix bakes the
  absolute `fw` path into a root-hooks project) → resolve (`${CLAUDE_PROJECT_DIR}` form, no bin);
  `cross_tree_dogfood_settings_are_portable` is the executable reproduce→resolve. One-line M2 note
  (re-adapt self-heals a pre-fix cross-tree artifact).
- **Review gate:** fresh-context critic at `docs/reviews/2026-06-13-adapt-cross-tree.md`, bound to the
  merge-base, both rubric dimensions, no blocking findings. (Run fmt/clippy first to avoid re-bind.)
- **Finish gate:** `gatekeeper check finish -- env -u GATEKEEPER_BIN -u TOPOLOGY_ROOT cargo test --manifest-path gatekeeper/Cargo.toml`.
