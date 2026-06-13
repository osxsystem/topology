VERDICT: pass
HEAD: 9ad4286a63f1b37df1ca32ea1aaf55db4901f046
BASE: 8e19256b7abb612ddbfe9de56fe0b91ba024e4ed

# Review: precommit-dotopology-misfire (2026-06-13)

## Blocking findings
None.

## Non-blocking notes
- `hooks/pre-commit.sh:28` — the why-comment is solid, but it asserts the governed path resolves "binary-adjacent from .topology/bin" without noting that this depends on the binary-finder ladder preferring `.topology/bin/gatekeeper` (line 17). The verify doc covers this; the inline comment is a slight over-simplification but not misleading.
- `gatekeeper/src/main.rs:2364` — the test relies on `env::temp_dir()` not being inside a git repo so `resolve_project_root` leaves `base` unmarked. True on macOS/Linux CI, but a contributor running tests from an unusual temp dir under a marked tree could see step 2 fire. Pre-existing pattern in sibling tests; acceptable.

## Criteria checked
### Spec/plan
- Hook no longer pins `TOPOLOGY_ROOT`, deleted block replaced by plain-prose why-comment naming #60 — confirmed at `hooks/pre-commit.sh:28-32`: the `if [[ -z ... && -d ... ]]; then export TOPOLOGY_ROOT` block is gone, replaced by a 5-line prose comment ending "(issue #60)". No command-like or secret-like tokens.
- Negative-gate unit test asserts a non-marked `.topology/` child of an unmarked root does NOT resolve `ProjectVendored` — confirmed at `gatekeeper/src/main.rs:2345-2378`. Traced: env_override=None (step 1 skip), `is_marked_root(base)` false (no skills/, step 2 false branch), exe_path=None (step 3 skip), `is_marked_root(base/.topology)` false at the line-381 gate (only CONTRACT.md, no skills/ — the previously-uncovered false branch), empty fake_home (step 5 false), → Fallback. `assert_ne!(ProjectVendored)` + `assert_eq!(Fallback)`. Genuine pin of line 381's false branch, not a step-2 short-circuit. Test runs and passes (`1 passed`).
- No change to install.sh or resolve_root logic; binary-finder ladder + cd+scan untouched — confirmed: `git diff` on install.sh/scripts/install.sh is empty; `resolve_root` (main.rs:340-404) unchanged (only a test added in `mod tests`); binary-finder ladder (hooks lines 13-26) unchanged; `cd "$ROOT" && "$GK" scan --staged` byte-identical to BASE (line 34 both).
- Commit records authorized --no-verify override — plan/spec document the Phase-14 protected-path-override; outside the reviewable code diff, recorded in commit messages per design Landing-mechanics.

### Standards
- Three-language-lanes (ADR-0012, AGENTS surgical-changes) — the fix REMOVES root-resolution logic from Bash and adds zero new Bash logic; resolution now lives solely in Rust's `resolve_root`/`is_marked_root`. Net Bash change is a deletion + comment. Correctly resolves the lane violation at source.
- Surgical / diff-traceability — diff is exactly the export deletion + replacement comment + one unit test; no adjacent cleanup or scope creep. Reads as the requested change and nothing else.
- ADR-0012 project-vs-framework-root — claim verified by reading `resolve_root`: WITHOUT the env export, self-governed framework repo resolves via step 2 SelfGoverned (repo is a marked root); a governed project resolves `.topology` via step 3 BinaryAdjacent (binary preferred at `.topology/bin/gatekeeper`, hooks line 17) before step 4 — same resulting path. No other caller depends on the hook's export: all remaining `TOPOLOGY_ROOT` references are test harnesses setting it as an explicit step-1 EnvOverride (unaffected) and the manual human-override path. Dropping the export does not break governed scanning.
