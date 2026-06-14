# Research — path-triggered routing + router eval harness (Phase 15 workstream B)

- **Date:** 2026-06-14 · **Feature slug:** path-routing
- **Source of truth:** ROADMAP Phase 15 deliverables (`docs/ROADMAP.md:537-545`).
- **Method:** Explore subagent fan-out (grep/Read; context-engine MCP was disconnected this session — documented fallback). Every claim carries a `file:line`; inferences marked ASSUMPTION.

## The goal

Route a **required skill from the file PATHS an edit touches**, not from the prompt's keywords — "security routing keys on what the diff *touches*, not how the prompt is phrased" (`docs/ROADMAP.md:537-541`). Plus a **router eval harness**: ≥50 labeled prompts, CI thresholds recall ≥0.90 (`require` skills) / precision ≥0.80 (`docs/ROADMAP.md:542-545`).

## Findings (cited)

### 1. Keyword routing today
- `gatekeeper activate` (`main.rs:592-629`): reads a prompt on stdin, strips a JSON envelope (`extract_prompt_owned`, `main.rs:567-590`), lowercases, calls `route(&rules, &prompt_lc)`.
- `route()` (`main.rs:657-685`): iterates `skills` in `hooks/skill-rules.json`, reads **only** `promptTriggers.keywords`, word-boundary regex match (`keyword_regex`, `main.rs:640-654`). Returns sorted `Vec<(skill, enforcement)>`.
- Output: the `Routed skills for this prompt: - <skill> [require|suggest]` block (`main.rs:617-627`).

### 2. skill-rules.json
- Lives at `hooks/skill-rules.json` (`version`, `skills: { <name>: { type, enforcement, priority, promptTriggers: { keywords:[...] } } }`).
- **No `pathTriggers` field exists.** Natural home: a sibling `pathTriggers: { globs: [...] }` per skill.

### 3. Hook wiring
- `.claude/settings.json`: `PreToolUse` → `hooks/security-scan.sh` (matcher `Bash|Write|Edit|MultiEdit`); `UserPromptSubmit` → `hooks/skill-activation.sh`. **No `PostToolUse` hook today.**
- `skill-activation.sh:43-46` pipes stdin to `gatekeeper activate`, fails open ("router unavailable") if the binary is missing.

### 4. CLI dispatch table (ADR-0014)
- `SUBCOMMANDS` static (`main.rs:63-197`); `SubcommandSpec { name, usage, synopsis, known_flags, handler }`. `activate` at `main.rs:88-94`.
- A `route --paths`/`--staged-paths` surface = one new `SubcommandSpec` row + a `cmd_route` handler. `cli_doc_sync.rs` auto-verifies docs stay in sync.

### 5. Eval/test precedent
- Only `cli_help_flags.rs:65-103` (help/flag tests for `activate`). **No functional routing tests, no labeled-prompt corpus.** Test harness pattern: `scratch_root()` + piped-stdin `run()`.

### 6. Protected paths (`security/rules.toml:203-226`)
- Protected: `gatekeeper/src/main.rs` ✅, `hooks/security-scan.sh` ✅, `hooks/pre-commit.sh` ✅, `.claude/settings.json` ✅, `scan.rs`, Cargo.*.
- **`hooks/skill-rules.json` is NOT protected** (data config) — editable without override.
- Implication: adding the CLI surface (main.rs) + wiring a PostToolUse hook (settings.json) **requires `--no-verify` with a documented override** (autonomy grant). A *new* `hooks/*.sh` file is not itself in the protected list, but its settings.json wiring is.

### 7. Glob matching
- `glob_match(path, glob)` (`scan.rs:498-527`): dep-free, trailing-`/` = prefix, `*` = wildcard segment. **Reusable** — but it is a private fn in the protected `scan.rs`. Reuse options: (a) make it `pub(crate)` (edits a protected file), or (b) duplicate a small matcher in the routing module (DRY cost). Design decision.

## Open decisions for the design gate

- **D1 — glob_match reuse vs duplicate.** `pub(crate)` on the scan.rs fn (one protected-file edit, DRY) vs a tiny private copy in the router (no protected edit, slight duplication).
- **D2 — PostToolUse hook: advisory-only.** Routing injects *context* ("you touched `hooks/**` — the security-scanning skill is required"); it must NOT block a tool call (weakest-enforcement; routing is a reminder, not a veto). Confirm advisory.
- **D3 — eval harness scope.** ≥50 labeled prompts is substantial; the corpus must cover both prompt-routing (existing) and the new path-routing. Thresholds recall ≥0.90 / precision ≥0.80 per ROADMAP.
- **D4 — new hook vs extend skill-activation.** A new `hooks/post-tool-routing.sh` + PostToolUse wiring, vs folding into existing scripts.

## Scope / stakes note

Unlike workstream A (pure-additive measurement, zero protected edits, flips nothing), **B edits the protected enforcement surface** (`main.rs`, `.claude/settings.json`, a new always-on PostToolUse hook) and changes runtime behavior for every future session. That raises the bar for the design gate's human approval.
