VERDICT: pass
HEAD: 68432dc5bfe8627ec585e824ee10363290896504
BASE: 318ea82c9f4185784104973fdcbf41acee789b7d

# Review: framework-root-resolution (2026-06-09)

## Blocking findings
None.

## Non-blocking notes
- The five `resolve_root_*` unit tests share `env::temp_dir()` with hard-coded subdir names and `remove_dir_all` at setup (e.g. `gatekeeper/src/main.rs:580`). Each name is distinct so reruns are clean, but two CI jobs on the same host could collide. The plan explicitly chose this over a `tempfile` dep (no new dependencies), so it is an accepted trade-off, not a defect.
- `gatekeeper/src/main.rs:120` includes `"gatekeeper"` as a marker. Inside the repo's own `gatekeeper/` crate dir there is no sibling `skills/`, so `is_marked_root` short-circuits on the `skills/` check first and the marker never produces a false positive — behaviour is correct, noting only that the marker is load-bearing only at the repo root.
- The verify note (`docs/verify/2026-06-09-framework-root-resolution.md:10`) reports "218 passed, 2 ignored"; reproduced locally at the reviewed HEAD (218 passed, 2 ignored). Matches.

## Criteria checked
### Spec/plan
- AC1 (stray `skills/` without a marker no longer hijacks; returns start) — `is_marked_root` (`gatekeeper/src/main.rs:122`) requires `skills/` AND a marker; covered by `resolve_root_hijack_regression` (`gatekeeper/src/main.rs:578`).
- AC2 (dir with `skills/` + marker returns that dir; nested start walks up to it) — `resolve_root` walk-up loop (`gatekeeper/src/main.rs:132`); covered by `resolve_root_marked_direct` and `resolve_root_nested_start` (`gatekeeper/src/main.rs:598`, `:614`).
- AC3 (valid `env_override` wins; non-existent override ignored, walk-up/fallback applies) — guard at `gatekeeper/src/main.rs:127` returns the override only when `o.is_dir()`; covered by `resolve_root_env_override_wins` and `resolve_root_env_override_invalid_ignored` (`gatekeeper/src/main.rs:632`, `:655`). Empty/file `$TOPOLOGY_ROOT` falls through to walk-up, matching "names an existing directory".
- AC4 (Topology repo resolves to its own root) — repo root carries `skills/` + `AGENTS.md` + `gatekeeper/` + `.claude-plugin/`, so `is_marked_root` matches at the root; subsumed by AC2 and confirmed by `cargo test` from the repo (`gatekeeper/src/main.rs:122`).
- AC5 (`cargo test`/`fmt`/`clippy` green; no new deps) — `cargo test` reports 218 passed, 2 ignored at HEAD; diff touches only existing files and adds no dependency (no `Cargo.toml` change in the diff).
- AC6 (`gatekeeper doctor` still reports `skills/: ok` in-repo) — `doctor` consumes `framework_root()` (`gatekeeper/src/main.rs:68`), which now resolves the marked repo root per AC4; recorded in the verify note (`docs/verify/2026-06-09-framework-root-resolution.md:27`).
- Testability refactor (pure `resolve_root(start, env_override)` + thin `framework_root` wrapper) — implemented exactly as the plan specifies (`gatekeeper/src/main.rs:126`, `:144`); `framework_root` supplies `env::current_dir()` and `env::var_os("TOPOLOGY_ROOT")`, no call sites changed.

### Standards
- ADR-0001 (extend Topology in place; no rewrite) — change is a localized refactor of one existing function plus tests inside the existing `gatekeeper` crate; no new module, brand layer, or dependency.
- METHODOLOGY 2.3 / AGENTS "Stack conventions" (one source, one engine; three language lanes) — production change stays in Rust under `gatekeeper/`, tests in a `#[cfg(test)]` module alongside the code, docs in Markdown; no lane crossed.
- AGENTS "Diff traceability" (surgical changes only) — `git diff --name-status` is exactly the four gate artifacts plus `src/main.rs` and the one `cli_review.rs` fixture line; the fixture's added `AGENTS.md` marker (`gatekeeper/tests/cli_review.rs:21`) is required because the resolver now demands a marker beside the scratch `skills/`. No adjacent or unrelated edits.
- AGENTS "Simplicity" — no config knobs or speculative abstraction; `ROOT_MARKERS` is a 3-entry const slice and the resolver is a single loop. A staff engineer would not call it overcomplicated.
- ADR-0006 (review artifact bound to clean HEAD + merge-base) — worktree is clean and this artifact pins the full-hex HEAD/BASE from `git rev-parse`/`merge-base`.
