# Plan — installer v2

Spec: [docs/specs/2026-06-10-installer-v2.md](../specs/2026-06-10-installer-v2.md).
Decision record: [ADR-0012](../adr/0012-project-root-vs-framework-root.md).
Branch: `feat/installer-v2` (worktree `topology-installer-v2`). One commit per task, conventional
prefixes, `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>` trailer. Finish with `just check`.

## Conventions for all tasks

- Rust: every new resolution function gets a pure testable core (`resolve_*(start, …) -> PathBuf`)
  with a thin env/CWD wrapper, exactly like the existing `resolve_root`. Unit tests use distinct
  tempdirs, no `env::set_var`, no process-global state.
- Tests first per unit of behavior (tdd gate): write the failing test, watch it fail, make it pass.
- Bash: `set -euo pipefail`; every prompt reads `< /dev/tty`; every prompt has a flag twin; no
  interactive path is reachable when `/dev/tty` cannot be opened.
- The review-gate hardening must survive: rename/copy porcelain entries stay fail-closed, the
  clean-path filter stays anchored to the *active* reviews relpath only.

## Task 1 — `project_root()` + `artifacts_root()` (Rust core)

- In `main.rs` beside `framework_root()`: `resolve_project_root(start: &Path) -> PathBuf` — walk
  up from `start` to the first dir where `.git` exists (`is_dir() || is_file()`); fallback
  `start`. Wrapper `project_root()` supplies `env::current_dir()`.
- `artifacts_root() -> PathBuf`: `framework_root()` equality check (compare canonicalized paths;
  fall back to plain equality when canonicalize fails) → `project_root().join("docs")` when equal,
  `project_root().join(".claude").join("topology")` otherwise. Pure core
  `resolve_artifacts_root(project: &Path, framework: &Path) -> PathBuf`.
- Unit tests: `.git` dir found / `.git` file (worktree) found / no `.git` → start; equal roots →
  `docs`; differing roots → `.claude/topology`.
- Check: `cargo test` green; `gatekeeper check design --feature installer-v2` still passes inside
  the repo (equal-roots path proves itself).

## Task 2 — Thread the artifacts root through the doc gates

- `find_doc` uses `artifacts_root().join(sub)`; gate FAIL messages print the resolved directory
  (e.g. `FAIL plan gate: no <dir>/*x*.md found`), replacing hardcoded `docs/…` strings in
  `gate_plan`, the design/verify arms of `cmd_check`, and the `--help`/module docs where they
  promise a location.
- Integration test (new or extended `tests/cli_check.rs`-style): scratch git repo + scratch
  framework dir (with `skills/` + `AGENTS.md` marker), `TOPOLOGY_ROOT` env on the spawned command;
  assert FAIL names `.claude/topology/`, then artifacts placed there flip design/plan/verify to
  PASS, and the same files under scratch `docs/` are ignored.
- Check: spec AC-2 executed by the new test; existing in-repo gate tests stay green (AC-1).

## Task 3 — Review gate + adapt on the project root

- `main.rs` dispatch: `review::gate_review(&project_root(), …)`; inside `review.rs` compute the
  artifact dir via the artifacts root (pass it in as an argument — keep `gate_review`'s signature
  explicit, no hidden globals) and derive the clean-path prefix from that same value
  (`docs/reviews/` or `.claude/topology/reviews/`).
- `adapt::cmd_adapt(args, read_root, write_root)`: read skills/instincts/AGENTS.md from
  `framework_root()`, write/`--check` against `project_root()`; `build_claude` hook command paths
  use the framework root.
- Integration tests: (a) scratch-repo review pass/fail per spec AC-3 (well-formed artifact pinning
  the scratch HEAD passes; a dirty non-reviews file fails; framework-repo fixtures unchanged);
  (b) adapt writes `<scratch>/.claude/settings.json` pointing at framework hooks, writes nothing
  under the framework root, `--check` passes after (AC-4).
- Check: `cargo test` green including the untouched review fixtures.

## Task 4 — `doctor`: both roots + PATH version skew

