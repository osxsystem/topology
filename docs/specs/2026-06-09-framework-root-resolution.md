# Spec — robust framework-root resolution

## Goal

`framework_root()` must resolve to a genuine Topology root, must not be hijacked by an unrelated
ancestor `skills/` directory, and must offer an explicit override. Behaviour inside the Topology
repo and its forks is unchanged.

## Resolution algorithm (in precedence order)

1. **Env override.** If `$TOPOLOGY_ROOT` is set and names an existing directory, return it.
   (Mirrors the `$GATEKEEPER_BIN` override precedent. A set-but-invalid value is ignored, not fatal —
   resolution continues to the walk-up, consistent with the function's infallible signature.)
2. **Marked walk-up.** From the start directory, walk up to the filesystem root. Return the first
   ancestor that is a *marked Topology root*: it contains a `skills/` directory **and** at least one
   of these markers: `AGENTS.md`, `gatekeeper/`, or `.claude-plugin/`.
3. **Fallback.** If no marked root is found, return the start directory (the current behaviour's
   fallback). An external project then reads its own `docs/`, as a user would expect.

## Why these markers

The Topology root uniquely carries all three of `AGENTS.md` (the agent contract), `gatekeeper/` (the
engine), and `.claude-plugin/` (the plugin manifest). A fork "you own" keeps `AGENTS.md` and the
skills; a thin install may drop the source — accepting *any one* marker tolerates these modes. A
stray `~/skills` has none, so it is skipped.

## Testability refactor

`framework_root()` currently reads `env::current_dir()` and (after this change) `env::var` directly,
which is why it has no unit tests — process cwd/env are global and racy under parallel tests.

Extract a pure helper:

```rust
fn resolve_root(start: &Path, env_override: Option<&Path>) -> PathBuf
```

It implements steps 1–3 against its arguments only (no process state). `framework_root()` becomes a
thin wrapper that supplies `env::current_dir()` and `env::var("TOPOLOGY_ROOT")`. Unit tests drive
`resolve_root` over `tempdir` fixtures.

## Acceptance criteria

1. `resolve_root` with a `skills/`-only dir in the chain and **no** marker returns the start dir
   (the hijack regression: a stray `skills/` no longer wins).
2. `resolve_root` over a dir containing `skills/` + a marker returns that dir; nested start dirs walk
   up to it.
3. `resolve_root` with a valid `env_override` returns it regardless of the walk-up; an override that
   does not exist is ignored and the walk-up/fallback applies.
4. The existing Topology repo continues to resolve to its own root (covered by 2).
5. `cargo test`, `cargo fmt --check`, `cargo clippy -- -D warnings` all green; no new dependencies.
6. `gatekeeper doctor` still reports `skills/: ok` from within the repo.
