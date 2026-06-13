# Research: idempotent setup-time `gatekeeper adapt` (auto-wire fresh clones/worktrees)

- **Date:** 2026-06-13
- **Feature slug:** auto-adapt-on-setup
- **Issue:** #58 (corrective complement to #52; decision recorded in ADR-0019)

## Question

Make a fresh clone/worktree self-wire its `.claude/settings.json` (portable hook paths, correct/absent
`GATEKEEPER_BIN`) with zero manual steps, where re-running on an already-correct tree is a silent
no-op. Heavy exploration was delegated to a research subagent; the two load-bearing claims were
re-verified directly against source.

## Headline finding

**`gatekeeper adapt --harness claude` is already idempotent and already emits portable settings by
default for the dogfood case.** The genuinely missing piece is a *trigger* that runs it automatically
on a fresh clone/worktree — **#58 is trigger-wiring, not new `adapt` logic.**

## Findings (cited)

### `adapt` is already idempotent (verified directly)
- Apply mode writes settings.json **only on drift**: the write is guarded by `} else if !disk_ok {`
  at `gatekeeper/src/adapt.rs:930`. When `disk_ok` is true, nothing is written → re-run is a literal
  no-op (`adapt.rs:907-944`).
- `disk_ok` compares only the two managed keys (`hooks` wholesale + `env.GATEKEEPER_BIN`); all other
  user keys are preserved and never count as drift (`adapt.rs:907-923`; `merge_claude_settings`
  `adapt.rs:156-196`).
- `--check` (dry-run) mode prints `DRIFT .claude/settings.json` and sets exit 1 on drift, else 0
  (`adapt.rs:925-929`). Flag parsed at `adapt.rs:812-815`; dispatch `main.rs:168-173`.
- read_root = `framework_root()`, write_root = `project_root()` (`main.rs:168-173`, `:442-459`).

### Portable settings converge for both dogfood shapes
- `use_portable = !roots_differ || project_has_root_hooks(write_root)` (`adapt.rs:849-856`);
  `project_has_root_hooks` requires both root hook scripts present (`adapt.rs:526-529`).
- Portable emits `${CLAUDE_PROJECT_DIR}/hooks/<name>.sh` and drops `GATEKEEPER_BIN`
  (`build_claude_hooks` `adapt.rs:536-562`; `bin_opt = None` `adapt.rs:871-880`).
- Self-governed repo and sibling worktrees both converge to a correct portable settings.json on a
  plain re-run — covered by tests `dogfood_settings_are_portable` (`cli_adapt.rs:648`) and
  `cross_tree_dogfood_settings_are_portable` (`cli_adapt.rs:723`); a partial clone stays absolute
  (`cross_tree_partial_hooks_stays_absolute` `cli_adapt.rs:760`).

### The gap is the trigger
- **`just setup` (justfile:13-28, verified)** only installs/updates the git pre-commit hook. It does
  **not** call `adapt` and does **not** wire `.claude/settings.json`. This is the framework-dev-clone
  gap #58 targets.
- **`scripts/install.sh` already calls adapt** for local-scope governed installs
  (`install.sh:653-671`: `… "$BIN" adapt --harness "$HARNESS"`). Governed onboarding is already
  covered — no change needed there.
- **No git `post-checkout` hook exists**; `.git/hooks/` holds only the pre-commit copy; `lefthook.yml`
  defines only `pre-commit` + `pre-push`, no setup/checkout automation. A fresh sibling worktree
  currently gets its settings.json from nothing automatic.
- `docs/DEVELOPMENT.md` says nothing about adapt / settings.json / worktree bootstrap (grep: 0 hits).

### Tests on hand (for later TDD)
- `gatekeeper/tests/cli_adapt.rs`: `check_mode_is_idempotent_then_detects_drift` (`:148`, opencode),
  `ac4_settings_no_clobber` (`:510`, claude write-then-`--check`-clean), the portability tests above.
- **Coverage gap relevant to #58:** no test asserts a *claude* settings.json apply re-run is a literal
  no-op (only opencode has that), and none covers absolute→portable convergence on re-run.

## Risks / unknowns

- **Binary must exist before `just setup` can call adapt.** A fresh framework clone has no built
  binary yet; the setup recipe currently does not build. The trigger must build-then-adapt or guard on
  the binary's presence. (Design decision.)
- **Running adapt in *this* dogfood clone will converge its currently absolute/pinned settings.json to
  portable** — a visible diff, but the desired end state (ADR-0019, generated-only). Expected, not a
  defect.
- **Three writers to `.git/hooks/pre-commit`** (`just setup`, `install.sh`, lefthook) already collide;
  a new trigger must not add a fourth writer to that file — it should only call `adapt` (settings.json
  + adapter files), not touch the pre-commit hook.
- **`${CLAUDE_PROJECT_DIR}` runtime expansion** is assumed (tests assert the literal string, not
  runtime resolution); Claude Code sets it. Out of scope to re-verify here.
- A `post-checkout` hook would fire on every branch switch (worktrees share the main hooks dir) —
  noisy and easy to get wrong; weigh against the `just setup` approach in design.

## Recommended direction (for design)

Implement #58 as a **`just setup` enhancement** that builds the binary (if needed) and runs
`gatekeeper adapt --harness claude` after the pre-commit install — idempotent by construction, portable
by default, settings.json-only (never a fourth pre-commit writer). Document the wiring in
`docs/DEVELOPMENT.md`. Governed projects already get this via `install.sh`. Defer/decline a
`post-checkout` git hook. Add the two missing TDD tests (claude apply no-op; absolute→portable
convergence).
