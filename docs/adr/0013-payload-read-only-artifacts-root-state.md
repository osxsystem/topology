# 0013 — The payload is read-only at runtime; mutable state anchors to the artifacts root

- **Status:** 🟢 Accepted
- **Date:** 2026-06-10

## Context

Track 2 (ROADMAP Phases 7–12) replaces the clone-based install with a **distribution payload**: a
platform-neutral tarball of operators (`skills/`, `instincts/`, `security/rules.toml`, hooks) that
is the unit of install and is **replaced wholesale on upgrade** — no `git pull`, no merge. The
local-customization overlay was explicitly deferred; forking the framework is the interim
customization story.

That upgrade model collides with two write paths: `learn capture` appends the gotcha ledger to
`<framework_root>/docs/learn/ledger.md` (`learn.rs`), and `memory write` puts handoff artifacts
under `<framework_root>/memory/artifacts/` (`memory.rs`). In a governed project the framework root
*is* the payload (`<project>/.topology/`), so every upgrade would silently delete the project's
learned gotchas and in-flight handoffs. `learn promote` is worse: it writes new instincts, skills,
and scan rules into the payload, which the next upgrade also deletes.

## Decisions

1. **The payload is read-only at runtime.** `gatekeeper` never writes inside the framework root
   after install (`bin/` is populated once, at install time). Anything mutable belongs to the
   project, not the payload.
2. **Memory handoffs and the learn ledger anchor to `artifacts_root()`, not `framework_root()`.**
   In a governed project they land in committed project state: `.claude/topology/memory/` and
   `.claude/topology/learn/ledger.md` — beside the gate artifacts they relate to. In the framework
   repo (`artifacts_root()` = `docs/`) the ledger path is **unchanged** (`docs/learn/ledger.md`);
   only the repo's `memory/artifacts/` migrates to `docs/memory/` (one `git mv`).
3. **`learn promote` is framework-only.** Its targets (`instincts/`, `skills/`,
   `security/rules.toml`) live inside the payload. In a governed project, `promote` refuses with a
   pointer to the fork story ("the gotcha is safe in the ledger; promote it in your framework
   fork") instead of writing a file the next upgrade deletes. Capture keeps working everywhere —
   the ledger is project state.

## Alternatives considered

- **Teach the upgrader to preserve specific paths inside the payload.** Rejected: every upgrade
  becomes a merge problem, and the preserved-path list is a second, drift-prone manifest — exactly
  the clone-era entanglement Track 2 removes.
- **Ship the customization overlay now** (project-owned `skills/` shadowing the payload's, giving
  `promote` a safe target). Deferred, not rejected: it is the natural lift of decision 3, but it is
  scope beyond what the reference fixture (`react-weather-app`) needs to validate the payload model.

## Consequences

- Upgrades are trivially safe: delete the payload, unpack the new one. Nothing of the project's is
  inside.
- A project's learning history and handoffs are committed and reviewable alongside its specs,
  plans, and reviews — one artifacts root holds all mutable Topology state.
- The learning loop's *promotion* half is unavailable in governed projects until the overlay
  exists; the *capture* half is unaffected. This is a deliberate, visible refusal rather than a
  silent data loss.
- Installer v3 must rescue legacy state (clone-era `memory/artifacts/*`, `docs/learn/ledger.md`)
  into the artifacts root before replacing a clone-based `.topology/` (ROADMAP Phase 8).
