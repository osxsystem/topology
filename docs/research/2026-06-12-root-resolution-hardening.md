# Research — root resolution hardening (Phase 11)

## Problem

`framework_root()` (`gatekeeper/src/main.rs:329`) still anchors resolution on an **upward marker
walk from cwd**. The 2026-06-09 fix (`docs/research/2026-06-09-framework-root-resolution.md`)
made the marker specific (`skills/` **and** one of `AGENTS.md` / `gatekeeper/` / `.claude-plugin/`,
`is_marked_root` at `main.rs:299`), which killed the original `~/skills` hijack. Three residual
weaknesses remain; Phase 11 (ROADMAP) is scoped to kill the class, not the instance.

## Current behavior (read 2026-06-12, main @ 831aa03)

`resolve_root(start, env_override)` (`main.rs:303`):

1. `$TOPOLOGY_ROOT` if set and an existing directory.
2. Walk up from cwd; first ancestor that `is_marked_root` wins; during the same walk, each
   ancestor's `.topology/` child is probed too (vendored installs).
3. Fallback: return `start` (cwd) unchanged.

Install layout (`scripts/install.sh`): global → payload at `${TOPOLOGY_HOME:-~/.topology}`,
binary at `<root>/bin/gatekeeper`; project → payload at `<project>/.topology`, same `bin/`
layout. The payload carries a `VERSION` file (`scripts/build-payload.sh:117`); a dev checkout
does not.

`doctor` (`gatekeeper/src/doctor.rs:78`) already prints framework/project/artifacts roots and
the binary path, and already **FAILs on binary↔payload version skew** (`version_skew`,
`doctor.rs:73`) — the third ROADMAP deliverable for Phase 11 shipped earlier with the
distribution work. What it does *not* do is fail when a governed project resolves
project == framework.

## Residual weaknesses

**W1 — the cwd walk is still hijackable by construction.** Any ancestor directory that happens
to contain `skills/` plus one marker beats the *actual* governing install. Concretely: a stale
framework fork or an extracted payload anywhere above cwd wins over `~/.topology`; the more
specific sentinel narrowed the false-positive surface but the trust model is unchanged — cwd
ancestry is attacker-/accident-controlled, the binary's own location is not.

**W2 — silent wrong-root fallback for governed projects outside the walk's reach.** Bare
`gatekeeper` (no `TOPOLOGY_ROOT`, hooks not involved) in a globally-governed project that is
**not** under `$HOME` (e.g. `/tmp`, a second volume, CI workspace): the walk finds nothing,
falls back to cwd → framework == project. Consequences cascade: `artifacts_root()` flips to
`<project>/docs` (the framework-repo rule, `main.rs:362`), skills/instincts silently resolve
to nonexistent paths. Today only `doctor`'s `skills/: ok` probe hints at it, and `doctor` still
exits 0 in some of these states. (When the project *is* under `$HOME`, the walk reaching `$HOME`
probes `~/.topology` as a `.topology` child — global installs work by coincidence of ancestry,
not by design.)

**W3 — the binary's own location is never consulted.** An installed binary at
`~/.topology/bin/gatekeeper` *knows* where its payload is (`current_exe()/../..`), yet
resolution ignores it and trusts cwd ancestry instead. This inversion is the root cause of
W1 and W2.

## Direction (per ROADMAP Phase 11)

Replace the cwd marker walk with deterministic locations, validated by `is_marked_root`:

1. `$TOPOLOGY_ROOT` — explicit override, kept first (see deviation note below).
2. **Binary-adjacent**: walk up from `env::current_exe()` (not cwd). Installed binary at
   `<root>/bin/gatekeeper` → `<root>`; dev binary at
   `<repo>/gatekeeper/target/{debug,release}/gatekeeper` → `<repo>`. Anchored on the
   executable path, so cwd ancestry can no longer hijack it.
3. `<project>/.topology` — vendored install, relative to `project_root()` (the `.git` walk,
   unchanged).
4. `~/.topology` — global install, by its real path, no longer dependent on cwd being under
   `$HOME`.
5. Fallback: cwd unchanged (infallible signature; `doctor` gains the loud failure — see below).

**Deviation from the ROADMAP line:** the ROADMAP lists binary-adjacent *before*
`$TOPOLOGY_ROOT`. This research recommends keeping the explicit override first:
explicit-beats-implicit is the precedence the hooks already document for `$GATEKEEPER_BIN`
("explicit override, wins when set", `hooks/skill-activation.sh:12`), it is today's behavior
(fixtures and verify replays pin roots with `TOPOLOGY_ROOT=…`), and binary-adjacent-first
would silently ignore a user's explicit pin when testing an installed binary against a dev
payload. Skew between the pinned root and the binary is the version-skew check's job, which
doctor already fails on. To be ratified in the spec / PR review.

**Doctor**: add a probe that FAILs (non-zero) when the resolved project root is a governed
project (has `.topology/` or hook wiring) yet framework == project, or when run from inside a
payload (`cwd` inside `<framework>/…` with `VERSION` present and no enclosing `.git` project).
Print which resolution step won, so "why this root?" is answerable from output.

## What must not change

- Hooks are immune by design (they `cd "$ROOT"` derived from their own path) — keep that.
- `resolve_root`'s pure-function testability seam (args only, no process state) — extend the
  signature rather than re-reading env/cwd inside.
- Framework dev repo self-governance: running from inside the repo must keep resolving to the
  repo root (binary-adjacent covers it for dev builds; `TOPOLOGY_ROOT` covers foreign-built
  binaries).
- `main.rs` is a protected path — the commit carries the documented `--no-verify` override
  (maintainer grant covers Track 2).

## Open questions for the spec

1. Ratify the `$TOPOLOGY_ROOT`-first deviation (above).
2. Binary-adjacent: bounded walk-up from `current_exe()` (handles both `bin/` and
   `target/<profile>/` depths) vs. fixed `../..` — recommend the bounded marker walk from the
   exe path; it is anchored and self-validating.
3. Fallback semantics when nothing matches: keep returning cwd (current, infallible) and let
   doctor scream, or print a one-line stderr warning on fallback in every command? Recommend
   cwd + stderr warning — gates stay usable in odd fixtures, but nothing is silent anymore.
4. Does anything depend on the *during-walk* `.topology` probe from arbitrary ancestors
   (i.e. cwd deeper than the project root, project not yet `git init`-ed)? `project_root()`
   falls back to cwd when no `.git` exists, so step 3 still probes `<cwd>/.topology` — the
   nested-dir case inside a git-less project is the only regression candidate; needs a test.
