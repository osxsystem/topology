# 0008 — Cross-harness adapter mappings

- **Status:** Accepted
- **Date:** 2026-06-08

[ADR-0003](0003-one-markdown-source-per-harness-adapters.md) decided *that* we generate each harness's
native config from one Markdown source. This ADR records *how* each operator maps onto each harness's
real primitives, and the implementation shape — decided after grounding every format in 2026 docs and
in the installed `codex 0.137.0` binary (see
[research](../research/2026-06-08-cross-harness-adapters.md)).

## Decisions

1. **Generation is pure builders in `gatekeeper/src/adapt.rs`; `adapters/` is the mapping doc.** Each
   harness is a `fn(root) -> Result<Vec<GenFile>, String>`; one `apply_or_check` does all I/O and
   provides `--check` (idempotency). No external template files and **no templating engine** — the
   builders *are* the templates. This keeps a generator that already emits three formats free of a new
   dependency and trivially testable (pure functions). `adapters/README.md` documents the per-harness
   mapping for humans.

2. **No new Cargo dependencies.** `serde_json` (already a dep) serializes `opencode.json` and
   `.claude/settings.json`; TOML, `.mdc`, and Markdown are emitted directly. So this phase does not
   touch `gatekeeper/Cargo.toml` or `Cargo.lock`. (Had a dep been needed, it would be justified here —
   none was.)

3. **Cursor: keyword-routed skills → Agent Requested; instincts → Always.** Cursor has no keyword
   primitive; its scoping is path `globs` or agent-by-`description`. A keyword-routed skill therefore
   becomes an `alwaysApply: false` rule with a `description` and **no `globs`** (Agent Requested), which
   selects on the skill's house-format "Use when …" description. Always-on instincts become
   `alwaysApply: true` (Always). Because Cursor does not read `AGENTS.md`, the contract is carried into
   an Always `agents-contract.mdc`. (The ROADMAP's "per-path scoping from keywords" is imprecise —
   keywords are not paths; path-glob Auto-Attach rules are available the day a skill declares *file*
   triggers, which none do today.)

4. **Codex: contract via `AGENTS.md`, not config.** Project-local `.codex/config.toml` is on a
   `PROJECT_LOCAL_CONFIG_DENYLIST` (`profile`/`profiles`/`model_provider`/`notify`/credentials/
   endpoints) — so the ROADMAP's "Codex profiles/agents" cannot live there. Codex auto-discovers
   `AGENTS.md` as instructions, so the generated config sets only `project_doc_max_bytes` (validated
   against `codex --strict-config`) to guarantee the full contract is ingested, and defers all
   model/sandbox/approval choices to the user's `~/.codex/config.toml`.

5. **Claude is a generated target too.** `adapt --harness claude` emits `.claude/settings.json` (the
   hooks block `install.sh` otherwise hand-prints), so the source-native harness is uniform with the
   rest. `install.sh` keeps *printing* the block (opt-in) rather than auto-writing, so install never
   clobbers a user's existing settings.

## Consequences

- Adding a harness is a new builder + an `adapters/` section; operator content is never re-authored.
- The security floor in *other* harnesses (e.g. `gatekeeper scan` as a Codex/OpenCode lifecycle hook)
  is **deferred future work**, recorded here so it is a decision, not an omission. Today the floor is
  enforced in Claude Code (hooks) and at the git boundary (pre-commit); Codex/Cursor/OpenCode receive
  the *methodology* (contract + skills + instincts) but not yet the deterministic veto.
- `--check` makes drift detectable in CI (Phase 6) without re-deriving outputs by hand.
