# 0019 — `.claude/settings.json` stays generated-only, never committed

- **Status:** 🟢 Accepted
- **Date:** 2026-06-13
- **Resolves:** [#53](https://github.com/osxsystem/topology/issues/53) · detective complement [#52](https://github.com/osxsystem/topology/issues/52) (merged #57) · corrective complement [#58](https://github.com/osxsystem/topology/issues/58) · depends on portable settings #50/#51 (merged #55/#56)

## Context

Once the dogfood `.claude/settings.json` became portable (#50 portable hook paths, #51 no
clone-pinned `GATEKEEPER_BIN`), it stopped leaking a clone-specific absolute path — so committing it
to version control became *safe*. The open question (#53): should the framework commit its own
dogfood `settings.json`?

`settings.json` is a **protected path** — the scan gate guards exactly this hook/binary wiring
(`Write`/`Edit` to it is gated behind human approval; Bash mutations of it are denied). The file is
currently *untracked* (not gitignored; `settings.local.json` and the task lock are the ignored
ones). Committing it would change a standing invariant: **settings.json is always *generated* by
`gatekeeper adapt`, never hand-authored or hand-reviewed.**

The trade-off:

- **Commit it** — hook-wiring changes become reviewable in git history; new clones/worktrees are
  wired with zero setup. But it breaks the generated-only invariant and adds a protected path the
  scan gate must sign off on every change.
- **Don't commit it** — keep settings.json generated-only; close the worktree-onboarding gap with
  the #52 doctor stale-path warning plus a cheap, idempotent auto-`adapt` at setup time.

Live evidence weighing on the decision: while shipping #52, a stray untracked `.topology/` in a
self-governed clone made the pre-commit hook resolve scan rules from a nonexistent
`.topology/security/rules.toml` and **blocked every commit**. That is exactly the per-clone wiring
drift that tracking more wiring in the tree tends to invite — a concrete reminder that the
generated-only boundary is load-bearing, not ceremonial.

## Decision

**Do not commit `.claude/settings.json`. It stays untracked and generated-only — produced by
`gatekeeper adapt`, never hand-authored.** The generated-only invariant is preserved.

The worktree/clone onboarding gap that committing the file would have closed is instead covered by
**two complementary controls**, matching the [weakest-enforcement-that-works] instinct (a detective
warning + a corrective regenerate, rather than a new protected tracked artifact):

1. **Detective — #52 (shipped, #57).** `gatekeeper doctor` emits an advisory `WARN:` when a hook
   `command` or `GATEKEEPER_BIN` path in settings.json no longer exists on disk, naming the stale
   path. It tells an operator their wiring is broken before it surfaces as a runtime hook error.

2. **Corrective — #58 (filed).** An idempotent, setup-time `gatekeeper adapt --harness claude` (wired
   into `just setup` and/or `scripts/install.sh`) so a fresh clone/worktree self-wires a correct
   portable `settings.json` with zero manual steps, and re-running on an already-correct tree is a
   silent no-op.

## Consequences

- The generated-only invariant holds: `settings.json` is never reviewed as committed source, and the
  scan gate is not asked to sign off on a tracked copy of its own wiring on every change.
- Onboarding a fresh clone/worktree no longer depends on a human remembering to run `adapt` — once
  #58 lands, setup converges the wiring automatically; until then, #52 surfaces the gap loudly rather
  than letting it ambush mid-session.
- Hook-wiring changes remain **invisible in git history** (the con of this option). That cost is
  accepted: wiring is *derived* from the committed contract + adapter logic (one Markdown source per
  ADR-0003), so the reviewable artifact is the generator, not its per-clone output.
- This decision is reversible: if invisibility of wiring drift later proves costly, committing the
  (now portable) file remains on the table — the portability work that made it safe is already done.
