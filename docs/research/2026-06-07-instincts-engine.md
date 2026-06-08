# Research: Instincts engine (Phase 2)

- **Date:** 2026-06-07
- **Feature slug:** instincts-engine
- **Stage:** RESEARCH (precedes design → spec → plan → implement). This note maps the terrain,
  surveys prior art, and enumerates the decisions the design stage must make. *Update: the maintainer has
  since locked four of those forks (the always-on identity and its cascade — §1, §6 A/B/C/H); they are
  recorded inline so design starts from a coherent base. The remaining forks (§6 D–G) stay open.*
- **Method:** 5 parallel read-only research lanes (codebase integration, methodology semantics, seed
  candidates, external prior art, format/rendering) + a completeness critic. ~482k tokens, 6 agents.
- **Governance:** maintainers (human + assistant) own every decision below; the agents and external
  sources are advisory. Load-bearing repo-internal claims were independently re-verified before
  recording (see the accuracy note).

---

## 0. Maintainer accuracy note — read first

The critic flagged several "facts" as inferences. Three were load-bearing and repo-internal, so they
were verified against source before this note canonized anything:

| Claim (as researched) | Verdict | Truth |
|---|---|---|
| "4-dependency cap (ADR-0007) — do not add a 5th crate" | ⚠️ **Inference, not a rule** | ADR-0007 only *"adopts four vetted crates"* / *"gains four well-known, offline-buildable crates."* It nowhere caps the count or forbids a fifth. (Since resolved — §6.B: the always-on identity removes globs from instincts, so Phase 2 adds **zero** new crates; the fifth-crate question is moot here and defers to the skill/adapter layer.) |
| `docs/ARCHITECTURE.md:195` cites a retired parser | ✅ **Confirmed stale** | Line 195 still calls `gatekeeper/src/json.rs` "the dependency-free JSON parser." ADR-0007 retired `json.rs` (migrated to `serde_json`). Doc bug to fix. |
| `docs/ROADMAP.md:108` "Cursor globs from each skill's `applies`" | ✅ **Confirmed wrong** | Skills route on *keywords* (`skill-rules.json` `promptTriggers.keywords`) and have **no** `applies` field (zero hits in `skills/`). Only **instincts** carried `applies` — and the always-on decision (§1) drops that field entirely (instincts are always-on by definition), so Cursor's per-path globs now derive from the **skill** layer, not instincts (§3, §7). |

