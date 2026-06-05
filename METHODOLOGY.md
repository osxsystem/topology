# Topology — The Operator Methodology

Topology is a **harness-native operator system** for building software with AI coding agents and humans
*together*. It turns a development methodology into something an agent cannot quietly skip: a small
set of **operators** — instincts, skills, gates, and scans — defined once in Markdown, enforced by
one Rust binary, and delivered natively to every harness the team uses (Claude Code, Codex, Cursor,
OpenCode).

This document is the canonical definition of the methodology.

- **How it's built:** [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md)
- **The path to build it:** [`docs/ROADMAP.md`](docs/ROADMAP.md)
- **How to add your own operators:** [`docs/EXTENDING.md`](docs/EXTENDING.md)
- **The decisions behind it:** [`docs/adr/`](docs/adr/)
- **The underlying research:** [`RESEARCH.md`](RESEARCH.md)

> **Status legend.** The **gate** operators and the **Claude Code** delivery exist today. **Instincts**,
> **security scanning**, **continuous learning**, and the **Codex / Cursor / OpenCode** adapters are
> *planned* — see the [roadmap](docs/ROADMAP.md). This document describes the whole system and marks
> what is `[built]` vs. `[planned]` throughout, so it never claims more than ships.

---

## 1. Why Topology exists

Coding agents are capable but unreliable in predictable ways:

- **They under-trigger their own knowledge.** A skill that isn't loaded may as well not exist.
- **They rationalize past soft rules.** "I'll verify after." "This is too simple to plan."
- **They drift across harnesses.** One team uses Claude Code, Codex, Cursor, and OpenCode; each reads
  different config, so discipline that lives in one tool evaporates in the others.
- **They forget.** A lesson learned in one session doesn't survive into the next.
- **They have no safety floor.** Nothing deterministically stops a leaked secret or a `curl … | sh`
  before it runs.

