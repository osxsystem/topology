---
name: research-first
description: Gather cited evidence about a problem space before writing any design or code; research artifact lands under the artifacts root (`docs/` here, `.claude/topology/` in a governed project). Use when starting a new feature, when the problem space is unfamiliar or ambiguous, or when the design gate requires a research note to proceed.
---

# Research First (the research gate)

No design doc until a research note exists at `<artifacts-root>/research/<date>-<slug>.md` and the research gate passes (artifacts root: `docs/` in the framework repo, `.claude/topology/` in governed projects — gate FAIL messages print the resolved path). The design gate is sequence-locked behind it.

## Process

1. **Decompose.** Break the question into distinct sub-questions: prior art, constraints, risk areas, open unknowns. Write them down before searching.
2. **Gather.** Delegate heavy exploration to a subagent. Give it the sub-questions and a scope boundary. The subagent reads code, searches docs, and gathers facts — the main loop does not do this work itself.
3. **Cite.** The subagent returns a structured summary. Every factual claim names its source (file path, line range, URL, or doc section). Unsupported claims are marked as assumptions.
4. **Verify.** Cross-check the top-risk claims before accepting the summary. If a claim contradicts the codebase, resolve the conflict before writing the note.
5. **Write the note.** Consolidate the verified summary into `<artifacts-root>/research/<YYYY-MM-DD>-<feature-slug>.md` and commit it.

## Gate check

```bash
gatekeeper check research --feature <feature-slug>
```

Passes when a file matching `<artifacts-root>/research/*<slug>*.md` exists. When it passes, the design gate unlocks — `gatekeeper check design` will proceed to the spec check instead of failing with "research-first."

## Sequence lock

`gatekeeper check design` calls `find_doc("research", slug)` before it checks for the spec. If the research note is absent, design returns:

```
FAIL design gate: research-first — no <artifacts-root>/research/*<slug>*.md
```

Fix by running this skill and committing the note, then re-run the design gate.

## Reach

- **Claude Code** reads `skills/` natively at session start — this skill is always active.
- **Cursor and OpenCode** receive the skill via `gatekeeper adapt` (see `adapt.rs:221-256`): it is written to `.cursor/rules/skill-research-first.mdc` and `.opencode/skills/research-first/SKILL.md` respectively.
- **Codex** reaches this skill only through `AGENTS.md`, which instructs it to read `skills/`. It is not delivered by `adapt`.

## Why delegate exploration to a subagent

Heavy codebase exploration is context-expensive and error-prone when interleaved with planning. A dedicated subagent can fan out across files without polluting the main loop's context window. The main loop receives a bounded, cited summary — not raw search noise — and only then makes design decisions.

## Rationalizations (rebutted)

| Excuse | Reality |
|--------|---------|
| "I already know this domain." | Then the note takes ten minutes. Write it — the gate is cheap and the lock is real. |
| "We can research while designing." | Interleaved research and design means decisions are made before all evidence is in. |
| "The subagent might miss something." | That is what step 4 (Verify) is for. Delegate, then spot-check the top risks. |
