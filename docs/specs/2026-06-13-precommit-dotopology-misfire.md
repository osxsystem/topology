# Design: fix pre-commit hook blocking commits on a stray `.topology/` (issue #60)

- **Date:** 2026-06-13
- **Feature slug:** precommit-dotopology-misfire
- **Status:** approved
- **Issue:** #60
- **Research:** docs/research/2026-06-13-precommit-dotopology-misfire.md

## Problem

`hooks/pre-commit.sh:28-32` exports `TOPOLOGY_ROOT="$ROOT/.topology"` whenever a `.topology/`
directory merely *exists* at the repo root. In the self-governed framework repo, that `.topology/` is
a **deliberate, non-marked** directory — it holds `CONTRACT.md`, which `CLAUDE.md` imports
(`@.topology/CONTRACT.md`, the contract-split) — but it has no `skills/` markers and no
`security/rules.toml`. So the override points `gatekeeper scan --staged` at a nonexistent rules file
→ fail-closed → **every commit is blocked** (the symptom that forced a `TOPOLOGY_ROOT="$PWD"`
workaround on every commit this session).

**Deleting `.topology/` is not the fix** — it is load-bearing (`CLAUDE.md` imports its `CONTRACT.md`).
A non-marked `.topology/` is now a permanent, by-design feature of the framework repo, which is
precisely what makes the hook's bare-`-d` existence check provably wrong.

Root cause (verified): the Bash guard gates on bare directory existence, while the binary's own
resolution gates on `is_marked_root` (`main.rs:381`). The hook *overrides* the binary's correct
resolution with a wrong path. The export is **redundant** — `gatekeeper scan` already resolves its
root via the same `framework_root()` ladder and loads rules from there.

Success: a commit in a self-governed clone with a stray `.topology/` succeeds; a genuine governed
project (marked `.topology/` payload) still scans against `.topology`; no `TOPOLOGY_ROOT` workaround
needed.

## Constraints

- **Three-language lanes.** The bug *is* a lane violation: framework-root resolution logic living in
  Bash, duplicating `is_marked_root`/`resolve_root` in Rust. The fix removes that logic from Bash and
  leaves resolution to the binary (the enforcement lane).
- **Surgical.** Delete the redundant export block only; leave the binary-resolution ladder
  (`:13-26`, which finds the *binary*, a separate concern) and the `cd "$ROOT"` + `scan --staged`
  untouched.
- **Constraints-as-reasoning.** Replace the deleted block with a short comment explaining *why* the
  hook must not pin `TOPOLOGY_ROOT` (the binary resolves: governed → `ProjectVendored`, framework →
  `SelfGoverned`), so a future maintainer cannot silently re-introduce the bug.
- **No regression to governed projects.** The "rules from framework root, staged files from cwd repo"
  separation the old comment wanted must still hold without the export (verified: scan computes the
  staged repo from cwd independently of `framework_root()`).
- **Non-goals.** Not adding a new `gatekeeper root`/`--print-root` surface (option C); not changing
  `resolve_root`; not touching `install.sh` (independent); not building a new shell-test harness for a
  5-line deletion.

## Approaches considered

1. **Delete the `TOPOLOGY_ROOT` export, rely on the binary (recommended).** Remove
   `hooks/pre-commit.sh:28-32`; replace with a why-comment. The binary's `is_marked_root`-gated ladder
   already resolves correctly in both cases (empirically verified: without the export, `scan --staged`
   exits 0 in this repo; with it, exit 2). Smallest diff; fixes the lane violation at its source.
   *Trade-offs:* relies on the binary being present and resolving correctly — which it is, and which
   is the design intent. Regression guard is a Rust unit test, not a hook-runner.

2. **Tighten the Bash guard** to require `-f "$ROOT/.topology/security/rules.toml"`.
   *Trade-offs:* fixes the symptom but keeps root-resolution logic in Bash that duplicates (and can
   drift from) `is_marked_root` — the lane violation persists, and it needs new shell-test infra.
   Rejected.

3. **Hook asks the binary for the resolved root.** No machine-readable root command exists today;
   needs new Rust surface for no benefit over (1). Rejected.

## Decision

