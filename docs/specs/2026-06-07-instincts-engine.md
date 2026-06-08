# Design: Instincts engine (Phase 2)

- **Date:** 2026-06-07
- **Feature slug:** instincts-engine
- **Status:** draft

> Grounded in `docs/research/2026-06-07-instincts-engine.md` (the RESEARCH note). That note mapped the
> terrain and enumerated decisions A–H; this doc **resolves** them. The load-bearing fork — **what an
> instinct *is*** — was decided interactively this session: an instinct is **strictly always-on**, which
> collapses the glob / dependency / `applies` questions (A, B, C) and simplifies the rest. Decisions D,
> E, G adopt the research's leans and are flagged below for veto on review.

## Problem

Topology has gates and scans (hard enforcement) and skills (routed context), but not yet the **weakest**
operator: the **instinct**. An *Instincts Engine* is a context-engineering pattern — a hyper-lean,
always-on layer of **behavioral axioms** and **project constraints** that gives the agent immediate
*common sense about how this repo operates* (METHODOLOGY §5 Pillar 2; ADR-0004). Where a skill teaches
the agent **how to execute** a tool (routed, ~5k tokens), an instinct encodes **how to reason** here
(always-on, a few tokens) — paid on every prompt, so the unit of value is **leverage per token**.

Phase 2 must deliver: an `instincts/` source layer, a `gatekeeper instinct` command surface
(`list` / `render`), injection of the always-on set into the `activate` preamble, and a small,
repo-grounded seed set — so the agent's reasoning is framed *before* it acts. It must also exist *before*
Phase 3, whose `learn promote` writes new instincts into this layer; the promotion target has to exist
first.

**Success looks like:** running any prompt injects the always-on instincts into the agent's context via
the existing `UserPromptSubmit` hook; `gatekeeper instinct list` enumerates them; `gatekeeper instinct
render --harness claude` emits the preamble subset; the loader fails loud on malformed input but never
breaks a turn; and a small efficacy check shows the seeds actually shift the agent's first action.

## Constraints

- **Strictly always-on (the identity decision).** An instinct is always injected; it carries no scope.
  Guidance that needs file / path / language scope is **not an instinct** — it is a skill (keyword-routed)
  or a scan. This dissolves the research §2 "central blocker" (the hook pipes only prompt text,
  `hooks/skill-activation.sh:29` — no file/path context): with nothing to scope, there is nothing to match.
- **Fail-loud, except don't break the turn.** `activate` falls back to empty on unreadable sources
  (`main.rs:146`); a missing `instincts/` dir = no instincts, **not** exit 1. Direct `instinct
  list`/`render` may fail loud (exit 2) on malformed input, mirroring `scan`.
- **Exit codes:** 0 pass/clean, 1 fail/veto, 2 usage/load error (`main.rs:1-17`).
- **Zero new dependencies.** With no globs, the engine needs no glob crate; the current deps (`regex`,
  `serde`, `serde_json`, `toml`) suffice. **No ADR-0007 amendment.**
- **One static offline binary**, `Cargo.lock` committed; `cargo fmt` + `clippy -D warnings` clean before
  finishing (`AGENTS.md:48`).
- **Self-protection friction (expected, not a blocker).** Adding `gatekeeper/src/instinct.rs` and editing
  `gatekeeper/src/main.rs` trips `protected_paths` + the `tamper-security-wiring` regex
  (`gatekeeper/(src/|Cargo\.)`), so the PreToolUse hook will `ask` and pre-commit will gate those edits —
  human-approval by design. (No `Cargo.toml`/`Cargo.lock` edits this phase, since no crate is added.)
  `instincts/` itself is **not** a protected path (instincts are soft framing, not the safety floor).

**Non-goals (explicitly NOT doing in Phase 2):**

- No glob / scope machinery anywhere in the instinct engine — no `globset`, no `applies` field, no
  per-file matching. (Per-path scoping is a skill-layer concern.)
- No file-list plumbing into the `UserPromptSubmit` hook (decision A makes it unnecessary).
- No Phase-4 `adapt` surface (`--harness {codex,cursor,opencode}`); only `render --harness claude`.
- No `learn promote` (Phase 3) — but the loader/validator + `schema` contract is pinned so Phase 3 reuses it.
- No first-class `languages:` mode (deferred until a polyglot stack arrives).
- No standing-up of the de-homed gatekeeper skills (research §4) — deferred to its own change.

## Approaches considered

The load-bearing fork is **what an instinct is**, because it cascades into the glob, dependency, and
`applies` questions (A/B/C).