Topology's answer is to stop relying on the agent's goodwill and make the methodology **executable**.
Anthropic's own guidance draws the line precisely: CLAUDE.md instructions are *advisory*, while hooks
are *deterministic and guarantee the action happens*
([Best practices for Claude Code](https://code.claude.com/docs/en/best-practices)). Topology is built on
the deterministic side of that line.

---

## 2. First principles

### 2.1 Gates over rules
A *rule* ("verify before asserting") has an invisible opt-out — the agent skips it when it feels
confident. A *gate* is a hard block with an objective, testable criterion. Every methodology stage
that matters becomes a gate the agent — or CI — can run, not a sentence it can skip.

### 2.2 The enforcement spectrum
Blocking everything is as useless as advising everything. Topology places each unit of behavior — an
**operator** — at the *weakest enforcement that still works*:

| Operator | Enforcement | Cost | Who can skip | Example |
|---|---|---|---|---|
| **Instinct** | Soft, always-on reasoning nudge | a few tokens, every prompt | the agent, by reasoning | "Prefer constructor injection — field injection breaks testability." |
| **Skill** | Loaded on trigger, then followed | ~5k tokens when relevant | the agent, if mis-routed | `tdd-loop`, `systematic-debug` |
| **Gate** | Hard block until a check passes | one CLI call | no one | `design`, `plan`, `verify`, `finish` |
| **Scan** | Deterministic veto on a tool call | one CLI call per tool use | no one | block a committed secret or a piped-shell install |

Choosing the right strength is the craft. Anthropic's *Building Effective Agents* makes the same point
from the other direction: *find the simplest solution possible, and only increase complexity when
needed* — promote an operator to a heavier strength **only when it demonstrably improves outcomes**.

### 2.3 One source, one engine, thin glue
- **Markdown is the source of truth.** Operators are human- and agent-editable text.
- **Rust is the engine.** `gatekeeper` is one small, fast, dependency-free binary, safe to run on
  every prompt and in CI. (Anthropic, *Writing effective tools for agents*: consolidate functionality
  into a few high-signal tools with clear contracts and actionable errors.)
- **Bash is the glue.** Hooks and installers shell out; they're the lowest common denominator every
  harness can call.

### 2.4 Research-first
The first question is never "how do I build this" but "what already exists, and what is actually being
asked." Research precedes design; design precedes a plan; no production code is written before the
approach is understood. This mirrors Anthropic's *explore → plan → code → commit* loop.

### 2.5 Learning closes the loop
A failure that isn't captured will recur. When a gate fails or a human corrects the agent, the lesson
is written down and **promoted** back into the source — as a new instinct, skill, or scan rule — so
the system gets stricter exactly where it got burned.

### 2.6 Harness-native, not lowest-common-denominator
Each harness keeps its own idioms — Claude Code's hooks, Codex's orchestrator agents, Cursor's rule
files, OpenCode's config — all generated from the single Markdown source. Portability without
flattening.

---

## 3. The operating contract

Every Topology agent, on every coding task, in any harness:

1. **Load instincts.** Read the always-on reasoning nudges; they frame everything below. `[planned]`
2. **Route skills.** List the available skills, name every one relevant to this task (one line each),
   then load and follow each. `[built]`
3. **Walk the gates in order.** Don't skip ahead. If you're about to skip a gate, stop and state which
   gate, and why. `[built]`
4. **Submit to the scanner.** Tool calls that touch the shell or filesystem pass the security scan
   first; a veto is final. `[planned]`
5. **Show evidence, not assertions.** "Done" is a command someone can re-run and output they can see —
   never "I'm confident." (Anthropic: *have Claude show evidence rather than asserting success*.)
6. **Capture what bit you.** Surprises and corrections become operators for next time. `[planned]`

> "This is too simple to need a gate" is an anti-pattern. Trivial tasks still pass through the gates —
> they just pass quickly.

---

## 4. The gate sequence

```
research ─► brainstorm-design ─► write-plan ─► tdd-loop ─► verify-before-done ─► code-review ─► finish-branch
(research)    (design gate)      (plan gate)   (tdd gate)     (verify gate)      (review gate)   (finish gate)
[planned]                                          ▲
                                         systematic-debug (invoked on failure)
```

| Gate | Passes when | Check |
|---|---|---|
| **research** `[planned]` | a research note exists at `docs/research/<date>-<feature>.md` | `gatekeeper check research --feature <slug>` |
| **design** `[built]` | an approved design doc exists at `docs/specs/<date>-<feature>.md` | `gatekeeper check design --feature <slug>` |
| **plan** `[built]` | a placeholder-free plan exists at `docs/plans/<date>-<feature>.md` | `gatekeeper check plan --feature <slug>` |
| **tdd** `[built]` | every behavior had a test you watched fail *before* the code existed | discipline; enforced by review |
| **verify** `[built]` | the original symptom is reproduced-then-resolved with evidence | `gatekeeper check verify --feature <slug>` |
| **review** `[built]` | a fresh-context critic's artifact passes — bound to the current clean `HEAD` and merge-base, both rubric dimensions present, no blocking findings | `gatekeeper check review --feature <slug> [--base <ref>]` |
| **finish** `[built]` | the full test suite passes | `gatekeeper check finish -- <cmd>` |

You may not write production code until **design** and **plan** pass (and, once shipped, **research**).

---

## 5. The six pillars

Each pillar names a capability, how it works as an operator, its status, and the Anthropic principle
it embodies.

### Pillar 1 — Skills `[built, extending]`
Progressive-disclosure units of methodology and expertise. A skill is a directory with a `SKILL.md`
whose YAML frontmatter (`name`, `description`) is always loaded; the body loads on trigger; bundled
`references/` load on demand. Three kinds: **process** (the gated spine), **domain** (stack-specific
expertise), **meta** (the framework maintaining itself). Today: 8 process skills (including the `code-review` critic). Planned: domain
skills, plus meta skills `capture-gotcha` and `new-skill`.
→ *Anthropic, [Agent Skills](https://www.anthropic.com/engineering/equipping-agents-for-the-real-world-with-agent-skills): progressive disclosure as the core scaling principle.*

### Pillar 2 — Instincts `[planned]`
Tiny, always-on, **reasoning-based** guardrails — "constraints as reasoning, not commands." Phrased
as *why*, not *don't*: "Use constructor injection — field injection breaks testability" generalizes
where "NEVER use field injection" does not. Cheaper than a skill (a handful of tokens), softer than a
gate (the agent may reason past one with cause). `gatekeeper activate` injects the relevant instincts
into every session, rendered per harness.
→ *Anthropic, Best practices: "develop your intuition" — the durable judgments that no checklist captures.*

### Pillar 3 — Memory optimization `[partial → hardening]`
Three levers that keep the context window — *the* constraining resource — lean: **progressive
disclosure** (load operators only when relevant), the **RTK** token-killer proxy (60–90% savings on
shell I/O), and a **`memory/` protocol** for handoff and compaction artifacts so a fresh session
resumes without re-reading everything.
→ *Anthropic, [Effective context engineering for AI agents](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents): treat context as a finite budget; load just-in-time.*

### Pillar 4 — Continuous learning `[planned]`
The loop that turns failures into permanent guardrails. `gatekeeper learn` records each gate failure
or human correction to `docs/learn/`, then **promotes** the recurring ones into a new instinct, skill,
or scan rule. The system tightens precisely where it has been burned, with the human approving each
promotion.
→ *Anthropic, Best practices: capture "common gotchas and non-obvious behaviors" instead of relearning them.*

### Pillar 5 — Security scanning `[planned, front-loaded]`
A deterministic safety floor — the biggest current gap, so it ships first. `gatekeeper scan` reads a
diff or a proposed command and **vetoes** on detected secrets, dangerous shell (`rm -rf /`, piped-shell
installs, history rewrites), and known vulnerable patterns, driven by `security/rules.toml`. Wired as
a `PreToolUse` hook (block before execution) and a pre-commit hook (block before history). A veto is
final; it is not advice.
→ *Anthropic, Best practices: the bundled security-reviewer subagent and "hooks are deterministic" — security belongs on the deterministic side.*

### Pillar 6 — Research-first development `[partial → gate]`
Exploration is a first-class, gated stage, not a preamble. A `research-first` skill drives the
explore phase (what exists, what's actually asked, which approaches and trade-offs), producing a
`docs/research/<date>-<feature>.md` artifact that the new `research` gate checks **before** design.
→ *Anthropic, Best practices: "Explore first, then plan, then code" — separate research from execution to avoid solving the wrong problem.*

---

## 6. Anthropic alignment

Topology is a concrete implementation of Anthropic's published guidance on building agents and software.

| Anthropic principle | Source | How Topology embodies it |
|---|---|---|
| Workflows (deterministic) vs. agents (autonomous) | [Building Effective Agents](https://www.anthropic.com/engineering/building-effective-agents) | The gate sequence is a deterministic *workflow*; research and code-review subagents are the autonomous *agents*. |
| Simplest solution; add complexity only when it pays | Building Effective Agents | The enforcement spectrum — promote an operator's strength only when outcomes demand it. |
| Hooks deterministic; instructions advisory | [Best practices for Claude Code](https://code.claude.com/docs/en/best-practices) | Gates and scans run as hooks, not prose. |
| Explore → plan → code → commit | Best practices | The research → design → plan → tdd → finish sequence. |
| Show evidence, not assertions; independent verifier | Best practices | The `verify` gate + a fresh-context `code-review` critic subagent. |
| Progressive disclosure | [Agent Skills](https://www.anthropic.com/engineering/equipping-agents-for-the-real-world-with-agent-skills) | Instinct → skill metadata → skill body → references. |
| Context is a finite budget | [Effective context engineering](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents) | The memory-optimization pillar. |
| Few high-signal tools, clear contracts | [Writing effective tools for agents](https://www.anthropic.com/engineering/writing-tools-for-agents) | One `gatekeeper` binary with explicit subcommands and exit codes. |

---

## 7. Humans and agents, together

Topology is a working agreement, not an autopilot. It maps cleanly onto Agile roles so a human and an
agent can share one definition of done:

| Agile concept | Topology operator | Owner |
|---|---|---|
| Working agreements / team norms | **Instincts** | Human sets; agent follows |
| Playbooks | **Skills** | Either authors; agent runs |
| Definition of Done | **Gates** | Human approves design & merge; agent produces the evidence |
| Safety / compliance policy | **Scans** | Human sets policy; engine enforces |
| Retrospective | **Continuous learning** | Agent proposes; human approves promotion |

The division of labor: the **human** owns intent, the design approval, the merge decision, and the
policy. The **agent** owns the loop — research, plan, test-first implementation, and the evidence at
each gate. The **engine** owns the parts neither should be trusted to do by feel: checking gates and
vetoing unsafe actions. Gates make handoffs auditable; either party can pick up a feature by reading
its `docs/{research,specs,plans,verify}` trail.

---

## 8. Glossary

- **Operator** — any installed, enforced unit of agent behavior: an instinct, skill, gate, or scan.
- **Gate** — a hard block with an objective check the agent or CI can run.
- **Instinct** — a soft, always-on, reasoning-based nudge.
- **Skill** — a progressively-disclosed `SKILL.md` unit (process / domain / meta).
- **Scan** — a deterministic veto on a tool call (security).
- **Harness** — an agent runtime: Claude Code, Codex, Cursor, OpenCode.
- **Gotcha** — a captured failure or correction, eligible for promotion into an operator.
- **gatekeeper** — the Rust binary that routes skills and enforces gates (and, planned, scans/learns).
- **Source of truth** — the Markdown (`AGENTS.md`, `skills/`, `instincts/`, `security/rules.toml`) all
  harnesses are generated from.
