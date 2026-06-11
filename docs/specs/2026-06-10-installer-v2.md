# Spec — installer v2: harness/scope selection, `.claude/topology/` artifacts, stale-PATH repair

**Status:** approved

## Goal

The one-liner becomes a guided install, and governing an external project stops polluting its root
`docs/`:

1. `curl -fsSL …/install.sh | bash` **asks** (when a terminal is available) which harness to wire
   — Claude Code, Codex, Cursor, OpenCode, or none — and whether to install **global**
   (`~/.topology`) or **local** (vendored into a project you name).
2. Gate artifacts for a governed external project live under **`<project>/.claude/topology/`**
   (`research/ specs/ plans/ verify/ reviews/`), never in the project's root `docs/`. The
   framework repo itself keeps its root `docs/` layout unchanged.
3. After install, no stale `gatekeeper` silently survives on PATH: a version-skewed PATH binary is
   detected, named, and (interactively) offered an in-place overwrite with the new verified one.

Grounding: [research](../research/2026-06-10-installer-v2.md),
[ADR-0012](../adr/0012-project-root-vs-framework-root.md).

## Non-goals

- `learn`, `memory`, `instinct`, `scan`, and `check docs` stay anchored to `framework_root()` —
  framework-owned state, not project artifacts.
- No legacy fallback to a governed project's root `docs/`: migration is one `git mv` per stage
  dir, documented in the USER-GUIDE. (The only affected externally-governed project is the
  operator's test bed.)
- No deletion of any pre-existing PATH binary without an explicit interactive yes.
- The Claude Code *plugin* flow is untouched (its hooks, self-provisioning, marketplace files).

## Design

### 1. Two roots in the binary (Rust)

- **`project_root()`** (new, beside `framework_root()` in `main.rs`): walk up from CWD to the
  nearest directory containing `.git` (dir *or* file — worktrees); fallback CWD. Pure helper
  `resolve_project_root(start: &Path) -> PathBuf` for unit tests, thin env wrapper, mirroring the
  existing `resolve_root` pattern.
- **`artifacts_root()`** (new): if `project_root() == framework_root()` → `project/docs`
  (framework repo: unchanged layout); else → `project/.claude/topology`.
- Threading:
  - `find_doc(sub, feature)` reads `artifacts_root().join(sub)`. Gate FAIL messages print the
    *resolved* directory (no more hardcoded `docs/plans/*` wording).
  - `review::gate_review` is called with the **project root** for all git commands and computes
    its artifact dir from `artifacts_root()`; `is_clean_review_path` accepts exactly the active
    reviews relpath (`docs/reviews/` in the framework repo, `.claude/topology/reviews/` outside),
    keeping the rename/copy fail-closed behavior.
  - `adapt`: **reads** skills/instincts/AGENTS.md from `framework_root()`, **writes** generated
    files relative to `project_root()`; generated hook paths point at the framework root's
    `hooks/*.sh`. `--check` compares against the project-side files.
  - `doctor` prints both roots and the active artifacts root; the existing probes keep their
    meaning.

### 2. Installer prompts + flags (Bash)

Flags (all optional; every prompt has a flag twin):

```
--harness <claude|codex|cursor|opencode|none>
--global                      # framework at ~/.topology (default)
--project <path>              # local: framework vendored at <path>/.topology, project wired
--yes                         # accept defaults for any unanswered prompt
--build-from-source           # unchanged
```

- **Interactivity**: prompts only when `/dev/tty` is open and readable
  (`( : < /dev/tty ) 2>/dev/null`); all `read`s use `< /dev/tty`. Without a tty every unanswered
  choice takes its default (global, harness `claude`, PATH repair warn-only) and the script prints
  the choices it assumed plus the flags that override them.
- **Scope step**: global → clone/update `${TOPOLOGY_HOME:-$HOME/.topology}` (existing behavior).
  Local → resolve `--project` path (prompt default: current directory; must contain `.git`),
  clone/update `<project>/.topology`, and append `.topology/` to the project's `.gitignore` if
  absent (noted in the manifest).
- **Harness step**: for `claude|codex|cursor|opencode`, run
  `(cd <project> && TOPOLOGY_ROOT=<framework> <framework>/bin/gatekeeper adapt --harness <h>)`
  and add the generated files to the manifest. With scope=global and no project given, the
  harness step degrades to printing the exact command to run later inside any project, plus the
  plugin alternative for claude. `none` skips wiring.
- Binary acquisition, symlink, pre-commit copy, manifest, and `doctor` close-out are unchanged
  from v1; new files (adapt outputs, `.gitignore` edit) join the manifest.

### 3. Stale-PATH repair (Bash + doctor line)

After the binary is installed and smoke-tested:

- Probe `command -v gatekeeper`. If it resolves to a path **outside** the new install whose
  `--version` output differs from the new binary's, then:
  - tty: prompt `replace <path> (<old version>) with <new version>? [y/N]` — on yes,
    `cp <new> <path>` (preserving the file, not deleting it; manifest records the overwrite);
    on no/default, print the warning.
  - no tty: print the warning naming the path, both versions, and the two remedies
    (`cp` to overwrite; or remove it from PATH).
- `doctor`: the existing `PATH gatekeeper:` probe additionally compares that binary's `--version`
  to its own and appends `(version skew: X vs Y)` when they differ — informational, not a failure.

### 4. Docs

- `README.md` + `docs/USER-GUIDE.md`: the prompt flow, the flags table, the
  `.claude/topology/` artifact layout for governed projects (with the one-time `git mv`
  migration), and the stale-PATH repair behavior.
- `docs/adr/0012-project-root-vs-framework-root.md` + index link.
- Skills that name `docs/<stage>/` paths (`write-plan`, `brainstorm-design`, `research-first`,
  `verify-before-done`, `code-review`, `resume`, `_getting-started`) say the artifact lands under
  the **artifacts root** (`docs/` in this repo, `.claude/topology/` in a governed project) —
  one-line adjustments, not rewrites.

## Acceptance criteria

1. **Framework repo unchanged**: inside this repo all seven `check` gates and `just check` pass
   with artifacts in root `docs/` exactly as on `main` (no artifact file moves here).
2. **External project artifacts**: in a scratch git repo with `TOPOLOGY_ROOT` pointing at a
   topology checkout, `gatekeeper check design --feature x` FAILs naming
   `.claude/topology/research|specs`; after placing research+spec files under
   `.claude/topology/{research,specs}/`, it PASSes. Same shape for `plan` and `verify`. Artifacts
   in the scratch repo's root `docs/` are **not** found (the old location genuinely moved).
3. **Review gate runs against the project repo**: in the scratch repo (its own git history, clean
   tree), a well-formed artifact at `.claude/topology/reviews/<date>-x.md` pinning the scratch
   repo's HEAD/merge-base passes `check review`; a dirty non-reviews file in the *scratch repo*
   fails it. The framework repo's review flow still passes against `docs/reviews/`.
4. **adapt writes to the project**: from the scratch repo with `TOPOLOGY_ROOT` set,
   `gatekeeper adapt --harness claude` creates `<scratch>/.claude/settings.json` whose hook
   commands point at the framework root's hook scripts; nothing is written under the framework
   root. `adapt --check` passes immediately after.
5. **Prompted install**: `install.sh --project <scratch> --harness claude --yes` (non-tty) vendors
   the framework at `<scratch>/.topology`, wires `<scratch>/.claude/settings.json`, appends
   `.topology/` to the scratch `.gitignore`, and the manifest lists the vendored binary, wiring
   file, and `.gitignore` edit. `--global --harness none --yes` reproduces v1 behavior
   (plus the assumed-defaults printout).
6. **Stale-PATH repair**: with a fake old-version `gatekeeper` on PATH, a non-tty run prints the
   skew warning naming the stale path and both versions and leaves the file untouched; a tty run
   piping `y` to the prompt overwrites it in place (same path, new `--version`) and records the
   overwrite in the manifest.
7. **Doctor**: reports both roots and the artifacts root; with the fake stale PATH binary present
   it appends the version-skew note and still exits 0 on an otherwise healthy tree.
8. `just check` green; `gatekeeper check docs` green; shellcheck green; all new Rust resolution
   logic unit-tested via the pure helper (tempdir fixtures, no process-global state in tests).