1. **Strictly always-on (chosen).** An instinct is always injected; anything needing scope becomes a
   skill or scan. *Trade-offs:* keeps the four-operator taxonomy crisp (instinct = *reason*, skill =
   *execute*); dissolves the activate blocker by construction; zero new dependencies; the seed set is
   tiny. Cost: file-area-specific guidance must live as skills, which route on keywords, not paths (a
   deliberate re-sort — research §4).
2. **Glob-scoped instincts via `globset` (the prior design — rejected).** Keep glob-scoped instincts, add
   `globset`, reserve scoped ones for `render`/Cursor. *Trade-offs:* a glob-scoped, conditionally injected
   instinct *is* a cheap routed skill — it blurs the instinct/skill boundary the spectrum exists to draw;
   it needs path context `activate` does not have; and it satisfies the ROADMAP verify criterion only
   **trivially** (no scoped instinct can fire from a prompt alone). Adds a 5th crate + an ADR amendment
   for capability the seed set doesn't use.
3. **Infer paths from prompt keywords / extend the hook.** *Trade-offs:* fuzzy and non-deterministic, or
   heavy plumbing with no reliable "files in play" signal at prompt-submit time. Rejected.

## Decision

### A — Identity & activate contract: **strictly always-on.**

An instinct is always-on by nature, not by configuration. `cmd_activate` emits the **entire** (small)
always-on set; there is no scope to evaluate, so no file context, keyword inference, or hook change is
needed. This retires the weak ROADMAP verify criterion ("a Kotlin instinct doesn't fire on a
Markdown-only prompt"), which presumed scoped instincts; the rewritten criterion exercises the always-on
set instead (see Acceptance).

### B + C — Dependencies & glob dialect: **zero new crates.**

No globs means nothing to glob-match, so `globset` / `glob` / regex-translation do not arise. The existing
deps suffice and **no ADR-0007 amendment is needed**. If a future layer (the skill router or the Phase-4
Cursor adapter) needs per-path matching, `globset` remains the recorded front-runner (braces; transitive
deps already in `Cargo.lock` via `regex`) — recorded *there*, when it lands, not here.

### C-detail — `applies` shape: **drop the field.**

Under always-on, an `applies` field carries one value, so it is removed. Absence ≡ always-on.
Re-introducing a versioned `scope` field later is backward-compatible (the `schema` field below is the
reservation), so dropping it costs no forward-compat.

### D — Priority semantics (research lean; veto on review).

Reuse the `high | medium | low` enum (default `medium`). Priority drives **ordering and truncation order
only**, never enforcement (instincts are always soft). Deterministic tie-break **by `id`** (preserving the
sorted-output property of `cmd_list`/`route`). No hard cap at seed scale; `render` accepts an optional
`--budget <n>` that, when set, truncates lowest-priority-first, deterministically — the lever that
enforces the leverage-per-token discipline once the set grows. **`<n>` is a word budget**
(whitespace-delimited, the same counter as the author-time lint) — a deterministic, offline proxy for
tokens, not a true token count (which needs a model tokenizer or a new dep, both rejected). Truncation
drops **whole instincts**, never partial bodies: a half-rendered *why* loses the rationale that makes it
generalize, so a budget too small for an instinct omits it entirely rather than splitting it.

### E — Loader fail-mode matrix (research lean; veto on review).

| condition | `activate` | `instinct list` / `render` |
|---|---|---|
| missing `instincts/` dir | empty result, exit 0 (don't break turn) | empty list / render, exit 0 |
| empty dir | empty, exit 0 | empty, exit 0 |
| one malformed file (bad frontmatter) | **skip that file + warn to stderr**, exit 0 | **fail loud, exit 2** (name the file) |
| duplicate `id` | skip dupes + warn, exit 0 | fail loud, exit 2 (name the id) |
| unknown frontmatter field | skip file + warn, exit 0 | fail loud, exit 2 (name id + field) |

Adopt `rules.toml`'s loader discipline: name the offending id/file in errors. Phase-3 `promote` validates
against this **same** contract before writing. (The research's "uncompilable glob" row is gone with globs.)

### F + H — Seed set & efficacy: **6 always-on instincts + ship-and-measure.**

Ship the six universal `always` instincts (research §4), split into the two halves of "common sense,"
de-duped against `AGENTS.md`/`CLAUDE.md` to avoid double-injection:

*Behavioral axioms — how to reason here:*

| id | prio | source | why (abbrev.) |
|---|---|---|---|
| `constraints-as-reasoning` | high | `doc:ADR-0004` | A guardrail phrased as reasoning generalizes; a bare "NEVER X" doesn't. |
| `evidence-over-assertion` | high | `doc:ROADMAP` | "Done" = a re-runnable command + its output, never a feeling. |
| `gates-not-rules` | high | `doc:AGENTS.md` | Phrase commitments as trigger→check→act, not a soft rule with an invisible opt-out. |
| `weakest-enforcement-that-works` | medium | `doc:METHODOLOGY` | Default to the lightest operator; earn added strength with evidence. |
| `surgical-changes-only` | medium | `doc:EXTENDING` | Change what the task needs; no drive-by refactors. |

*Project constraint — non-negotiable fact about this repo:*

| id | prio | source | why (abbrev.) |
|---|---|---|---|
| `three-language-lanes` | high | `doc:ARCHITECTURE` | Put each change in its lane — Markdown is truth, Rust enforces, Bash only glues; never bridge a behavior across lanes. |

The file-area-specific candidates from research §4 (`fail-closed-on-adversarial-input`,
`trust-boundary-is-code-not-the-model`, `actionable-error-messages`, …) are **not** instincts; they
re-sort to skills/scans and are deferred to their own change. They were path-scoped, but skills route on
keywords, so each needs trigger keywords designed first (research §4 follow-up).

**Efficacy (decision H) — ship + measure.** The "reasoning generalizes" premise (ADR-0004) is asserted,
not proven; it does **not** gate shipping, but it **does** gate *keeping* each instinct. The verify stage
runs a small eval — a representative prompt with vs. without the injected instincts — scoring *leverage
per token*: does the always-on framing shift the agent's first action (e.g. proposing a gate/evidence
step rather than diving into code)? Each instinct earns its permanent slot or is pruned;
`three-language-lanes` (a constraint, not an axiom) is first to scrutinize.

### G — Render/adapt split (research lean; veto on review).

`instinct render --harness claude` emits the always-on bodies as the prose `activate` already prints. The
Phase-4 `adapt` surface (Cursor/Codex/OpenCode) is deferred, but `render` and `adapt` share **one stable
in-memory `Instinct` shape** and the `matching_instincts(root)` helper. Because instincts are always-on,
the Cursor mapping is the simplest one — Cursor's **Always** mode, not `globs:`. Phase-2 valid `--harness`
value: `claude`; others return a "not yet supported in Phase 2" usage error (exit 2), not a silent empty.

## Architecture (integration map — confirmed against source)

- **New source layer:** `instincts/<id>.md` (Layer 0), frontmatter + 1–2 sentence why-body.
- **New module:** `gatekeeper/src/instinct.rs`; declare `mod instinct;` at `main.rs:25-26` beside
  `mod review;` / `mod scan;`.
- **Dispatch:** add one arm at `main.rs:39-53` (beside `scan`, ≈line 43):
  `Some("instinct") => instinct::cmd_instinct(&args[1..], &framework_root())`. The module owns its inner
  sub-flag match (`list` / `render` / `_ => usage exit 2`), mirroring `scan::cmd_scan`.
- **Injection:** inside `cmd_activate` (`main.rs:129-160`), emit an instincts section in the gap between
  the routed-skills loop (ends `:157`) and the trailing gate-warning line (`:158`), reusing the
  `framework_root()` resolved at `:137`. The section is **demarcated from routed skills** by its own
  header and the *absence* of an `[enforcement]` tag — signalling always-on framing, not a routed
  procedure. Each line is `- [id] <why>`, priority-ordered then by id; the `[id]` makes an instinct
  referenceable when the agent reasons past it (the hook the Phase-3 override signal will read):

  ```
  Topology: evaluate your skills before acting.
  Routed skills for this prompt:
    - request-refactor-plan [suggest]
  Always-on instincts — how to reason here (framing you may reason past only with cause):
    - [constraints-as-reasoning] A guardrail phrased as reasoning generalizes; a bare "NEVER X" doesn't.
    - [evidence-over-assertion] "Done" means a re-runnable command and its output, never a feeling.
    - [gates-not-rules] Phrase commitments as trigger → check → act, not a soft rule with an opt-out.
    - [three-language-lanes] Put each change in its lane — Markdown is truth, Rust enforces, Bash glues.
  You may not write production code before the design and plan gates pass.
  ```
- **Shared helper:** `matching_instincts(root) -> Vec<Instinct>` (load + parse + sort) used by both
  `activate` and `instinct list`/`render`, so the load logic lives once. **No context filtering** — every
  instinct is always-on.
- **Frontmatter fields:** `id` (kebab-case ≤64, no reserved words, unique), `priority`
  (`high|medium|low`, default `medium`), `schema` (int, default `1` — reserved for Phase-3 attach),
  optional `source` provenance (accept-but-ignore string; format below), body (the why). **No `applies`
  field.** Reject unknown fields.
- **`source` format (reserved for Phase 3):** a `<scheme>:<value>` string — type-validated but otherwise
  ignored in Phase 2, so Phase 3 needn't retrofit. Two schemes: `doc:<tag|path[#anchor]>` for
  hand-authored seeds (e.g. `doc:ADR-0004`, `doc:docs/METHODOLOGY.md#pillar-2`); `ledger:<entry-id>` for
  Phase-3-promoted instincts — a back-link into the `docs/learn/` gotcha ledger, written by `learn
  promote` and read back for audit. Split on the first `:` to parse. One origin per field in v1 (a list
  can come later, versioned via `schema`). `promote` requires a known scheme; hand authors may omit `source`.
- **No dependency changes:** `Cargo.toml`/`Cargo.lock` untouched.
- **Tests:** `gatekeeper/tests/cli_instinct.rs`, mirroring `cli_review.rs`/`cli_scan.rs` (scratch root
  with a `skills/` marker + an `instincts/` dir; exec `env!("CARGO_BIN_EXE_gatekeeper")`).
- **Docs:** update `docs/ROADMAP.md` (Phase 2 → delivered; **rewrite the verify criterion** off scoped
  instincts); update `docs/ARCHITECTURE.md §5` (the canonical instinct shape — no `applies`). Fix the two
  stale references from research §7 (`ARCHITECTURE.md:195` retired `json.rs`; `ROADMAP.md:108`
  skills→instincts, reframed to the skill router). **No ADR amendment** (no new crate).

## Risks & open questions

- **Efficacy is genuinely unproven.** The small eval (H) mitigates but does not settle whether always-on
  framing changes behavior across models/harnesses. Accept as a known limitation; the eval gates
  *keeping*, not shipping.
- **Double-injection.** The six seeds overlap conceptually with `AGENTS.md`. De-dup is a judgment call;
  verify should confirm the preamble doesn't merely restate `AGENTS.md` lines.
- **Self-protection friction** on the `main.rs` + new `instinct.rs` edits is expected; the human will be
  asked to approve. Land them as discrete, reviewable commits.
- **Priority's behavioral effect is assumed.** That injection order / `--budget` truncation affects model
  salience is plausible but unmeasured — folds into the same eval.
- **De-homed skills' triggers undesigned.** Converting the path-scoped §4 candidates into keyword-routed
  skills is real design work, deferred — flagged so it isn't mistaken for mechanical.
- **D / E / G are maintainer leans, not yet ratified** — flagged for veto before the plan stage locks them.

## Acceptance criteria

- [ ] `instincts/` layer exists with the **6** always-on seed files; each has valid frontmatter (no
      `applies` field) and a `source` using the `doc:` scheme (e.g. `doc:ADR-0004`).
- [ ] `gatekeeper instinct list` enumerates the seeds, sorted, with id + priority; exit 0.
- [ ] `gatekeeper instinct render --harness claude` emits the always-on bodies as preamble prose;
      `--harness <other>` exits 2 with a "not supported in Phase 2" message.
- [ ] `gatekeeper instinct render --budget <n>` truncates lowest-priority-first, deterministically
      (tie-break by id), counting **words** and dropping whole instincts (never splitting a body).
- [ ] `gatekeeper activate` injects **all** always-on instincts into the preamble between the
      routed-skills block and the gate warning, under a distinct "Always-on instincts —" header with
      `- [id] why` lines and **no** `[enforcement]` tag; a missing `instincts/` dir yields no instincts
      and exit 0 (turn not broken). *(Rewritten ROADMAP verify criterion — replaces the scoped-instinct test.)*
- [ ] Loader fail-mode matrix holds: malformed file / duplicate id / unknown field → skip+warn in
      `activate` (exit 0), fail-loud (exit 2, names the offender) in `list`/`render`.
- [ ] **No new dependency:** `Cargo.toml`/`Cargo.lock` unchanged; binary still builds offline.
- [ ] `cargo test` green (new `cli_instinct.rs` + colocated unit tests), `cargo fmt --check` clean,
      `cargo clippy --all-targets -- -D warnings` clean.
- [ ] Small efficacy eval recorded in the verify note: a prompt's first proposed action shifts toward
      gate/evidence framing with the instincts injected vs. without (scored per instinct).
- [ ] Stale doc references fixed (`ARCHITECTURE.md:195`, `ROADMAP.md:108`); `ARCHITECTURE.md §5` canonical
      shape and `ROADMAP.md` Phase 2 status + verify criterion updated.
