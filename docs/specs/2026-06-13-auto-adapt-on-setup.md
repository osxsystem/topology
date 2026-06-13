# Design: idempotent setup-time `gatekeeper adapt` (auto-wire fresh clones/worktrees)

- **Date:** 2026-06-13
- **Feature slug:** auto-adapt-on-setup
- **Status:** approved
- **Issue:** #58 (corrective complement to #52; decision recorded in ADR-0019)
- **Research:** docs/research/2026-06-13-auto-adapt-on-setup.md

## Problem

ADR-0019 keeps `.claude/settings.json` generated-only (never committed). That leaves an onboarding
gap: a fresh framework clone or sibling worktree has no automatic step that wires its settings.json,
so the developer must remember to run `gatekeeper adapt --harness claude` by hand — and if they
don't, the hooks silently point nowhere (the worktree-portability incident). #52 (shipped) is the
*detective* control that warns about it; #58 is the *corrective* control that wires it up front.

Research established the key fact: **`adapt` is already idempotent and emits portable settings by
default** (write-on-drift-only at `adapt.rs:930`; portable hooks at `adapt.rs:536-562,849-880`). The
missing piece is purely a **trigger** — `just setup` (justfile:13-28) installs only the pre-commit
hook and never calls `adapt`. Governed projects are already covered by `install.sh:653-671`.

Success: after the framework-clone bootstrap (`just setup`), `.claude/settings.json` exists and is
correct (portable, no stale paths) with zero manual `adapt`; re-running bootstrap on an
already-correct tree changes nothing.

## Constraints

- **Three-language lanes.** The trigger is Bash *glue* in the justfile that builds the binary and
  invokes the Rust `adapt` — no logic in the recipe (no path computation, no conditionals beyond
  invocation). The wiring decision lives in `adapt` (Rust); the doc lives in Markdown.
- **Idempotent where it matters.** Re-running changes **settings.json only on drift** (the true
  no-op, `adapt.rs:930`). The recipe is not silent overall on re-run — it always re-copies the
  pre-commit hook ("updated") and always re-invokes `cargo build` ("Finished", an incremental no-op).
  Only the settings.json write is a genuine no-op (this is what AC-2 asserts).
- **No fourth writer to `.git/hooks/pre-commit`.** Three managers already collide there
  (`just setup`, `install.sh`, lefthook). The new step calls `adapt` only — `adapt` writes
  settings.json + adapter files, never the pre-commit hook.
- **Generated-only invariant (ADR-0019).** The step *generates* settings.json; it is never committed.
- **Surgical.** Enhance the existing `setup` recipe + document it; no new Rust behavior.
- **Claude-only wiring, deliberately.** The recipe hardcodes `--harness claude`. A contributor using
  codex/cursor/opencode gets (harmless) claude wiring, not theirs — accepted: this is the dogfood
  bootstrap for the reference harness, not a per-developer harness selector.
- **Non-goals.** No `post-checkout` git hook (see Decision); no change to `install.sh` (governed
  onboarding already runs `adapt`); not changing `adapt`'s logic; not committing settings.json.

## Load-bearing invariants (why "purely a trigger" is true)

The claim that #58 needs no new `adapt` logic holds only because four invariants align today. They
are named here so a future change cannot silently break them:

1. **Root resolves without `TOPOLOGY_ROOT`.** The recipe runs `adapt` with no `TOPOLOGY_ROOT` (unlike
   `install.sh:657`, which sets it). Safe because a clone/worktree root is a *marked root* (`skills/`
   + `AGENTS.md`/`gatekeeper`), so `framework_root()` resolves via the self-governed step (and
   binary-adjacent as an independent second path) — the unrelated `~/skills` hijack path is never
   reached (`main.rs:300,340-404`).
