# Topology framework — developer conventions

This document is for **framework contributors only** — people changing the `gatekeeper` crate,
skills, or scripts in this repo. It is never shipped in the payload and never applies to governed
projects.

## Stack conventions for this repo

- **Rust**: the `gatekeeper` crate. Run `cargo fmt` and `cargo clippy -- -D warnings` before finishing. Tests live alongside code in `#[cfg(test)]` modules.
- **Bash**: all scripts start with `set -euo pipefail`. Keep them POSIX-friendly where practical; they are the portable glue.
- **Markdown**: skills follow the house description format (see below). Keep each `SKILL.md` body under ~5k tokens; push detail into `references/`.

## Skill description house format

Every skill's `description` frontmatter:

> `<verb phrase: what it does>. Use when <concrete user-facing trigger conditions and keywords>.`

Third person, one line, real user vocabulary, slightly pushy (agents under-trigger). When a skill fails to trigger, widen its trigger language; when it over-triggers, narrow its scope.
