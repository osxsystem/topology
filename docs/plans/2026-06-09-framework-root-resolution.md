# Plan — robust framework-root resolution

Implements `docs/specs/2026-06-09-framework-root-resolution.md`. Test-first; each step is a small,
verifiable commit's worth of work. No new dependencies.

## Step 1 — Extract a pure, testable resolver (red)

In `gatekeeper/src/main.rs`, add a unit test module covering `resolve_root(start, env_override)`
before it exists, encoding the three acceptance cases:
- a chain whose only `skills/` dir lacks any marker resolves to `start` (hijack regression);
- a dir with `skills/` + `AGENTS.md` resolves to that dir, including from a nested `start`;
- a present `env_override` wins; a non-existent `env_override` is ignored (walk-up/fallback applies).

Tests build fixtures under `std::env::temp_dir()` with unique per-test subdirectories (no reliance
on process cwd). Run `cargo test` and watch the new tests fail (red).

## Step 2 — Implement `resolve_root` (green)

Add:

```rust
const ROOT_MARKERS: &[&str] = &["AGENTS.md", "gatekeeper", ".claude-plugin"];

fn is_marked_root(dir: &Path) -> bool {
    dir.join("skills").is_dir()
        && ROOT_MARKERS.iter().any(|m| dir.join(m).exists())
}

fn resolve_root(start: &Path, env_override: Option<&Path>) -> PathBuf {
    if let Some(o) = env_override {
        if o.is_dir() { return o.to_path_buf(); }
    }
    let mut dir = start.to_path_buf();
    loop {
        if is_marked_root(&dir) { return dir; }
        if !dir.pop() { return start.to_path_buf(); }
    }
}
```

Run `cargo test`; the new tests go green and all existing tests stay green.

## Step 3 — Rewire `framework_root` as a thin wrapper

Replace the body of `framework_root()` so it delegates:

```rust
fn framework_root() -> PathBuf {
    let start = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let env_override = env::var_os("TOPOLOGY_ROOT").map(PathBuf::from);
    resolve_root(&start, env_override.as_deref())
}
```

No call sites change. Run `cargo test`, `cargo fmt --check`, `cargo clippy -- -D warnings`.

## Step 4 — Manual verification in-repo and cross-repo

- From the repo root: `gatekeeper list` shows the 12 Topology skills; `gatekeeper doctor` reports
  `skills/: ok`.
- From an external dir under `$HOME` (e.g. a temp dir): `gatekeeper list` no longer prints `~/skills`
  entries — it falls back to cwd (no skills/ there) rather than hijacking to `$HOME`.
- `TOPOLOGY_ROOT=<repo> gatekeeper list` from anywhere shows the 12 skills.

## Step 5 — Gate artifacts and close-out

Write the verify note `docs/verify/2026-06-09-framework-root-resolution.md` with the commands and
their output. Run the `code-review` gate, then `check finish -- cargo test`. Commit, push, open a PR.