2. **The build is load-bearing, not incidental.** Portable mode *drops* `GATEKEEPER_BIN`
   (`adapt.rs:876-880`), so the hooks find the binary only via `security-scan.sh`'s fallback to
   `$ROOT/gatekeeper/target/release/gatekeeper` (`security-scan.sh:39`). **The build must run first
   and must produce the `release` binary** — switching it to debug, or skipping it when `gatekeeper`
   is merely on PATH, would silently break the security floor in a dev clone. (Portable is in fact
   *more* robust than today's pinned bin: it tries release→debug→PATH.)
3. **Self-governed `adapt` writes only settings.json.** The `.topology/`/CONTRACT.md/CLAUDE.md
   scaffold block is guarded by `if roots_differ` (`adapt.rs:962`); self-governed (`roots_differ ==
   false`) touches only `.claude/settings.json`. The pre-existing stray `.topology/` is unrelated and
   left untouched.
4. **No fourth pre-commit writer.** `adapt` never writes `.git/hooks/pre-commit` — only `just setup`,
   `install.sh`, and lefthook do (`lefthook.yml:10-26`). The new step calls `adapt` only.

## Approaches considered

1. **Enhance `just setup` (recommended).** After the pre-commit install, the recipe runs
   `cargo build --release --manifest-path gatekeeper/Cargo.toml` then
   `gatekeeper/target/release/gatekeeper adapt --harness claude`, and documents the bootstrap in
   `docs/DEVELOPMENT.md`. Build-then-adapt makes a brand-new clone fully zero-touch (maintainer's
   chosen trade-off); `cargo build` no-ops when up-to-date, `adapt` no-ops when settings are correct.
   *Trade-offs:* adds a one-time multi-minute compile to a fresh clone's first `just setup`; simplest,
   one recipe, no new git plumbing, idempotent by construction.

2. **Dedicated `just wire` recipe that `setup` invokes.** Factor build+adapt into its own recipe so
   wiring can be re-run without re-touching the pre-commit hook.
   *Trade-offs:* slightly cleaner separation and a handy standalone `just wire`, but adds recipe
   surface for a two-line sequence. Deferred — can be extracted later if a standalone re-wire is
   wanted; folding it into `setup` now is the smaller diff.

3. **`git post-checkout` hook.** Auto-wire on every worktree/branch checkout.
   *Trade-offs:* none exists today; worktrees share the main hooks dir so it fires on every branch
   switch (noisy); collides with the existing pre-commit-hook management story. **Rejected** — the
   issue asks for a verdict on a worktree-level hook, and this is it: decline.

## Decision

**Approach 1.** Enhance the `setup` recipe to: install the pre-commit hook (unchanged) → build the
release binary → run `gatekeeper adapt --harness claude`. Document the bootstrap in
`docs/DEVELOPMENT.md` — **linking** ADR-0019 for the generated-only invariant rather than restating
it (m4), and stating **why the build is load-bearing** (M2 / invariant 2: portable hooks resolve
`target/release/gatekeeper`, so dropping `GATEKEEPER_BIN` is safe *only* because the release build
ran first) — mirrored as a one-line comment in the recipe. Add an Unreleased `CHANGELOG.md` entry
(m3). Governed projects keep getting this via `install.sh` (no change). Decline the `post-checkout`
hook with rationale.

### Shape (recipe sketch — exact text in the plan)

```make
setup:
    # 1. install/update the topology pre-commit hook (UNCHANGED) — its own shell line, first
    # 2. cargo build --release --manifest-path gatekeeper/Cargo.toml
    # 3. ./gatekeeper/target/release/gatekeeper adapt --harness claude
```

**Ordering & failure semantics (M1).** `just` runs each recipe line in a separate shell and aborts on
the first non-zero exit. The pre-commit install is its **own line, first**, so it always lands. The
build is its own line: **a build failure is fatal and loud** (open question 2) — the recipe stops
before `adapt`, leaving the hook installed and `adapt` skipped. Rationale: a half-wired clone that
*looks* fine is worse than a red, explained `just setup`. Always-build (not build-if-missing, open
question 1): a `[ -x … ] || build` guard is exactly the conditional Bash that three-language-lanes
forbids, the warm-run tax is a few seconds, and always-build guarantees `adapt` runs the current
binary.

### TDD note (gate honesty)

`adapt` already implements the idempotent-no-op and absolute→portable convergence this feature relies
on (verified: `adapt.rs:930`; tests `dogfood_settings_are_portable` `cli_adapt.rs:648`). So #58 adds
**no new Rust behavior** — the new artifacts are Bash glue (the recipe) and Markdown (the doc), plus
**characterization tests** in `cli_adapt.rs` that pin the relied-upon behavior against regression.

- The tdd iron-law ("watch it fail before the code exists") governs *new* production code. The two
  added tests are explicitly *test-after of existing code* (characterization), since the behavior
  already ships — stated here rather than dressed up as red→green theater.
- The recipe is glue with no logic to unit-test; its end-to-end correctness is proven at the
  **verify gate** by running the bootstrap in a scratch clone and observing settings.json appear /
  converge.

## Risks & open questions

- **First-setup compile time.** A fresh clone's first `just setup` now compiles gatekeeper (minutes).
  Accepted for true zero-touch (maintainer's call). Subsequent runs are incremental no-ops.
- **Converges *this* dogfood clone's settings.json to portable.** Its current settings.json is
  absolute/pinned; the first `adapt` will rewrite it to portable `${CLAUDE_PROJECT_DIR}` form with no
  `GATEKEEPER_BIN`. Expected and desired (ADR-0019), but it is a visible diff on next setup.
- **Partial/sparse checkout** lacking both root hook scripts would make `adapt` emit absolute (not
  portable) paths (`project_has_root_hooks`, `adapt.rs:526-529`). Out of scope — a full clone/worktree
  has them.

## Acceptance criteria

- [ ] After `just setup` in a fresh clone/worktree (no prior settings.json), `.claude/settings.json`
      exists and is portable (`${CLAUDE_PROJECT_DIR}` hook paths, no clone-pinned `GATEKEEPER_BIN`),
      with no manual `adapt` invocation. (verify gate, end-to-end)
- [ ] Re-running `just setup` on an already-wired tree writes nothing to settings.json and exits 0
      (idempotent). (characterization test + verify gate)
- [ ] Characterization test (`cli_adapt.rs`), using the **single-root self-governed harness**
      (`scratch_root` like `dogfood_settings_are_portable:648`, *not* the governed `run_proj` path of
      `ac4_settings_no_clobber:510`): a *claude* `adapt` apply re-run is a no-op when settings are
      already correct — asserted explicitly (no `wrote` line on the 2nd apply, **or** `--check`
      returns 0 after the write) so it does not duplicate `ac4` (m5).
- [ ] Trigger points are decided and wired: `just setup` (new) + `install.sh` (already); the
      `post-checkout` worktree hook is explicitly declined with rationale. (this doc + DEVELOPMENT.md)
- [ ] `docs/DEVELOPMENT.md` documents the bootstrap, **links** ADR-0019 for the generated-only
      invariant (m4), and states why the release build is load-bearing (M2).
- [ ] An Unreleased `CHANGELOG.md` entry records the `just setup` build+adapt enhancement (m3).
