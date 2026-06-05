# 0003 — One Markdown source; generate per-harness configs

- **Status:** Accepted
- **Date:** 2026-06-04

The system must run natively across Claude Code, Codex, Cursor, and OpenCode, which read different
config files in different formats. We will keep **one Markdown source of truth** (`AGENTS.md` +
`skills/` + `instincts/` + `security/rules.toml`) and **generate** each harness's native config via
`gatekeeper adapt`. `AGENTS.md` is the lingua franca for the three harnesses that read it natively;
Cursor, which does not, receives generated `.cursor/rules/*.mdc` carrying the same content.

## Why

Hand-maintaining four parallel configs guarantees drift. Flattening to a lowest-common-denominator
format throws away each harness's strengths (Claude Code hooks, Codex orchestrator agents, Cursor
globs, OpenCode MCP). Generating from one source gives portability *and* native idioms.

## Consequences

- Adapters are generators; their outputs are build artifacts, never hand-edited.
- Adding a harness means adding a template, not re-authoring operator content.
- Humans and agents edit Markdown; they never edit the generated JSON/TOML/`.mdc`.
