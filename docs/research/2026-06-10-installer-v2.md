# Research — installer v2: harness/scope prompts, project-local artifact root, stale PATH binary

## Problem (operator-reported, from the first live cross-project test, 2026-06-10)

The v0.2.0 one-liner installed cleanly, but wiring it to an external project
(`react-weather-app`) surfaced three gaps:

1. The installer makes every decision silently: it cannot ask which harness to wire (Claude Code,
   Codex, Cursor, OpenCode) or whether the install is **global** (`~/.topology`, shared) or
   **local** (pinned inside one project).
2. Gate artifacts (`specs/`, `plans/`, `verify/`, `reviews/`, `research/`) land in the governed
   project's root `docs/` directory. The operator wants them under **`.claude/topology/`** so a
   governed project's own `docs/` stays untouched.
3. A stale `~/.cargo/bin/gatekeeper` (0.1.0, from an old `cargo install`) sits on PATH.
   `doctor` reports it; nothing flags the version skew, and `skill-activation.sh` resolves PATH
   before the repo build — in an external project a stale PATH binary silently routes skills.

## Root cause of (2): one root where there are two

Every gate anchors to `framework_root()`:

- `find_doc` (`main.rs:427`) — `framework_root().join("docs").join(sub)` — research/specs/plans/
  verify gates.
- `review::gate_review` is called with `&framework_root()` (`main.rs:284`): the reviews dir
  (`review.rs:273`), **and all git commands** (`git -C root`, `review.rs:188`), and the
  clean-tree filter that special-cases `docs/reviews/` (`review.rs:208-214`).
- `adapt::cmd_adapt` reads skills/AGENTS.md from its root **and writes the generated configs to
  the same root** (`adapt.rs:42`, `apply_or_check`).

So in a wired external project (`TOPOLOGY_ROOT=~/.topology`), gates would read artifacts from —
and `adapt` would write configs into — `~/.topology`, not the project. The only reason the live
test "worked" is that it ran *without* `TOPOLOGY_ROOT`, letting `framework_root()` fall back to
the project dir — which then put artifacts in the project's root `docs/`. The conflation, not the
location, is the underlying defect: **framework root** (where `skills/`, `security/rules.toml`,
`instincts/` live) and **project root** (the repo being governed) are different directories in
every cross-project setup.

Framework-repo-only surfaces correctly stay on `framework_root()`: the docs-coverage lint
(`check docs`, `main.rs:331/366/380` — ADR index, ROADMAP), `learn` (ledger + `docs/learn/`),
`memory` artifacts (`memory/artifacts/`, gitignored runtime state), `instinct`, `scan` rules.

## Interactive prompts under `curl … | bash`

When bash runs a piped script, **stdin is the script text** — a bare `read` consumes script lines.
The portable pattern is prompting from the controlling terminal: `read -r ans < /dev/tty`, guarded
by interactivity detection (`[ -e /dev/tty ] && [ -t 1 ]` is not enough alone — `-t 0` is false
under a pipe by definition, so the check must be tty-writability of `/dev/tty` itself, e.g.
`( : < /dev/tty ) 2>/dev/null`). Every prompt needs a flag equivalent so CI/non-tty runs are
deterministic: `--harness`, `--global`/`--project <path>`, and silent defaults when no tty
(global + claude + warn-only), printed so the user knows what was chosen.

## Stale-PATH handling options

The 0.1.0 at `~/.cargo/bin/gatekeeper` is user data — deleting it unprompted is out. Options:
(a) warn only; (b) interactive offer to overwrite that file with the freshly verified binary
(`cp new old`), defaulting to no; (c) always shadow via a `~/.local/bin` symlink. (c) depends on
the user's PATH order and silently adds a third copy; (b) fixes the actual skew at its source with
consent; (a) is the non-interactive floor. Conclusion: (b) interactive, (a) non-interactive.

## Layout precedent for `.claude/topology/`

`.claude/` is already the per-project agent-config home (settings, worktrees) and is the path the
operator explicitly chose. Subdirs mirror the framework's stage names one-to-one
(`research/ specs/ plans/ verify/ reviews/`) so skill text ("a plan exists at `plans/…`") needs no
per-location phrasing. Inside the framework repo itself the artifacts stay in root `docs/` — its
CI, docs-coverage lint, and history depend on that layout, and there the project *is* the
framework.

## Conclusion → design seams

| Ask | Seam |
|---|---|
| Project-local artifacts under `.claude/topology/` | New `project_root()` (nearest `.git` ancestor of CWD) + `artifacts_root()` = `docs/` iff project == framework, else `.claude/topology/`; thread through `find_doc`, the review gate (dir, git `-C`, clean filter), and `adapt`'s write side |
| Harness choice at install | Prompt (tty) / `--harness` flag → run the existing `adapt` generator for the chosen project |
| Local vs global | Prompt / flags: global = `~/.topology`; local = framework vendored at `<project>/.topology` + project wiring |
| No stale PATH binary | Post-install version-skew probe: interactive overwrite offer, non-interactive warning; `doctor` states the skew |
