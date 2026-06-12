# Building Your Own Composable Agent-Skills Framework

*A research report + concrete design recommendations for a Superpowers-style methodology in your own style — portable across Claude Code, Codex, Cursor, and Gemini.*

Date: 2026-06-03

---

## How to read this

Part 1 is the research: what the field has converged on across four areas — skill design patterns, the workflow/methodology layer, the triggering/instructions problem, and how the major frameworks compare. Part 2 is the build: a concrete taxonomy, file layout, bootstrap instructions, and workflow stages you can fork and make your own.

A note on confidence: the SKILL.md format, progressive-disclosure model, and triggering best practices are well-documented in primary sources (Anthropic docs, the AGENTS.md standard, the Superpowers repo). Popularity figures are not — reported GitHub star counts for Superpowers ranged from ~56k to ~213k across sources within weeks, so treat any single number as unreliable. The *design* lessons below don't depend on those figures.

---

## Part 1 — Research findings

### 1. Skill design patterns

A skill is a directory whose entry point is a `SKILL.md` file: YAML frontmatter (metadata) followed by Markdown instructions, optionally accompanied by scripts and reference files. The design principles that recur across primary sources:

**One skill, one job.** Domain-focused skills (one for PDFs, one for tests, one for planning) stay discoverable and avoid context bloat. Bundling unrelated tasks into one skill hurts both routing and token cost.

**Progressive disclosure — the central idea.** Loading happens in three tiers:

- *Level 1 — metadata, always loaded.* Just the `name` + `description` frontmatter sits in the system prompt (~100 tokens/skill). This is how the agent knows a skill exists.
- *Level 2 — instructions, loaded on trigger.* The `SKILL.md` body is read into context only when the description matches the task. Keep it lean — roughly under 5k tokens.
- *Level 3+ — resources, loaded on demand.* Reference files (`REFERENCE.md`, `FORMS.md`), scripts, schemas, and examples are pulled in only when referenced. Scripts can *execute* via the shell without ever entering the context window. This makes a skill's bundled material effectively unbounded.

**Naming.** Lowercase, hyphenated, ≤64 chars, no reserved words ("claude", "anthropic"). Gerund forms read well: `processing-pdfs`, `writing-plans`, `filling-forms`. Names are for humans; descriptions are for the model.

**Descriptions are the routing signal.** Write in the third person, state *what it does AND when to use it*, and include the keywords a user would actually say. Be slightly "pushy" — agents tend to *under*-trigger skills, so explicit trigger context matters more than brevity. "Extract text and tables from PDFs, fill forms, merge documents. Use when the user mentions PDFs, forms, or document extraction" beats "PDF utility."

**Write constraints as reasoning, not commands.** "Use constructor injection — field injection breaks testability because we can't mock without a Spring context" generalizes better than "NEVER use field injection." State the rule, then *why*, so the agent extends it to cases you didn't enumerate.

**Capture gotchas reactively.** The recommended development loop is evaluation-first: run the agent on real tasks, watch where it fails, and add each failure as a "known gotcha." Don't try to anticipate everything up front.

### 2. The workflow / methodology layer

This is what separates a *methodology* from a *pile of skills*. Superpowers is the clearest worked example; its stages and — crucially — its enforcement mechanisms:

1. **Brainstorming / design.** Socratic clarifying questions, 2–3 alternative approaches with trade-offs, a written design doc the user approves *before any code*. "Too simple to need a design" is treated as an anti-pattern.
2. **Git worktrees.** After design approval, work happens in an isolated branch/worktree with a verified-clean test baseline.
3. **Writing plans.** Decompose into 2–5 minute tasks, each with exact file paths, complete code, test commands with expected output, and a commit message. No "TBD" or "implement later" placeholders allowed.
4. **Subagent-driven development.** Dispatch a fresh subagent per task to limit context pollution, with a two-stage review gate (spec compliance, then code quality).
5. **Test-driven development.** A strict RED → GREEN → REFACTOR loop. The "iron law": code written before its test gets deleted. The skill explicitly enumerates ~15 rationalizations ("already manually tested", "too simple") and rebuts each.
6. **Verification before completion.** Prove the fix with a root-cause method, don't just assert it's done.
7. **Code review + finishing.** Review between tasks; finish by verifying tests, then merge/PR/cleanup.

