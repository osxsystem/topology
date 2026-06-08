# adapters/ — per-harness mapping reference

Topology keeps **one Markdown source of truth** — `AGENTS.md` (the contract), `skills/` (keyword-routed
process skills), `instincts/` (always-on reasoning guardrails) — and **generates** each harness's native
config from it ([ADR-0003](../docs/adr/0003-one-markdown-source-per-harness-adapters.md)). This file is
the human-readable spec of *what* each harness gets and *why*; the generator is
[`gatekeeper/src/adapt.rs`](../gatekeeper/src/adapt.rs), and the concrete mapping decisions are recorded
in [ADR-0008](../docs/adr/0008-cross-harness-adapter-mappings.md).

```
gatekeeper adapt --harness <codex|cursor|opencode|claude> [--check]
```

- Without `--check`, the files below are written under the framework root.
- With `--check`, nothing is written: the adapter re-renders in memory and exits `0` if every file is
  byte-identical to disk, `1` if any is missing or has drifted. Use it in CI (Phase 6) to prove the
  committed/installed config still matches the source.

**Outputs are build artifacts — never hand-edit them.** Edit the Markdown source and regenerate. (They
are also not committed by default: `.claude/settings.json` in particular embeds absolute, machine-local
hook paths and is produced per-machine by `adapt`/`install.sh`.)

## Mapping at a glance

| Harness | Files written | The contract (`AGENTS.md`) | Skills (`skills/`) | Instincts (`instincts/`) |
|---|---|---|---|---|
| **codex** | `.codex/config.toml` | auto-discovered by Codex (no config key needed) | reached via the `AGENTS.md` pointer to `skills/` | carried in `AGENTS.md` |
| **cursor** | `.cursor/rules/*.mdc` | `agents-contract.mdc` (Always) | `skill-<name>.mdc`, Agent Requested | `instincts.mdc` (Always) |
| **opencode** | `opencode.json`, `.opencode/instincts.md`, `.opencode/skills/<name>/SKILL.md` | `instructions: ["AGENTS.md", …]` | copied verbatim (Agent Skills format) | `instructions: [".opencode/instincts.md"]` |
| **claude** | `.claude/settings.json` | native (`CLAUDE.md` → `AGENTS.md`) | native (`skills/`) | native (`gatekeeper activate`) |

## Per-harness detail

### codex → `.codex/config.toml`
Codex auto-discovers `AGENTS.md` as project instructions, so the contract needs no config key. The
generated config sets exactly one project-safe, `--strict-config`-validated key —
`project_doc_max_bytes` — so the full contract is ingested as `AGENTS.md` grows, and **nothing else**:
model/sandbox/approval choices stay in the user's `~/.codex/config.toml`, and project-local config may
not carry credential/provider/`profile` keys (Codex strips them, so the ROADMAP's "Codex profiles" idea
is invalid here). Codex reaches the skills through `AGENTS.md`'s instruction to consult `skills/`.

### cursor → `.cursor/rules/*.mdc`
Cursor has no prompt-keyword router; rules attach by path `globs` or by the agent reading a
`description`. So:
- **`instincts.mdc`** — `alwaysApply: true` (Cursor's **Always** mode): the always-on instincts.
- **`agents-contract.mdc`** — `alwaysApply: true`: the operating contract, because Cursor does not read
  `AGENTS.md` natively.
- **`skill-<name>.mdc`** — `alwaysApply: false` with a `description` and **no `globs`** (Cursor's
  **Agent Requested** mode): the agent pulls the skill in by relevance to its "Use when …" description —
  the closest primitive to Topology's keyword routing.

### opencode → `opencode.json` + `.opencode/`
- **`opencode.json`** — `{ "$schema": "https://opencode.ai/config.json", "instructions": ["AGENTS.md",
  ".opencode/instincts.md"] }`. `instructions` injects the contract and the always-on instincts as
  context.
- **`.opencode/instincts.md`** — the always-on instincts rendered as Markdown.
- **`.opencode/skills/<name>/SKILL.md`** — each `skills/*/SKILL.md` copied verbatim (OpenCode reads the
  Anthropic Agent Skills format natively; our skills are already compatible).

### claude → `.claude/settings.json`
Claude Code is the source-native harness, so `adapt` makes its install config a generated artifact too:
the hooks block (`UserPromptSubmit` → `skill-activation.sh`; `PreToolUse` on `Bash|Write|Edit|MultiEdit`
→ `security-scan.sh`) in the loadable array-of-matcher-groups schema. `scripts/install.sh` still *prints*
this block for manual paste; `adapt --harness claude` is the precise generator.

## Adding a harness

Add a `build_<harness>(root) -> Result<Vec<GenFile>, String>` to `adapt.rs`, a dispatch arm in
`cmd_adapt`, a section here, and integration tests in `gatekeeper/tests/cli_adapt.rs`. Operator content
is never re-authored — only the mapping is new.