External numeric claims (Cursor "<200 words", Claude Code "~200 lines", Codex/LF donation, "Agent
RuleZ" priority bands) are single-blog-sourced and treated as **illustrative, not load-bearing** —
confirm before baking any number into a lint.

Verified-correct and load-bearing (confirmed against source this pass): the `cmd_activate` injection
point and integration shape; that routing today is keyword-substring against prompt text only with no
file/path context; the fail-closed-vs-don't-break-the-turn split; and that `globset`'s transitive
deps (`aho-corasick`, `regex-automata`) are already in `Cargo.lock` via `regex`.

---

## 1. What an instinct is

An **Instincts Engine is a context-engineering pattern**: a hyper-lean, foundational layer of
**behavioral axioms** and **project constraints** that gives an agent immediate *common sense about how
this specific repository operates* — loaded before it reasons, paid in a handful of tokens, every
session.

The leverage is in *what* an instinct encodes, not how much. A **skill** teaches the agent **how to
execute** a tool or procedure (technical, routed on demand, ~5k tokens). An **instinct** encodes **how
to reason** inside this codebase (axiomatic, always-on, text-compressed). You are not paying for a
capability you summon occasionally — you are paying a few tokens to shape *every* decision the agent
makes. That asymmetry — a fraction of the tokens for leverage over all output — is the whole point.

So an instinct is the weakest of Topology's four operator types (METHODOLOGY §5 Pillar 2; ADR-0004): a
tiny guardrail phrased as the **why** (the rationale that generalizes) — never a bare "don't" (which
doesn't). It frames reasoning *before* the agent acts; the agent **may reason past it with cause**.
Because it shapes reasoning, it must be present before reasoning begins — an instinct is **always-on by
nature, not by configuration**.

**The two halves of "common sense":**
- **Behavioral axioms** — *how to reason here*: `constraints-as-reasoning`, `evidence-over-assertion`,
  `weakest-enforcement-that-works`.
- **Project constraints** — *non-negotiable facts about this repo*: `three-language-lanes` (Markdown is
  truth, Rust enforces, Bash glues), offline / one static binary.

**Enforcement spectrum** (weakest → strongest), the decision model from ADR-0004:

| Operator | Encodes | Cost | Who can skip | Lives in |
|---|---|---|---|---|
| **Instinct** | **how to reason** here, always-on | a few tokens, **every prompt** | the agent, by reasoning | Markdown (`instincts/<id>.md`) |
| Skill | **how to execute** a tool/procedure | ~5k tokens, **on trigger** | not loaded ⇒ absent | Markdown (`skills/<name>/SKILL.md`) |
| Gate | stage checkpoint | one CLI call | no one | Rust (`gatekeeper check …`) |
| Scan | deterministic veto | one CLI call | no one (modulo `--no-verify` threat) | Rust + `rules.toml` |

The craft is placing each behavior at the **weakest enforcement that still works**, and promoting only
when evidence demands. Instincts are also a **Phase-3 promotion target** (`learn promote` →
instinct | skill | scan-rule) — which is *why Phase 2 must precede Phase 3*: the target must exist first.

**Hyper-lean by construction.** Because every instinct is paid on every prompt, the unit of value is
**leverage per token**. Author each as highly optimized, text-compressed Markdown — the least prose
that still carries the generalizing *why*. This makes the token budget (§6.F) a first-class design
constraint rather than an afterthought, and sharpens the efficacy question (§6.H) into *leverage per
token*, not merely "does it change behavior."

Canonical shape (the `docs/ARCHITECTURE.md §5` sketch, corrected to a real, always-on, repo-grounded
instinct — the original `constructor-injection` example is generic boilerplate this codebase does not
follow, §4):

```markdown
---
id: evidence-over-assertion
priority: high
---
"Done" means a re-runnable command and its output, never a feeling. A claim you can't replay is a
guess wearing a verdict's clothes.
```

---

## 2. Current codebase reality (integration map)

All file:line references confirmed against source.

- **Dispatch** — single `match args.first()` in `main()` at `gatekeeper/src/main.rs:39-53`. Add one
  arm beside `scan` (≈line 43): `Some("instinct") => instinct::cmd_instinct(&args[1..], &framework_root())`.
- **Module** — new `gatekeeper/src/instinct.rs`; declare `mod instinct;` at `main.rs:25-26` next to
  `mod review;` / `mod scan;`. Mirror `scan::cmd_scan(&[String], &Path) -> i32` (`scan.rs:325-347`):
  the module owns its own inner sub-flag match (`list` / `render` / `_ => usage exit 2`). `--harness`
  parsed with the existing hand-rolled flag-scan idiom (`feature_arg`/`base_arg`, `main.rs:214-232`).
- **Injection point** — inside `cmd_activate` (`main.rs:129-160`), emit an instincts section in the
  gap between the routed-skills loop (ends `:157`) and the trailing gate-warning line (`:158`),
  reusing the `framework_root()` already resolved at `:137`. The preamble is the stdout of
  `gatekeeper activate`, piped back by the `UserPromptSubmit` hook (`hooks/skill-activation.sh:29`).
- **Shared helper** — expose `matching_instincts(root, …) -> Vec<Instinct>` so `activate` and
  `instinct list`/`render` never duplicate the load+filter logic.
- **Tests** — `gatekeeper/tests/cli_instinct.rs`, mirroring `cli_review.rs`/`cli_scan.rs` (scratch
  root with a `skills/` marker + an `instincts/` dir; exec `env!("CARGO_BIN_EXE_gatekeeper")`).
- **New source dir** — `instincts/` (Layer 0), holding the seed `<id>.md` files.

### ⚠️ The central, design-blocking constraint

`route()` (`main.rs:163-190`) matches **lowercased keyword substrings against the prompt text only**.
There is **no file list, no path, no language context** at `activate` time. But the ROADMAP verify
criterion (`ROADMAP.md:75-76`) demands *"a Kotlin instinct doesn't fire on a Markdown-only prompt"* —
i.e. some scoping must work at injection time. **All four lanes independently converged on this as
THE blocker.** A glob-based `applies` cannot be evaluated against a prompt that has touched no file.
This must be resolved before a glob library is even chosen (§6, decision A).

**Update — identity resolved (§1):** instincts are strictly *always-on*, so nothing glob-scoped is
evaluated at `activate` and this blocker no longer gates the engine. Per-path scoping reduces to a
*skill*-layer concern (where keyword routing already lives); decisions A–C in §6 narrow accordingly,
and the file-area-specific seeds in §4 become skill/scan candidates rather than instincts.

### Conventions to honor (verified)

- **Exit codes:** 0 pass/clean, 1 fail/veto, 2 usage/load error (`main.rs:1-17`).
- **Fail-loud *except* don't break the turn:** `activate` falls back to an empty result on unreadable
  sources (`main.rs:146`; hook swallows errors at `skill-activation.sh:29-30`) — a missing
  `instincts/` dir = no instincts, **not** exit 1. But `instinct list`/`render` invoked directly may
  exit 1/2 on a malformed file (fail-loud), mirroring `scan`.
- **Dependencies:** currently `regex`, `serde`, `serde_json`, `toml` (`Cargo.toml:8-11`). Reuse them
  where possible. (No documented cap — see §0 and §6 decision B.)
- **Offline / one static binary**, `Cargo.lock` committed.
- **Test style:** colocated `#[cfg(test)] mod tests` + binary-level integration tests.
- **`cargo fmt` + `clippy -D warnings`** before finishing (`AGENTS.md:48`).

### Self-protection interaction (expected friction, not a blocker)

Editing `gatekeeper/src/main.rs` and (if a crate is added) `Cargo.toml`/`Cargo.lock` trips
`rules.toml` `protected_paths` (`:143-145`) and the `tamper-security-wiring` regex
(`gatekeeper/(src/|Cargo\.)`, `:122`). So the PreToolUse hook will `ask` and pre-commit will gate
those edits — human-approval friction by design. `instincts/` itself need **not** be a protected path
(instincts are soft framing, not the safety floor).

---

## 3. Prior art (external — advisory)

| System | Mechanism | Scoping | Lesson for Topology |
|---|---|---|---|
| **Claude Code** CLAUDE.md / memory | walk up tree, **concatenate** every CLAUDE.md into the system prompt; loaded once per session | always-on by **file presence + dir position**; **no within-file scoping** | Concatenate-don't-merge is the right model (it's what `activate` already does). Every line is paid every session → **keep the always-on set tiny and push anything scoped down to skills** (Topology's operator split, not glob-scoped instincts). |
| **Cursor** `.cursor/rules/*.mdc` | per-rule file injected by activation mode | **richest model:** Always / Auto-Attached (`globs:`) / Agent-Requested (description) / Manual | The converged on-disk shape (frontmatter scope + Markdown why-body) = what Topology already sketched. always-on instincts map to Cursor's **Always** mode; the richer glob → `.mdc` `globs:` (~1:1) mapping now **de-risks the Phase-4 _skill_→Cursor adapter** instead. Caveat: Cursor globs match the *open file*; a headless agent has none. |
| **Codex / AGENTS.md** | concatenate AGENTS.md down the path; `AGENTS.override.md` beats its sibling | **purely by directory placement; no glob/keyword** | Keep AGENTS.md as the lingua franca, but **render only the matching instinct subset into it** per session rather than dumping all statically. Borrow `AGENTS.override.md` as a local-override precedent. |
| **Rules-engine pattern** (priority bands) | numeric priority; higher fires first, can short-circuit; capped injected size | condition + priority | Adopt **priority as deterministic tie-break + truncation order** under a budget. Avoid a heavyweight runtime DSL — a deterministic Rust filter over static Markdown is the right weight. |

### Rust glob options — deferred (no longer an instinct concern)

The always-on identity (§1) removes globs from the instinct engine entirely, so **Phase 2 adds zero new
crates** and the §0 "fifth-crate" question is moot here. The glob-dialect decision (regex-translate vs
`glob` vs `globset`) moves to whoever first needs per-path matching — the **skill** router or the
**Phase-4 Cursor adapter**. Recorded for that stage: `globset` carries braces and its transitive deps
(`aho-corasick`/`regex-automata`) are already in `Cargo.lock` via `regex`, so it stays the front-runner
when a glob is actually needed.

---

## 4. Candidate seed set (proposed, not locked)

Mined from the repo's **own** documented conventions (each cites a source), per the "repo-grounded
over generic" brief. The lane deliberately **omitted** the ROADMAP's `constructor-injection` and
`no-platform-types-in-shared-code` examples as cargo-culting — this codebase is Rust + Bash + Markdown
with no DI container and no shared/platform split, so those are illustrative Anthropic examples, not
conventions Topology follows. **Maintainer concurs** (re-examine if a polyglot/domain stack arrives).

Under the **always-on identity** (§1), an instinct must be guidance *universal to all development in this
repo*. That splits the candidates cleanly: the repo-universal ones stay instincts (sorted into the two
halves of "common sense"); the file-area-specific ones were never instincts — they re-sort to the
operator that actually fits (skill, scan, or the loader itself).

**The always-on instinct set** (always-on by definition — there is no scope field, §5; this is the whole engine at seed scale).

*Behavioral axioms — how to reason here:*

| id | prio | source | why (abbrev.) |
|---|---|---|---|
| `constraints-as-reasoning` | high | ADR-0004 | A guardrail phrased as reasoning generalizes; a bare "NEVER X" doesn't. Founding rationale + house prose style. |
| `evidence-over-assertion` | high | ROADMAP | "Done" = a re-runnable command + its output, never a feeling. |
| `gates-not-rules` | high | AGENTS.md | Phrase commitments as trigger→check→act, not a soft rule with an invisible opt-out. |
| `weakest-enforcement-that-works` | medium | METHODOLOGY | Default to the lightest operator; earn added strength with evidence. |
| `surgical-changes-only` | medium | EXTENDING | Change what the task needs; no drive-by refactors. + the senior-engineer "overcomplicated?" test. |

*Project constraints — non-negotiable facts about this repo:*

| id | prio | source | why (abbrev.) |
|---|---|---|---|
| `three-language-lanes` | high | ARCHITECTURE | Put each change in its lane — Markdown is truth, Rust enforces, Bash only glues; never bridge a behavior across lanes (no logic in Bash, no enforcement in Markdown). |

That is **six** always-on instincts — retiring the earlier "~5" hand-wave (§6.F sets the real cap).
`three-language-lanes` is the lone *constraint*; the rest are *axioms*. **It stays:** per §1 a project
constraint is a first-class kind of instinct, not a lesser one — it need not read as an axiom to earn its
slot. The only fix was phrasing: stated as a bare fact it's reference material, so it's now written as a
*reasoning directive* ("put each change in its lane… never bridge"), which makes it shape a decision
rather than merely inform one. Like every always-on instinct it still faces the §6.H eval (ship +
measure) — no special pass.

**Re-sorted out of the instinct set** (file-area-specific → weakest operator that actually fits):

| former seed | new home | why it isn't an instinct |
|---|---|---|
| `fail-closed-on-adversarial-input` | **Skill** (routed in `gatekeeper/src/**`) | gatekeeper-specific, not repo-universal |
| `trust-boundary-is-code-not-the-model` | **Skill** (gatekeeper) | same; pairs with the above |
| `actionable-error-messages` | **Skill** (gatekeeper / `rules.toml`) | scoped to diagnostic-emitting code |
| `generated-configs-never-hand-edited` | **Scan** (extend `protected_paths`) / skill | a deterministic veto half-exists already |
| `redact-never-echo-secrets` | **Scan** (the security scan already vetoes this) | an instinct would duplicate the existing veto |
| `no-reserved-words-in-operator-names` | **Loader/validator** (already enforced, §5) | a parse rule, not a reasoning nudge — drop |

This re-sort *is* the `weakest-enforcement-that-works` instinct applied to the seed set itself: two of
the six de-homed candidates were already enforced deterministically and never needed to be instincts at
all.

**Follow-up — out of scope for Phase 2 (deferred, not dropped).** Standing these up is a separate change,
held back by `surgical-changes-only` and the research→design→implement staging. Three need no new
artifact: `redact-never-echo-secrets` and `generated-configs-never-hand-edited` are (mostly) covered by
existing scans, and `no-reserved-words-in-operator-names` is the loader validator that ships with the
engine (§5) — these just need their coverage confirmed. The other three
(`fail-closed-on-adversarial-input`, `trust-boundary-is-code-not-the-model`, `actionable-error-messages`)
become new `SKILL.md` files — but note they were *path-scoped* here, while skills route on **keywords**
(`skill-rules.json` `promptTriggers.keywords`). So each needs trigger keywords chosen and verified: a
design step, not a mechanical move, which is the real reason it belongs in its own change (§8).

---

## 5. Format / schema inputs (mirror existing Layer-0 schemas)

Proposed frontmatter for `instincts/<id>.md` (consistency with `SKILL.md` + `rules.toml`):

| field | type | required | semantics |
|---|---|---|---|
| `id` | kebab-case string ≤64, no reserved words | yes | unique; drives filename; reject duplicates like `scan.rs` rejects duplicate rule ids |
| ~~`applies`~~ | — | **dropped in v1** | a one-value field carries no information under the always-on identity (§1); absence ≡ always-on. Re-introduce a versioned `scope` field later only if real scoping appears — the `schema` row below is that reservation, so dropping it costs no forward-compat. |
| `priority` | enum `high\|medium\|low` | no (default `medium`) | **ordering + truncation order only**, never enforcement (instincts are always "soft") |
| body | Markdown, 1–2 sentences | yes | the **why** |
| `source`/provenance | string | no | optional ledger back-link for Phase-3 promotion; accept-but-ignore (the `rules.toml` `reason` precedent) |
| `schema` | int | no (default `1`) | **resolved: per-file `schema: 1`** — instincts are one-file-each, so version per file; reserved now so Phase-3 promotion attaches without a migration |

Adopt `rules.toml`'s **loader discipline**: fail-closed parse, reject duplicate `id`, reject unknown
fields, name the offending id in errors. Preserve the existing **sorted-output** property
(`cmd_list`, `route` call `.sort()`) for idempotent Phase-4 adapters. Strip frontmatter on render and
emit the body only — an always-on instinct has no scope to translate beyond the harness's always-include mode.

**Promotion compat (Phase 3):** keep the required surface minimal (`id` + body) so `learn promote` can
scaffold a valid file from a ledger entry; have `promote` run the same
validator before writing (Phase-3 verify: *"promote produces a valid instinct file"*). **Reserve the
feedback loop now, build it later:** keep `source`/provenance accept-but-ignore (above) and leave room
for a Phase-3 adherence/override signal — the record of when an agent reasons *past* an instinct, which
is the data that tells `promote` whether to keep, promote, or drop it. No telemetry in Phase 2; just
don't design it out.

**Adapter compat (Phase 4):** always-on instincts map to Cursor's **Always** mode (not `globs:`) and to
always-include prose in static AGENTS.md / the Claude preamble — the simplest possible mapping, since
there is no scope to translate. (The richer glob → `.mdc` `globs:` path now belongs to the *skill*
adapter.) Expose one stable in-memory shape both `render` and `adapt` call.

---

## 6. Decisions — resolved and still open (prioritized)

These were the genuine forks. Four are now **resolved by the maintainer** — identity (A), dependency
(B), `applies` shape (C), and efficacy (H) — all cascading from the always-on identity locked in §1; the
seed-purpose call (dogfooding) lands in §4 and the feedback-loop call (reserve, don't build) in §5. The
rest stay open for the design stage. Each is tagged ✅ **RESOLVED** or ⚠️ **OPEN**.

**A. The `activate` input contract — ✅ RESOLVED: instincts are strictly always-on (§1).** The blocker
was "a glob `applies` can't be matched against a fileless prompt." The identity decision removes globs
from instincts, so `activate` simply emits the whole (tiny) always-on set — no file context, no keyword
inference, no hook change. Options (b)/(c) are dropped; per-path matching, if ever needed, lives in the
skill layer where routing already exists. *This also retires the weak ROADMAP verify criterion ("a
Kotlin instinct doesn't fire on a Markdown-only prompt"), which presumed scoped instincts — rewrite it
to exercise the always-on set instead (§7).*

**B. Dependency policy — ✅ RESOLVED (follows A): zero new crates in Phase 2.** With no globs in
instincts there is nothing to match, so the `glob`/`globset`/regex-translate question doesn't arise
here, and the §0 "fifth-crate" inference is moot for this phase. The glob-dialect choice (with `globset`
as front-runner — braces, transitive deps already in `Cargo.lock`) defers to whatever first needs
per-path matching: the skill router or the Phase-4 adapter (§3). Record *that* as an ADR when it lands.

**C. `applies` shape & glob dialect — ✅ RESOLVED: drop the field in v1.** Every instinct is always-on,
so an `applies` field carries no information — it's removed (absence ≡ always-on; §5). Forward-compat is
covered without it: the versioned `schema` field plus the default-to-always rule let a `scope` field be
added back later with no migration. No glob dialect, brace-set, or `languages:` mode is needed in the
instinct engine — those belong to the skill layer if anywhere.

**D. Priority semantics — ⚠️ OPEN (lane contradiction).** Format lane: no hard cap initially (set is
tiny), priority = ordering only. Prior-art lane: bake in budget-truncation (drop lowest priority first).
*Lean: reuse the `high/medium/low` enum; deterministic tie-break by id; no hard cap at seed scale, but
design `render` to accept an optional `--budget` that truncates by priority — the lever that enforces the
§1 leverage-per-token discipline.*

**E. Loader fail-mode matrix — ⚠️ OPEN (simpler now).** Define exit code + skip-vs-fail-closed for
{missing dir, empty dir, one malformed file, duplicate id} × {activate, list, render} — the
"uncompilable glob" case is gone with globs. Phase-3 `promote` validates against this same contract, so
pin it now. *Lean: `activate` never breaks the turn (skip+warn, empty on missing); `list`/`render`
fail-loud (exit 2).*

**F. Token budget — ⚠️ OPEN (now first-class).** Per §1, *leverage per token* is the engine's unit of
value, so the budget is a design constraint, not a footnote. Still to set: a per-instinct word cap + a
total always-on cap, linted at author time in `instinct render`. The set is already trimmed to the six
in §4; de-dupe those against AGENTS.md/CLAUDE.md so the same line isn't paid twice (double-injection).

**G. Per-harness rendering split — ⚠️ OPEN.** How do `instinct render --harness` and the Phase-4 `adapt`
surface divide responsibility (overlap or subset)? Which harness values are valid in Phase 2?

**H. Efficacy — ✅ RESOLVED: ship + measure.** The "reasoning generalizes" premise (ADR-0004) stays
asserted, not proven — so it doesn't gate shipping, but it *does* gate **keeping** each instinct. Build a
small eval in design/verify (instinct vs none vs bare-"don't") that scores *leverage per token*; each
always-on instinct earns its permanent slot or is pruned. First on the block: `three-language-lanes` (§4).

---

## 7. Doc inconsistencies to fix (independent of Phase 2 build)

1. `docs/ARCHITECTURE.md:195` — still cites the **retired** `gatekeeper/src/json.rs` as the JSON
   parser; ADR-0007 moved routing to `serde_json`. Update the gatekeeper-contract paragraph.
2. `docs/ROADMAP.md:108` — "Cursor … globs from each **skill's** `applies`"; skills have no `applies`
   (they route on keywords), and under the always-on decision (§1) instincts have no scope field at all
   (always-on by definition, §5) — so neither feeds Cursor *globs*. The line should describe per-path Cursor scoping as
   deriving from the **skill router**, with instincts mapping to Cursor's **Always** mode.

The ROADMAP correction also reframes the prior-art claim that `applies`→Cursor `.mdc` `globs:` maps
1:1: that glob mapping now belongs to the **skill** adapter (skills route per-path), while always-on
instincts map to Cursor's simpler **Always** mode (§3, §5). *(These edits touch `gatekeeper`-adjacent docs only, not
the frozen Phase-1 code surface; safe to do anytime, but hold commits per the close-out discipline.)*

---

## 8. Recommended next step

Research is sufficient to open the **design stage** — and the maintainer calls above (A/B/C/H resolved,
seed set re-sorted in §4, schema reserved in §5) have already settled what was the design stage's hard
part. The remaining open work, in order: **D** (priority + budget truncation), **E** (loader fail-mode
matrix), **F** (concrete token caps + AGENTS.md/CLAUDE.md de-dupe), **G** (render/adapt split). No ADR
amendment is needed for dependencies (Phase 2 adds none); record the glob-dialect choice only when the
skill/adapter layer first needs it.

> Out of scope here / deferred to design: D–G; the §6.H efficacy eval (now a gate on *keeping* each
> instinct); standing up the de-homed gatekeeper skills (§4); whether `three-language-lanes` survives the
> eval.