**The key conceptual distinction: rules vs. gates.** A *rule* ("verify before asserting") has an invisible opt-out — the agent skips it when it feels confident. A *gate* is a hard block with an objective, testable criterion ("a claim about existence is forming → a web search happens → URLs are in hand → only then speak"). Reliable methodologies are built from gates, not rules. This is the single most transferable idea in the whole space.

### 3. Triggering — making the agent actually use the skills

The hardest problem. Agents *under-trigger*: they acknowledge a skill exists, then barrel ahead anyway. Reported failure modes and fixes, from weakest to strongest:

- **Frontmatter description (foundation, but ~20–30% alone).** Trigger condition must be *in the description*, in the user's language, ideally one line. Most "skill ignored" bugs trace to a description that states capability but not *when to use it*.
- **A bootstrap instruction file (`CLAUDE.md` / `AGENTS.md`), ~40%.** Loaded into every session; good for stable project rules. Degrades over long sessions as context drifts.
- **Hooks on prompt submission (~85–95%).** A `UserPromptSubmit` hook injects "evaluate your skills before acting" into every prompt, optionally keyed off a `skill-rules.json` mapping keywords/file-context to skills. This is the reliable mechanism for *mandatory* processes.
- **Subagents (~90% for research-heavy work).** Isolate a task in its own context window and return only distilled results.

Practical rule: combine tiers. Frontmatter for discovery + a bootstrap file for project rules + hooks for the few skills that must never be skipped.

### 4. Framework comparison — what to adopt, adapt, reject

- **Agent Skills / `SKILL.md` (Anthropic open standard).** The portable skill format — folder + `SKILL.md`, progressive disclosure, model-agnostic, broad cross-tool support. **Adopt as your skill format.**
- **`AGENTS.md` (now under the Linux Foundation).** Project-level config read natively by Codex, Cursor, Copilot, Gemini, Aider, Windsurf — but *not* Claude Code, which uses `CLAUDE.md`. **Adopt for project rules; symlink `CLAUDE.md → AGENTS.md`** for single-source-of-truth.
- **`CLAUDE.md`.** Richer (memory layers, activation) but Claude-Code-only. **Don't make it your sole config** — use it as the symlink target.
- **Superpowers.** A full methodology + ~15 prewritten skills, packaged per-platform (`.claude-plugin/`, `.codex-plugin/`, etc.). **Adapt** its workflow stages and gate philosophy; don't necessarily inherit the whole opinionated package if you want your own taste.
- **MCP.** Not a skill format — a protocol for external tools/data with credential isolation. **Adopt alongside** skills (skills teach behavior; MCP provides deterministic tools). Skills can call MCP servers.
- **Spec-driven development.** A methodology (spec → plan → tasks → implement → review) that pairs with any skill system. **Adapt incrementally** for complex features.

**Portability lesson from Superpowers:** keep one Markdown source per skill, then maintain *idiomatic* per-platform packaging rather than lowest-common-denominator auto-translation.

---

## Part 2 — Design recommendations for your own framework

Here's a concrete starting structure you can fork and reshape. The goal: portable skills + a thin, opinionated methodology that's enforced by gates, not vibes.

### File layout

```
your-framework/
├── AGENTS.md                  # project rules + the bootstrap (universal)
├── CLAUDE.md -> AGENTS.md      # symlink so Claude Code reads the same source
├── skills/
│   ├── _getting-started/       # the bootstrap skill (see below)
│   │   └── SKILL.md
│   ├── brainstorm-design/
│   │   ├── SKILL.md
│   │   └── references/design-doc-template.md
│   ├── write-plan/
│   │   └── SKILL.md
│   ├── tdd-loop/
│   │   └── SKILL.md
│   ├── systematic-debug/
│   │   └── SKILL.md
│   ├── verify-before-done/
│   │   └── SKILL.md
│   └── finish-branch/
│       └── SKILL.md
├── hooks/                      # optional, Claude Code: forced skill evaluation
│   ├── skill-activation.sh
│   └── skill-rules.json
├── scripts/install.sh          # one-command install (plugin channel retired in Phase 8)
└── README.md                   # multi-platform install instructions
```

### A skill taxonomy (your own slicing)

Group skills into three layers so the methodology stays legible:

