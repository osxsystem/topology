# Spec — root resolution hardening (Phase 11)

**Status:** approved
**Research:** `docs/research/2026-06-12-root-resolution-hardening.md`

## Goal

`framework_root()` resolves only from locations the binary can justify — an explicit pin, its
own install, the project's vendored payload, the global home install, or a self-governed
framework checkout — never from arbitrary cwd ancestry. `doctor` answers "why this root?" and
fails loudly when resolution is broken instead of letting gates misbehave quietly.

## Resolution algorithm (precedence order)

Implemented in a pure helper; each candidate must pass `is_marked_root` (unchanged predicate:
`skills/` + one of `AGENTS.md` / `gatekeeper/` / `.claude-plugin/`) except step 1:

1. **`$TOPOLOGY_ROOT`** — if set and an existing directory, return it verbatim (no marker
   check, current semantics: an explicit pin is obeyed; `doctor` reports marker problems).
2. **Self-governed project** — if `project_root()` (the `.git` walk, unchanged) is itself a
   marked root, return it. Covers the framework dev repo and forks governing themselves,
   regardless of which binary runs. Anchored at `.git`, not arbitrary ancestry.
3. **Binary-adjacent** — walk up from `env::current_exe()`'s directory; first marked ancestor
   wins. Installed layout `<root>/bin/gatekeeper` → `<root>`; dev build
   `<repo>/gatekeeper/target/<profile>/gatekeeper` → `<repo>`. The exe path is not
   cwd-derived, so cwd ancestry cannot influence this step.
4. **`<project>/.topology`** — vendored install at the project root, if marked.
5. **`~/.topology`** — the global install by its real path (works for projects outside
   `$HOME`, fixing research W2), if marked.
6. **Fallback** — return the start directory (cwd) unchanged, printing **one line to
   stderr**: `gatekeeper: no framework root found; falling back to <path> (run 'gatekeeper
   doctor')`. The signature stays infallible; nothing is silent anymore (research Q3).

The bare cwd marker walk and the during-walk `.topology` probing of every ancestor are
**removed**. The only cwd influence left is `project_root()`'s nearest-`.git` walk (steps
2 and 4), which is anchored at a repository boundary, and the identity fallback (step 6).

### Ratified deviations / decisions (research Q1–Q4)

- **Q1 — `$TOPOLOGY_ROOT` stays first** (ROADMAP listed binary-adjacent first): explicit
  beats implicit, matching the hooks' documented `$GATEKEEPER_BIN` precedence and today's
  fixture/replay usage. Skew between a pinned root and the binary is already a `doctor` FAIL
  (`version_skew`). Flagged for maintainer ratification in the PR description.
- **Q2 — binary-adjacent is a marker walk from the exe path**, not a fixed `../..`: handles
  both `bin/` and `target/<profile>/` depths, and self-validates via `is_marked_root`.
- **Q3 — fallback keeps returning cwd** plus the stderr warning above. Gates remain usable in
  odd fixtures; `doctor` provides the hard failure.
- **Q4 — nested cwd in a git-less project**: `project_root()` falls back to cwd when no
  `.git` exists, so step 4 probes `<cwd>/.topology` only — a `.topology` on a *deeper*
  unrelated ancestor no longer wins. Covered by an explicit test.
- **Step 2 is new vs. the ROADMAP list**: without it, a contributor running an *installed*
  binary from inside the framework repo would resolve `~/.topology` and reroute artifacts to
  `.claude/topology/` instead of `docs/`. Self-governance must not depend on binary
  provenance.

## Testability refactor

`resolve_root` becomes pure over **all** of its inputs and reports which step won:

```rust
pub(crate) enum RootSource {
    EnvOverride, SelfGoverned, BinaryAdjacent, ProjectVendored, GlobalHome, Fallback,
}

pub(crate) struct ResolvedRoot { pub path: PathBuf, pub source: RootSource }

fn resolve_root(
    start: &Path,                 // cwd
    env_override: Option<&Path>,  // $TOPOLOGY_ROOT
    exe_path: Option<&Path>,      // env::current_exe().ok()
    home: Option<&Path>,          // $HOME
) -> ResolvedRoot
```

`framework_root()` stays a thin wrapper returning `.path` (emitting the fallback warning when
`source == Fallback`); `doctor` calls a sibling wrapper that exposes the full `ResolvedRoot`.
Unit tests drive `resolve_root` over `tempdir` fixtures only — no process state.

## Doctor changes

- Print one new line after `framework root:`: `resolved by: <env override | self-governed
  project | binary-adjacent | project .topology | global ~/.topology | fallback (cwd)>`.
- **F1 (FAIL, non-zero):** the resolved framework root is not a marked root (fallback landed
  on a plain directory, or a pinned/changed root lost its markers) — message names the path
  and the missing pieces.
- **F2 (FAIL, non-zero):** project == framework **and** the root carries a `VERSION` file —
  the user is running from inside an installed payload (e.g. cwd inside a cloned
  `~/.topology`), so "their project" is the payload itself. Message says to cd into the real
  project. The dev checkout (project == framework, marked, **no** VERSION) remains OK — that
  is the supported self-governance mode.
- The existing binary↔payload `version_skew` FAIL (`doctor.rs:73`) is unchanged and covers
  the ROADMAP's third deliverable; AC below keeps it green.

## Out of scope / unchanged

- Hooks: still `cd "$ROOT"` from their own path; no hook edits.
- `is_marked_root` predicate, `project_root()` / `resolve_project_root()`,
  `resolve_artifacts_root()` rule: unchanged.
- No shadow mode: this is infrastructure resolution, not a gate verdict — `doctor` is a
  diagnostic command whose new failures are the deliverable, and gate behavior changes only
  where resolution was already wrong (research W1/W2). Noted per Track 3 doctrine.
- No new dependencies (ADR-0007). `main.rs` is protected — implementation commits carry the
  documented `--no-verify` override per the Track 2 grant.

## Acceptance criteria

1. **Hijack-class regression:** a marked directory that is an ancestor of cwd but not the
   project root, with no other candidate, no longer resolves — `resolve_root` returns
   `Fallback(start)`. (The 2026-06-09 instance test is superseded by class removal.)
2. `$TOPOLOGY_ROOT` wins over all other candidates; a nonexistent override is ignored.
3. Exe at `<root>/bin/gatekeeper` (marked `<root>`) → `BinaryAdjacent(<root>)`; exe at
   `<repo>/gatekeeper/target/release/gatekeeper` (marked `<repo>`) → `BinaryAdjacent(<repo>)`.
4. A marked project root beats binary-adjacent (`SelfGoverned`), so the dev repo resolves to
   itself even under an installed binary.
5. `<project>/.topology` resolves for a vendored project when steps 1–3 miss; a `.topology`
   on a deeper ancestor outside the project root does not (Q4 test).
6. `~/.topology` resolves for a governed project **outside** `$HOME` (W2 regression test).
7. Fallback returns `start`, `framework_root()` emits exactly one stderr warning line, and
   gates still run.
8. `doctor` prints `resolved by:` and exits non-zero in two fixtures: F1 (no root anywhere)
   and F2 (cwd inside a payload clone with `VERSION`); exits zero in the dev checkout and in
   a healthy governed fixture.
9. Existing suite, `cargo fmt --check`, `cargo clippy -- -D warnings` green; `version_skew`
   tests untouched; no new dependencies.
