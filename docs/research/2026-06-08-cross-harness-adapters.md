# Research: Cross-harness adapters (Phase 4)

- **Date:** 2026-06-08
- **Feature slug:** cross-harness-adapters
- **Question:** What is the *current, authoritative* native-config format for Codex, Cursor, and
  OpenCode, so `gatekeeper adapt` can generate configs that actually load — and how do Topology's
  operators (the `AGENTS.md` contract, keyword-routed `skills/`, always-on `instincts/`) map onto each
  harness's real primitives? Grounds [ADR-0008](../adr/0008-cross-harness-adapter-mappings.md) and the
  [spec](../specs/2026-06-08-cross-harness-adapters.md).

> Knowledge cutoff is Jan 2026 and these harnesses move fast, so every claim below is backed by a 2026
> docs source or by probing the **locally installed** `codex` binary. No parametric guessing
> (instinct: [[evidence-over-assertion]]).

## Cursor — `.cursor/rules/*.mdc`

Source: Cursor docs, *Project rules* and *Rule anatomy* (`https://cursor.com/docs/rules`), via Context7
`/websites/cursor`.

- Rules are `.mdc` files under `.cursor/rules/` (subfolders allowed). Each has YAML frontmatter +
  a Markdown body.
- Frontmatter fields: `description` (string), `globs` (path glob[s]), `alwaysApply` (bool).
- **Application logic (verbatim from "Rule anatomy"):** `alwaysApply: true` ⇒ included regardless of
  anything else. If `alwaysApply: false`: `globs` auto-attach the rule to matching files; otherwise a
  `description` lets the Agent select the rule by relevance; if **neither** `globs` nor `description` is
  present, the rule is only pulled in by manual `@`-mention.
- The four modes are therefore: **Always** (`alwaysApply: true`), **Auto Attached** (globs), **Agent
  Requested** (description, no globs), **Manual** (neither).

**Implication.** Cursor has *no keyword-routing primitive* — its scoping is path globs or agent-by-
description. Topology's skill router triggers on **prompt keywords**, not paths, so a keyword-routed
skill maps to **Agent Requested** (`alwaysApply: false`, `description` set, no `globs`) — the skill's
house-format description ("…. Use when <triggers>") is exactly what Cursor selects on. Always-on
**instincts** map to **Always** (`alwaysApply: true`). Cursor does **not** read `AGENTS.md` natively
(ADR-0003), so the contract must be carried into an Always rule too.

## OpenCode — `opencode.json` + `.opencode/skills/`

Source: OpenCode docs *rules*, *skills*, *tools* (`https://opencode.ai/docs/*`), via Context7
`/websites/opencode_ai`.

- Config file `opencode.json` uses `"$schema": "https://opencode.ai/config.json"`.
- `instructions`: an **array of file paths/globs** whose contents are loaded as context, e.g.
  `["docs/standards.md", "packages/*/AGENTS.md"]`. This is the injection point for `AGENTS.md` and for
  always-on instincts.
- **Agent Skills** are native: one folder per skill with a `SKILL.md` inside. OpenCode searches, among
  others, `.opencode/skills/<name>/SKILL.md` (project) and `.claude/skills/<name>/SKILL.md`. Skills use
  the Anthropic Agent Skills frontmatter (`name`, `description`, optional `license`/`compatibility`/
  `metadata`) — **our `skills/*/SKILL.md` are already compatible**, so the adapter copies them verbatim.

**Implication.** OpenCode mapping: `opencode.json` with `instructions: ["AGENTS.md",
".opencode/instincts.md"]` (always-on instincts as an instructions file), plus `.opencode/skills/<name>/
SKILL.md` copied from `skills/`.

## Codex — project-local `.codex/config.toml` + `AGENTS.md`

Sources: the `openai/codex` repo (Context7 `/openai/codex`) **and** probing the installed
`codex-cli 0.137.0` binary.

- **Config layering** (`codex-rs/config/src/loader/mod.rs`): admin → system `/etc/codex/config.toml`
  → user `${CODEX_HOME}/config.toml` → profile `${CODEX_HOME}/<name>.config.toml` → **cwd
  `${PWD}` project config** (loaded, but disabled when the directory is untrusted). A Codex skills
  sample (`codex-rs/skills/.../SKILL.md`) states project-specific **`.codex/config.toml`** carries
  trusted-repo settings (sandbox, MCP, hooks, model/reasoning defaults). So generating
  `.codex/config.toml` is correct and it *is* loaded for a trusted repo.
- **`PROJECT_LOCAL_CONFIG_DENYLIST`** (same loader): project-local config may **not** set
  `openai_base_url`, `chatgpt_base_url`, `apps_mcp_product_sku`, `model_provider`, `model_providers`,
  `notify`, **`profile`**, **`profiles`**, `experimental_realtime_ws_base_url`, `otel` — the repo must
  not control credentials/endpoints/providers or command-exec. **The ROADMAP's "Codex … profiles/agents"
  is therefore invalid in project-local config** — those keys are stripped. The contract must ride on
  `AGENTS.md` instead.
- **`AGENTS.md` is auto-discovered** as the user/project instructions (`config_toml.rs`: "User
  instructions come from AGENTS.md files, not from a config key"). The config-file `instructions` key is
  *system* instructions — a different layer — so we leave the contract to `AGENTS.md`.
- **`[mcp_servers.<name>]`** with `command`/`args`/`[mcp_servers.<name>.env]` is the MCP table shape
  (`config_tests.rs`). We declare no MCP server, so we emit none.

### Empirical key validation (codex-cli 0.137.0, this machine)

`codex doctor` does **not** enforce `--strict-config` (it printed a full report with an unknown key and
failed only on auth). The schema validator that runs **pre-flight, before auth** is:

```
CODEX_HOME=<dir> codex exec --strict-config --skip-git-repo-check "noop"
```

- Negative control — a bogus key is rejected with a precise diagnostic:
  `config.toml:1:1: unknown configuration field \`totally_bogus_key_xyz\``.
- Candidate keys `project_doc_max_bytes`, `model_reasoning_effort`, `sandbox_mode`, `approval_policy`
  all **parse clean** (run advances to the 401 auth stage — no schema rejection).

**Implication.** The generated `.codex/config.toml` sets exactly one project-safe, validated key —
`project_doc_max_bytes` (raised so the full `AGENTS.md` contract is ingested as the contract grows) —
plus a generated-by header. It sets **no** model/sandbox/approval defaults (those belong to the user's
`~/.codex/config.toml`) and **no** denylisted keys. The same `codex exec --strict-config` command
re-validates the *generated* file in the verify note.

## Decisions carried into the spec/ADR

1. Generation lives in `gatekeeper/src/adapt.rs` (pure `root → Vec<GenFile>` builders); `adapters/`
   documents the per-harness mapping. No external templating engine, **no new crate** — `serde_json`
   (existing) serializes JSON; TOML/MDC/Markdown are emitted directly (instinct:
   [[weakest-enforcement-that-works]]).
2. `--check` mode re-renders in memory and diffs against disk → the idempotency gate the ROADMAP's
   verify demands ("regenerating is idempotent").
3. Per-harness mapping: **Codex** = `.codex/config.toml` + auto-discovered `AGENTS.md`; **Cursor** =
   Always rules (contract + instincts) + Agent-Requested rule per skill; **OpenCode** = `opencode.json`
   (`instructions`) + copied `.opencode/skills/` + `.opencode/instincts.md`; **Claude** =
   `.claude/settings.json` hooks (the source-native harness — adapt makes its install config a
   generated artifact too).