- **Process skills** (the methodology spine): `brainstorm-design`, `write-plan`, `tdd-loop`, `systematic-debug`, `verify-before-done`, `finish-branch`. These are the gated stages.
- **Domain skills** (your stack's expertise): e.g. `react-components`, `postgres-migrations`, `api-contracts`. Reactive, triggered by task keywords.
- **Meta skills** (how the framework maintains itself): `_getting-started` (bootstrap), `capture-gotcha` (append a failure to the relevant skill), `new-skill` (scaffold a skill from the template).

### Bootstrap instructions (the part that makes skills get used)

Put a short, aggressive activation block in `AGENTS.md` and mirror it in a `_getting-started` skill. Something like:

> **Before responding to any coding task:** (1) list the skills available to you; (2) name every skill relevant to this task; (3) load and follow each one; (4) only then act. Process skills are *gates*, not suggestions — you may not write production code before `brainstorm-design` and `write-plan` have produced approved artifacts. If you are about to skip a gate, stop and state which gate and why.

For the process skills that must never be skipped, back this with a `UserPromptSubmit` hook (Claude Code) keyed off `skill-rules.json`, since instructions alone leak.

### Workflow stages, as gates

Express each transition as an objective, checkable condition rather than an aspiration:

1. **Design gate:** an approved design doc exists at `docs/specs/<date>-<feature>.md` → planning may begin.
2. **Plan gate:** a plan with no placeholder steps exists at `docs/plans/<date>-<feature>.md`, and a clean test baseline is confirmed → implementation may begin.
3. **TDD gate:** for each unit of behavior, a test was observed failing *before* the implementation existed → that code may stay.
4. **Verification gate:** the original symptom is reproduced-then-resolved with evidence → the task may be marked done.
5. **Finish gate:** full test suite passes → merge/PR/cleanup options are offered.

### Description-writing convention (so triggering actually works)

Adopt a house format for every skill's `description`:

> `<verb-phrase of what it does>. Use when <concrete user-facing trigger conditions and keywords>.`

Third person, one line, real user vocabulary, slightly pushy. Then iterate: when the agent fails to trigger a skill it should have, widen the trigger language; when it over-triggers, narrow scope.

### How to make it yours

Three levers give you your own "style and taste" without reinventing the substrate:

1. **Pick your gates.** Superpowers is heavy on TDD and design docs. You might gate on, say, type-checking and a written API contract instead. The *gate mechanism* is the borrowed idea; *which* gates are your taste.
2. **Tune the voice.** The instruction tone (terse vs. explanatory), how much you rebut rationalizations, and how strict the bootstrap is are all stylistic choices.
3. **Choose your portability surface.** If you only target Claude Code, skip `AGENTS.md` and hooks complexity. If you target many clients, keep one `SKILL.md` source and add per-platform packaging.

### Suggested first build step

Start with just `_getting-started` + `write-plan` + `tdd-loop`, wire the bootstrap, and run them on one real task. Capture every failure as a gotcha. Grow the library reactively from there — that evaluation-first loop is the most consistent advice across all the primary sources.

---

---

## Part 3 — Agentic design patterns (Google Cloud) applied to Topology

Google Cloud's *Choose a design pattern for your agentic AI system* catalogs single-agent, multi-agent (sequential, parallel, loop, review-and-critique, iterative refinement, coordinator, hierarchical, swarm), and ReAct patterns. Mapped onto Topology, four are worth adopting and three are worth skipping.

**Topology's gate sequence is already a Sequential pattern — lean into it.** Google's sequential workflow agent runs specialized steps "on predefined logic without consulting an AI model for orchestration." That is exactly what the Rust `gatekeeper` does: the model does the work *inside* each stage, while deterministic code decides whether you may advance. Keeping orchestration out of the model is what makes the methodology cheap and hard to rationalize past. Recommendation: keep the gatekeeper as the orchestrator; don't hand routing back to the model.

**Review-and-critique (generator/critic) — the strongest fit and the main gap.** Google's own example is a coding workflow: a generator writes a function, then a *separate* critic agent acts as security auditor / test-runner before approval. Today Topology's `verify-before-done` asks the same agent that wrote the code to verify it — the weakest form. Add a `code-review` skill that dispatches a *fresh subagent* as critic (no memory of writing the code), checking against the design's acceptance criteria. The separate context window is what makes the critique honest.

**Loop / iterative-refinement — already present, needs bounded exits.** Topology's `tdd-loop` (red-green-refactor) and `systematic-debug` (hypothesis cycle) are loops. Google's warning is the actionable bit: every loop needs an explicit termination condition or it runs indefinitely and burns cost. The gatekeeper should own those bounds (max-iteration counters, a "tests green" exit) rather than trusting the agent to stop.

**Coordinator / hierarchical decomposition — adopt only when tasks are large.** This is the natural home for subagent-driven development: `write-plan` decomposes into 2–5 minute tasks; a coordinator dispatches each to a fresh worker subagent and collects results. Worth it only when tasks justify the extra model calls — Google flags the latency/cost tradeoff, and for small features single-agent + gates is cheaper.

**ReAct — reframe, don't build.** ReAct (thought → action → observation) *is* the loop the coding agent already runs. The useful reframing: a `gatekeeper check` call is the "observation" step. Rather than the agent reasoning "I think the plan is done," it takes the action `gatekeeper check plan` and observes a real PASS/FAIL — converting ReAct's soft self-assessment into a hard gate (the rules-vs-gates principle in Google's vocabulary).

**Skip:** parallel, swarm, and deep hierarchies. Google rates swarm "the most complex and costly"; parallel fan-out doesn't match a linear design → plan → build → verify pipeline. They add cost and convergence risk for no methodology benefit.

**Highest-leverage next step:** add a `code-review` critic skill (subagent dispatch) plus a `gatekeeper check review` gate.

---

## Sources

**Agentic design patterns**
- Google Cloud, *Choose a design pattern for your agentic AI system*: https://docs.cloud.google.com/architecture/choose-design-pattern-agentic-ai-system
- ReAct (Yao et al., 2022): https://arxiv.org/abs/2210.03629

**Primary — skill design & authoring**
- Anthropic, *Agent Skills overview*: https://platform.claude.com/docs/en/agents-and-tools/agent-skills/overview
- Anthropic, *Skill authoring best practices*: https://platform.claude.com/docs/en/agents-and-tools/agent-skills/best-practices
- Anthropic engineering, *Equipping agents for the real world with Agent Skills*: https://www.anthropic.com/engineering/equipping-agents-for-the-real-world-with-agent-skills
- Claude Code, *Skills*: https://code.claude.com/docs/en/skills
- Anthropic, *The Complete Guide to Building Skills for Claude* (PDF): https://resources.anthropic.com/hubfs/The-Complete-Guide-to-Building-Skill-for-Claude.pdf

**Primary — methodology / Superpowers**
- obra/superpowers (repo): https://github.com/obra/superpowers
- Superpowers release notes: https://github.com/obra/superpowers/blob/main/RELEASE-NOTES.md
- Jesse Vincent, *Superpowers* (intro): https://blog.fsck.com/2025/10/09/superpowers/
- Jesse Vincent, *Rules and Gates*: https://blog.fsck.com/2026/04/07/rules-and-gates/
- Jesse Vincent, *That time it tried to delete all my tests*: https://blog.fsck.com/2026/04/30/that-time-it-tried-to-delete-all-my-tests/
- Jesse Vincent, *My favorite adversarial review prompt*: https://blog.fsck.com/2026/05/01/adversarial-review/

**Triggering & cross-tool config**
- AGENTS.md standard: https://agents.md/
- agentsmd/agents.md (repo): https://github.com/agentsmd/agents.md
- *How to Activate Claude Skills Automatically*: https://dev.to/oluwawunmiadesewa/claude-code-skills-not-triggering-2-fixes-for-100-activation-3b57
- *Forcing Claude Code to TDD*: https://alexop.dev/posts/custom-tdd-workflow-claude-code-vue/
- *Claude Code customization guide*: https://alexop.dev/posts/claude-code-customization-guide-claudemd-skills-subagents/

**Framework comparison & portability**
- *SKILL.md: The Open Standard for AI Agent Skills*: https://www.agensi.io/learn/agent-skills-open-standard
- *Claude Code Skills vs Cursor Rules vs Codex Skills*: https://www.agensi.io/learn/claude-code-skills-vs-cursor-rules-vs-codex-skills
- *AGENTS.md vs CLAUDE.md vs Cursor Rules vs Copilot (2026)*: https://codersera.com/blog/agents-md-vs-claude-md-vs-cursor-rules-comparison-2026/
- *Spec-Driven Development: The Definitive 2026 Guide*: https://thebcms.com/blog/spec-driven-development
- *MCP vs AI Agent Skills*: https://earezki.com/ai-news/2026-03-13-model-context-protocol-mcp-vs-ai-agent-skills-a-deep-dive-into-structured-tools-and-behavioral-guidance-for-llms/
- *Skills vs MCP Explained*: https://duet.so/guides/agent-skills-101-tools-vs-mcp-vs-skills
