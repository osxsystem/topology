# Research: pre-commit hook blocks commits on a stray `.topology/` (issue #60)

- **Date:** 2026-06-13
- **Feature slug:** precommit-dotopology-misfire
- **Issue:** #60

## Question

In the self-governed framework repo, a stray `.topology/` stub at the repo root makes
`hooks/pre-commit.sh` export `TOPOLOGY_ROOT="$ROOT/.topology"`; the stub has no
`security/rules.toml`, so `gatekeeper scan --staged` fails to load rules and fails closed →
every commit is blocked. What is the smallest correct fix, in the right language lane?

Heavy exploration delegated to a subagent; the top-risk claim (the binary resolves correctly
*without* the export) was verified empirically.

## Findings (cited)

### The bug is the Bash override
- `hooks/pre-commit.sh:28-32` exports `TOPOLOGY_ROOT="$ROOT/.topology"` gated only on **directory
  existence** (`-d "$ROOT/.topology"`), not on whether `.topology/` is a real payload. `just setup`
  copies this file to `.git/hooks/pre-commit` (`justfile:22,30`).
- The stray repo `.topology/` holds only `CONTRACT.md` (no `security/rules.toml`), so scan reads a
  nonexistent rules file → `scan.rs:580-587` "cannot load …" → exit 2 → hook blocks.

### The binary already resolves correctly without the env var
- `resolve_root` (`main.rs:340-404`) is a 6-step ladder. Step 2 `SelfGoverned` selects the project
  root when it is a marked root; Step 4 `ProjectVendored` selects `<project>/.topology` **only when
  `is_marked_root(.topology)`** (`main.rs:381`) — *gated on markers, not bare existence*, unlike the
  Bash guard.
- `is_marked_root` (`main.rs:297-301`) = `skills/` dir + (`AGENTS.md` or `gatekeeper`). A full
  vendored payload has all of these + `security/rules.toml` (`scripts/build-payload.sh:88-124`); the
  stray stub has none.
- So without `TOPOLOGY_ROOT`: framework repo → Step 2 `SelfGoverned` (repo has `skills/`+`AGENTS.md`),
  stray stub never examined; genuine governed project → Step 4 `ProjectVendored` picks the marked
  `.topology`. Unit tests already cover both: `resolve_root_self_governed_when_project_root_is_marked`
  (`main.rs:2173`) and `resolve_root_finds_project_vendored_topology` (`main.rs:2283`).
- `scan` takes its root from `&framework_root()` at dispatch (`main.rs:157`) — the same ladder — and
  loads rules from `root/security/rules.toml` (`scan.rs:580-587`) while computing the staged-files
  repo from cwd separately. So the explicit export is **redundant**: dropping it preserves the
  "rules from framework root, staged files from cwd repo" separation the `:28-29` comment wanted.

### Empirical reproduce-then-resolve (verified directly, in this repo)
```
# WITHOUT TOPOLOGY_ROOT (proposed fix): resolves self-governed, loads repo rules
$ env -u TOPOLOGY_ROOT gatekeeper scan --staged   →  exit 0
# WITH TOPOLOGY_ROOT=.topology (current hook behavior): the bug
$ TOPOLOGY_ROOT="$PWD/.topology" gatekeeper scan --staged
gatekeeper scan: cannot load …/.topology/security/rules.toml: … (os error 2)   →  exit 2
```

### Testing surface
- `resolve_root` unit tests live in `main.rs` (`#[cfg(test)]` ~2137+); CLI resolution tests in
  `gatekeeper/tests/cli_root_resolution.rs` (drives `doctor` with `TOPOLOGY_ROOT` removed).
- **No test runs `hooks/pre-commit.sh`** today (only `just shell` shellcheck). A focused regression
  belongs as a `resolve_root` unit test: "marked project root + stray non-marked `.topology/` child →
  `SelfGoverned`."
- `install.sh` only sets `TOPOLOGY_ROOT` inline for one-shot install invocations (`:657,:677,:865`);
  it does **not** write it into the hook → no matching change needed there.

## Fix options

- **(A) Tighten the Bash guard** to require `.topology/security/rules.toml`. Fixes the symptom but
  keeps framework-root logic in Bash that duplicates `is_marked_root` (lane violation) and needs new
  shell-test infra.
- **(B) Delete the `TOPOLOGY_ROOT` export** (`hooks/pre-commit.sh:28-32`) and rely on the binary's
  resolution. **The code already fully supports this** (verified above); smallest diff; moves
  resolution out of Bash into Rust (three-language-lanes). Regression = one `resolve_root` unit test.
- **(C) Hook asks the binary for the root.** No machine-readable root command exists today (only
  `doctor`'s human output) — needs new Rust surface for no benefit over (B).

## Recommendation

**(B).** The export is redundant with, and strictly worse than, the binary's `is_marked_root`-gated
ladder. Verify the governed-project path empirically at the verify gate (consuming repo with a marked
`.topology` payload, `scan --staged` without `TOPOLOGY_ROOT` → loads `.topology` rules, scans cwd repo).
