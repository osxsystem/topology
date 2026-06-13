# Topology framework — developer conventions

This document is for **framework contributors only** — people changing the `gatekeeper` crate,
skills, or scripts in this repo. It is never shipped in the payload and never applies to governed
projects.

## Bootstrapping a fresh clone or worktree

Run `just setup` once in any fresh clone or worktree. It (1) installs the topology git pre-commit
hook, then (2) builds the release binary and runs `gatekeeper adapt --harness claude` to regenerate
the **portable** `.claude/settings.json`.

`.claude/settings.json` is **generated, never committed** (see
[ADR-0019](adr/0019-generated-only-settings-json.md)) — `just setup` is how a fresh tree gets it,
complementing the `gatekeeper doctor` stale-path warning (#52). Re-running `just setup` rewrites
settings.json only on drift, so it is safe to run repeatedly.

The release build in `just setup` is **load-bearing, not incidental**: portable settings deliberately
omit `GATEKEEPER_BIN`, so the hooks resolve the binary via `security-scan.sh`'s fallback to
`gatekeeper/target/release/gatekeeper`. Switching the bootstrap to a debug build, or skipping it when
`gatekeeper` is merely on `PATH`, would silently leave a dev clone's security floor unwired.

## Stack conventions for this repo

- **Rust**: the `gatekeeper` crate. Run `cargo fmt` and `cargo clippy -- -D warnings` before finishing. Tests live alongside code in `#[cfg(test)]` modules.
- **Bash**: all scripts start with `set -euo pipefail`. Keep them POSIX-friendly where practical; they are the portable glue.
- **Markdown**: skills follow the house description format (see below). Keep each `SKILL.md` body under ~5k tokens; push detail into `references/`.

## Skill description house format

Every skill's `description` frontmatter:

> `<verb phrase: what it does>. Use when <concrete user-facing trigger conditions and keywords>.`

Third person, one line, real user vocabulary, slightly pushy (agents under-trigger). When a skill fails to trigger, widen its trigger language; when it over-triggers, narrow its scope.
