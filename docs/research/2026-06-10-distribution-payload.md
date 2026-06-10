# Research — distribution payload: the install is a clone of the workshop, not the tool

## Problem (operator-reported, from the installer-v2 verification against `react-weather-app`, 2026-06-10)

The v0.3.0 local install "worked" — and demonstrated that a governed project receives the wrong
thing entirely. The install log + on-disk inspection showed:

1. **The unit of install is a `git clone` of the dev repo.** `react-weather-app/.topology/`
   contains the `gatekeeper/` Rust source, the framework's own `docs/` (ARCHITECTURE, ROADMAP,
   USER-GUIDE, ADRs, *and Topology's own gate artifacts* — specs/plans/verify/reviews for the
   framework's development), `RESEARCH.md`, `METHODOLOGY.md`, `.github/`, plugin manifests, and the
   full git history. None of it serves the governed project.
2. **The one artifact the project needs is never delivered.** The operating contract lands at
   `.topology/CLAUDE.md` (symlink → `AGENTS.md`) — a file no harness reads. Claude Code loads the
   *project root's* `CLAUDE.md`; nothing in `react-weather-app/` references the contract, so the
   agent governs itself with hooks only and never sees the gate sequence.
3. **The pre-commit hook guards the wrong repo.** `install.sh` checks `[[ -d "$ROOT/.git" ]]` where
   `$ROOT` is the vendored clone, so the hook installs into `.topology/.git/hooks/pre-commit` — the
   project's own commits are unguarded.
4. **The post-install health check verifies the wrong world.** The installer `cd "$ROOT"` before
   running `doctor`; `project_root()` walks up to the nearest `.git`, finds the vendored clone, and
   reports `project root = .topology`, `artifacts root = .topology/docs`.

## Code findings (this repo, v0.3.0 HEAD)

- `scripts/install.sh:163-195` — local scope is `git clone` (or `--local` clone from a checkout);
  `:271-279` — the pre-commit bug; `:197` + `:465` — the doctor-from-inside-payload bug;
  `:460-461` — the `sudo ln -sf` PATH suggestion that makes the contract's bare `gatekeeper`
  commands optional.
- `gatekeeper/src/main.rs:177-187` — `resolve_artifacts_root` already implements the two-roots
  model (`docs/` iff project == framework, else `.claude/topology/`); the install simply never
  produces a world where the resolution is exercised correctly.
- `gatekeeper/src/learn.rs:23` — `LEDGER_REL = "docs/learn/ledger.md"` anchored to
  `framework_root()` (main.rs:66); `gatekeeper/src/memory.rs:260` — handoffs under
  `<framework_root>/memory/artifacts/` (main.rs:67). Both are mutable state inside what Track 2
  makes a wholesale-replaceable directory → silent data loss on upgrade. `learn promote` writes
  instincts/skills/rules into the same replaceable tree.
- `gatekeeper/src/memory.rs:459` — the handoff template is `include_str!`'d into the binary: the
  payload does not need `memory/TEMPLATE.handoff.md`.
- `hooks/ensure-gatekeeper.sh` + `hooks/hooks.json` — plugin-only (SessionStart self-provisioning);
  `scripts/fetch-gatekeeper.sh` reads its pinned version from `.claude-plugin/plugin.json`, which
  dies when the plugin channel is retired.
- `.github/workflows/release.yml` — a working, verified four-target binary matrix
  (aarch64/x86_64 × darwin/linux) with `SHA256SUMS`; the version-guard ties the tag to
  `Cargo.toml` + the two plugin manifests.
- Repo layout (`hooks/`, `skills/`, `instincts/`, `security/`) already equals the desired payload
  layout — assembly needs a copy list, not a restructuring.

## Comparable prior art

- ADR-0011 (prebuilt-first binary distribution) already moved the *binary* from "build from clone"
  to "fetch a release artifact"; this track applies the same move to the *operators*.
- ADR-0012 named the two roots and moved gate artifacts to `.claude/topology/`; this track extends
  the same boundary to memory/learn state (ADR-0013).
- rustup/homebrew precedent: the unit of install is a versioned, checksummed, replaceable payload;
  user state lives outside it. `releases/latest/download/<stable-asset-name>` is the standard
  GitHub pattern for "always latest without an API call".

## Options considered (grilled with the operator, 2026-06-10)

1. **Payload shape** — four per-platform tarballs (binary inside; atomic but 4× duplicated and a
   new matrix) vs **one neutral tarball + the existing binary fetch** ← chosen (B).
2. **Version resolution** — pin inside `install.sh` (reproducible but every release touches `main`,
   raw-CDN staleness) vs **always latest release + `TOPOLOGY_VERSION` override; Cargo.toml is the
   single source** ← chosen (A).
3. **Mutable state** — teach the upgrader to preserve paths inside the payload (merge problem,
   second manifest) vs **payload read-only; memory/learn move to `artifacts_root()`; `promote`
   framework-only until the overlay exists** ← chosen (ADR-0013).
4. **Legacy clones** — refuse + document manual migration vs **rescue state → prompt → replace**
   ← chosen (Phase 8).

## Conclusion

Introduce a distribution payload distinct from the repository (spec:
[2026-06-10-distribution-payload.md](../specs/2026-06-10-distribution-payload.md)), then rebuild
the install/integration layer on top of it (ROADMAP Track 2, Phases 7–12). The gatekeeper gate
logic, skills, instincts, and hook scripts survive unchanged; what changes is what users receive
and where mutable state lives.