**Approach 1.** Delete `hooks/pre-commit.sh:28-32` and replace it with a comment stating that the
binary resolves the framework root itself (so the hook must not pin `TOPOLOGY_ROOT`, or it
re-introduces #60). Pin the binary's robustness with one `resolve_root` unit test, and prove the
end-to-end hook fix (and governed no-regression) at the verify gate.

### Regression guard (TDD-honesty)

The actual fix is a **Bash deletion**; the binary's resolution already works (no Rust behavior
changes). So:
- **Rust unit test (`main.rs`) — the negative gate (the one genuinely-uncovered branch).** The
  framework-repo case is already pinned by Step-2 precedence
  (`resolve_root_self_governed_beats_project_vendored_topology`, `main.rs:2315`), and the governed
  positive case by `resolve_root_finds_project_vendored_topology` (`main.rs:2283`). A test of "marked
  root + non-marked `.topology/` → SelfGoverned" would be a **hollow pin**: it short-circuits at Step 2
  (`main.rs:358-363`) and never reaches the `is_marked_root` gate at `main.rs:381`, so it asserts
  nothing new. Instead add the **negative gate**: an *unmarked* project root with a *non-marked*
  `.topology/` child → `resolve_root` does **not** return `ProjectVendored` (it falls through to
  `GlobalHome`/`Fallback`). That case exercises line 381's *false* branch — the one branch no current
  test covers, and the gate that makes a stub-`.topology/` safe. (Set `exe_path=None`/away from the
  fixture so Step 3 `BinaryAdjacent` doesn't pre-empt the gate.)
- **Verify gate (the real red→green) — through the *deployed* hook, not the source.** `just setup`
  (and `install.sh:622`) *copy* `hooks/pre-commit.sh` into `.git/hooks/pre-commit`, so editing the
  source does not change the live hook until `just setup` re-runs. The verify gate therefore:
  1. reproduces in-repo (the misfire is live now): `TOPOLOGY_ROOT="$PWD/.topology" gatekeeper scan
     --staged` → exit 2;
  2. redeploys the fixed hook via `just setup` (expect a side effect: it also re-runs `adapt`, which
     may rewrite `.claude/settings.json` on drift);
  3. resolves: stage a **trivial non-protected** change (e.g. a temp file — *not* the fix's own
     protected-path diff, so this isolates the #60 rules-resolution fix from the separate, intended
     protected-path veto) and run a real `git commit` → the deployed hook's `scan` now resolves
     self-governed → succeeds;
  4. governed no-regression fixture using the **vendored-binary layout** (`.topology/bin/gatekeeper` +
     a marked `.topology/` payload with `security/rules.toml`): run the binary from that bin path with
     no `TOPOLOGY_ROOT` → it resolves `.topology` via Step 3 `BinaryAdjacent` (the path the hook
     actually takes, not Step 4) and loads `.topology` rules.

  A dedicated shell hook-runner *unit* harness is deliberately *not* added — disproportionate to a
  5-line deletion, and the binary (the enforcement) is unit-tested; the deployed-hook commit above is
  the end-to-end proof.

## Landing mechanics (protected paths)

The entire diff edits **protected paths**: `hooks/pre-commit.sh` (`rules.toml` protected_paths) and
`gatekeeper/src/main.rs`. This dominates how the fix lands:

- Each `Edit`/`Write` to those files trips the `PreToolUse` **ask** gate (human approval) — expected,
  per the CONTRACT.
- The commit is **double-blocked**: (a) by the #60 misfire itself until the fixed hook is redeployed,
  and (b) by the pre-commit **protected-path veto** on the staged diff (which edits protected files) —
  a *separate, working-as-intended* control that the #60 fix does not and should not remove.
- Landing therefore requires `git commit --no-verify` — a deliberate, **authorized** security-floor
  bypass, documented in the commit message (Phase-14 protected-path-override pattern). **Maintainer
  authorized `--no-verify` for this #60 fix** (2026-06-13); the grant is scoped to this fix only and
  must be re-asked for future protected-path work. Each commit message records the override.
- The replacement why-comment in `hooks/pre-commit.sh` must be **plain prose** (no command-like or
  secret-like tokens) so it does not itself trip a scanner rule in the staged diff.

## Risks & open questions

- **Governed-project regression.** Dropping the export must not break scanning in a real governed
  project. The production path is **Step 3 `BinaryAdjacent`**, not Step 4: the hook prefers the
  vendored binary at `$ROOT/.topology/bin/gatekeeper` (`hooks/pre-commit.sh:17`), so `resolve_root`
  walks up from `.topology/bin` and returns the marked `.topology` at Step 3 before Step 4 is reached
  (same resulting path). The verify fixture must use the vendored-binary layout to exercise this, not
  the `exe_path=None` Step-4-in-isolation path that `main.rs:2283` covers. Low risk; pinned at verify.
- **Governed project mid-install with a non-marked `.topology/` stub.** After the fix, such a tree
  falls through to `GlobalHome`/`Fallback` rather than pinning the stub. This is **not** a regression:
  the old export pointed at the same stub (no `rules.toml`) and failed identically. The fix assumes a
  governed `.topology/` is a *full marked payload*; a half-installed stub was never scannable either way.
- **Binary absent at commit time.** If no `gatekeeper` binary resolves, the hook already errors before
  this block (`:23-25`) — unchanged. Not introduced by this fix.
- **The `.topology/bin/gatekeeper` rung** in the binary-finder ladder (`:17`) is untouched — it finds
  the binary, not the framework root; out of scope.

## Acceptance criteria

- [ ] After redeploying the fixed hook via `just setup`, a real `git commit` (trivial non-protected
      change staged) in this repo — which has the deliberate non-marked `.topology/` — succeeds where
      it previously failed. (verify gate, reproduce-then-resolve through the *deployed* hook)
- [ ] A genuine governed project (vendored-binary layout: `.topology/bin/gatekeeper` + marked
      `.topology/` payload) still resolves `.topology` rules without `TOPOLOGY_ROOT` — via Step 3
      `BinaryAdjacent`. (verify gate fixture)
- [ ] `resolve_root` unit test — the **negative gate**: unmarked project root + non-marked
      `.topology/` child → result is **not** `ProjectVendored` (`main.rs`, exercises line 381's false
      branch; not the hollow Step-2-short-circuit pin).
- [ ] `hooks/pre-commit.sh` no longer pins `TOPOLOGY_ROOT`; the deleted block is replaced by a
      plain-prose comment explaining why (so #60 cannot silently return).
- [ ] No change to `install.sh` or `resolve_root` logic; the binary-finder ladder and `cd`+`scan` are
      untouched.
- [ ] The commit records the authorized `--no-verify` protected-path override in its message.