- Print `framework root:`, `project root:`, `artifacts root:` lines (read-only).
- The `PATH gatekeeper:` probe runs that binary with `--version` (already the smoke pattern) and
  appends ` (version skew: <theirs> vs <ours>)` when it differs from `version::tool()`;
  informational — `failures` is not incremented.
- Extend `tests/cli_doctor.rs`: output contains the three root lines; with a fake `gatekeeper`
  stub prepended to PATH that prints an old version, the skew note appears and exit stays 0.
- Check: spec AC-7 via the test.

## Task 5 — Installer prompts, scope, harness wiring (Bash)

- Arg parsing: `--harness <h>`, `--global`, `--project <path>`, `--yes`, existing
  `--build-from-source`; reject unknown flags with usage. `--global` and `--project` are mutually
  exclusive.
- `can_prompt()`: `( : < /dev/tty ) 2>/dev/null`. `ask(question, default)` reads `< /dev/tty`.
  Non-tty or `--yes`: defaults (scope=global, harness=claude when wiring a project, else print-only
  guidance; PATH repair=warn), echoed as `assumed: …` lines with the overriding flag named.
- Scope: global → current clone/update of `${TOPOLOGY_HOME:-$HOME/.topology}`. Local → validate
  the project path contains `.git`; clone/update `<project>/.topology`; append `.topology/` to
  `<project>/.gitignore` when absent (manifest entry).
- Harness: `claude|codex|cursor|opencode` → `(cd <project> && TOPOLOGY_ROOT=<framework>
  "<framework>/bin/gatekeeper" adapt --harness <h>)`; parse adapt's output paths into the
  manifest. Global scope without a project: print the one-liner to run later from any project +
  the plugin alternative; `none`: skip.
- Check (AC-5): non-tty `--project <scratch> --harness claude --yes` with the file:// fixture →
  vendored framework, wired settings, `.gitignore` line, manifest lists all three; non-tty
  `--global --harness none --yes` reproduces v1 output + assumed-defaults lines; shellcheck green.

## Task 6 — Stale-PATH repair (Bash)

- After the smoke test: `found="$(command -v gatekeeper || true)"`; skip when empty, inside the
  new install, or same `--version`. Otherwise tty → `replace …? [y/N]` (`cp "$BIN" "$found"` on
  yes + manifest entry); non-tty → warning block naming path, both versions, and the `cp` remedy.
- Check (AC-6): fixture PATH dir with a stub printing `gatekeeper 0.0.1`; non-tty run leaves the
  stub byte-identical and prints the warning; tty run scripted with `printf 'y\n' > /dev/tty`
  unavailable in CI, so simulate tty by running the repair function in a `bash -c` harness with
  `/dev/tty` redirected from a here-string via `socat`-free fallback: factor the repair into
  `repair_stale_path()` reading from a `PROMPT_INPUT_FD` that defaults to `/dev/tty`, and test by
  setting the fd — the production default stays `/dev/tty`.

## Task 7 — Docs + skill phrasing

- README + USER-GUIDE: prompt flow, flags table, `.claude/topology/` layout + `git mv` migration,
  stale-PATH repair, two-roots model (a short table: what anchors to framework vs project).
- Skill one-liners (`write-plan`, `brainstorm-design`, `research-first`, `verify-before-done`,
  `code-review`, `resume`, `_getting-started`): artifact paths phrased as "under the artifacts
  root (`docs/` here, `.claude/topology/` in a governed project)".
- ROADMAP addendum line under the Phase 6 addendum pointing at ADR-0012 + the verify doc;
  create the verify stub (Task 8 overwrites).
- Check: `gatekeeper check docs` green; `just check` green.

## Task 8 — Verify + review artifacts, finish (main loop, not the implementer)

- Re-run AC-1…AC-8 independently; record in `docs/verify/2026-06-10-installer-v2.md`.
- Fresh-context critic → `docs/reviews/2026-06-10-installer-v2.md`; `check review` on clean HEAD.
- `gatekeeper check finish -- just check`; push; PR.
