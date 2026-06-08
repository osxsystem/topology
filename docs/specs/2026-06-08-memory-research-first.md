# Design: Memory + research-first hardening (Phase 5)

- **Date:** 2026-06-08
- **Feature slug:** memory-research-first
- **Status:** approved (authorized as the Phase 5 build; grounded by
  [research](../research/2026-06-08-memory-research-first.md) and
  [ADR-0009](../adr/0009-memory-research-first-hardening.md))
- **Roadmap:** [Phase 5](../ROADMAP.md#phase-5--memory--research-first-hardening)

## Goal

Make context a **managed budget** and exploration a **gated stage**. Two deterministic additions to
`gatekeeper` — a `research` gate prepended to the workflow sequence, and a `memory` read/write protocol
for handoff artifacts — plus the soft operators that guide their use (`research-first` skill,
RTK proxy docs, house-stack domain skills). Recall is *read*, not *search* (ADR-0009 §1): no embeddings,
no graph, no new dependency.

## Shape

Three additions, all built from the helpers already in `main.rs`/`scan.rs`:

1. **`gatekeeper check research --feature <slug>`** — a new match arm in `cmd_check`
   (`main.rs:216-230`) calling `gate_doc_exists("research", &feature_arg(args))` (`main.rs:270-285`); it
   passes iff `docs/research/*<slug>*.md` exists, same logic as `design`→`docs/specs/`. **And** the
   `design` arm is changed to require a research note *first*: `"design" =>` checks
   `find_doc("research", slug)` before `gate_doc_exists("specs", slug)`, failing with a clear
   "research-first: no research note for <slug>" when absent. This is what makes "research blocks design"
   true at the command level — not just an independent arm (a real gap Codex flagged; the existing arms
   are otherwise orthogonal). `skills/research-first/SKILL.md` is the soft layer on top. *Scope:* the lock
   binds features from Phase 5 onward; the four pre-Phase-5 features shipped without a research note are
   not re-gated or backfilled (ADR-0009 consequences) — a `check design` failure for them is intended.

2. **`gatekeeper memory {write|read|list}`** — a new `memory.rs` module (registered in `main.rs`
   alongside `adapt`/`learn`), emitting and reading markdown **handoff** artifacts under `memory/artifacts/`
   (one kind this phase — see non-goals). The machine-state frontmatter fields are set by **flags**, not by
   the body (the body is prose only): `--feature`, `--date`, `--status {in-progress|blocked|done}` (default
   `in-progress`), `--verified-by <slug>`. `--feature` is validated with `instinct::validate_id`
   (`instinct.rs:84-102`) so it cannot carry `../` path traversal. The **rendered artifact** (frontmatter
   + body, not just the stdin body) is run through a secret-refusal scan before any byte hits disk
   (ADR-0009 §3).

3. **Directory-prefix protection in `scan.rs`** — `is_protected` (`scan.rs:461`) compares *exact*
   resolved paths today, so it cannot guard dynamically-named artifacts. Extend it so a protected entry
   naming a **directory** matches any path beneath it via `Path::starts_with` on the resolved paths
   (component-wise — so `memory/artifacts` matches `memory/artifacts/x.md` but **not** `memory/artifacts-evil/`);
   add `memory/artifacts` to `integrity.protected_paths` in `security/rules.toml`. This blocks the
   `Write`/`Edit`/`MultiEdit` tools on `memory/artifacts/*` via the PreToolUse hook (`scan.rs:905-931`),
   which defeats the *accidental* prose-clobbering the JSON finding worried about (ADR-0009 §1).
   **Residual, stated honestly:** the hook does not parse Bash, so a shell redirection
   (`echo … > memory/artifacts/x.md`) bypasses it; path matching is lexical and does not follow symlinks or
   normalise case; unrecognised file-writing tools are allowed (`scan.rs:970`). A heuristic tamper rule
   (below) raises — does not close — the Bash gap. `gatekeeper memory write` is a CLI command, not a tool
   call, so it is unaffected.

### `memory/` source-of-truth directory (committed contract)

```
memory/
  README.md                # the protocol: what an artifact is, the frontmatter contract  (committed, editable)
  TEMPLATE.handoff.md      # the format example (ADR-0009 §4) — frontmatter + section shape (committed, editable)
  artifacts/                # generated per-feature handoffs — gitignored + write-protected
    <slug>.handoff.md
```
Generated artifacts live under `memory/artifacts/`, kept out of the committed seeds so the two have
opposite policies: seeds are normal source (hand-editable); `artifacts/` is **gitignored** (`/memory/artifacts/`
in `.gitignore`) and **tool-write-protected** so the file-editing tools cannot clobber it (criterion 9,
with the Bash residual noted in surface 3). The split is what lets the protection be a directory prefix
without freezing the editable template (ADR-0009 §4).

### Artifact format

YAML frontmatter + Markdown body. The frontmatter is the machine contract; the body is for the resuming
agent.

```markdown
---
feature: memory-research-first      # slug — how `read`/`list` find it
created: 2026-06-08                 # set by gatekeeper (not the agent); lets `list` surface stale artifacts
branch: feat/memory-research-first
head_sha: 8ea06d8
status: in-progress                 # in-progress | blocked | done — defaults to in-progress; never self-asserted
verified_by:                        # slug/path resolving to an existing docs/verify/*<slug>*.md — REQUIRED before status: done
---

## Goal
<one paragraph: what this feature is for>

## State
- done: <…>
- in-progress: <…>
- blocked: <…, with the blocker>

## Next steps
1. <the very next action a fresh session should take>

## Key files
- `path:line` — <why it matters>

## Decisions & gotchas
- <decision/constraint that won't survive in the diff alone>
```

### Subcommand behaviour

- **`gatekeeper memory write --feature <slug> --date <YYYY-MM-DD> [--status <state>] [--verified-by <slug>]`**
  — reads the artifact body from **stdin** (prose only — a body that opens its own `---` frontmatter block
  is rejected); validates `--feature` (`validate_id`) and `--date` (shape, as `learn.rs` does — no wall
  clock); reads `branch`/`head_sha` via `git` (empty off-repo, a deliberate policy — see Conventions). It
  then **renders** the full artifact and runs the *rendered* text (frontmatter + body, so a secret-like
  branch name or feature string is caught too) through the `scan.rs` secret-refusal scan; on a hit it
  refuses with `exit 1` and the redacted pattern, **writing nothing**. `--status done` is refused (`exit 1`)
  unless a `docs/verify/*<feature>*.md` note exists — "done" is tied to verify evidence, never
  self-asserted (the long-running-agent post's top failure mode). On success it writes
  `memory/artifacts/<slug>.handoff.md` and prints
  `wrote memory/artifacts/<slug>.handoff.md`. The only update path is re-running `memory write` (surface 3
  blocks the editing tools).
- **`gatekeeper memory read --feature <slug>`** — prints `memory/artifacts/<slug>.handoff.md` to
  **stdout** (for injection into a fresh session) and exits `0`; exits `1` with a clear message if absent.
- **`gatekeeper memory list`** — a **plain directory read** of `memory/artifacts/`: prints
  `slug · created · status` per artifact so stale ones are visible. Read-only; explicitly **not** a
  query/ledger layer (non-goal).

### Soft operators (no `gatekeeper` code)

- **`skills/research-first/SKILL.md`** — keyword-routed skill (house format: "Use when …") that drives
  the research method (decompose → gather → cite → verify) and ends by leaving a `docs/research/` note,
  which is exactly what gate (1) checks. **Heavy exploration is delegated to a subagent** whose returned
  summary *becomes* the note — subagents get fresh isolated context and return only a summary, so research
  doesn't bloat the main window (how-claude-code-works; this is how the Phase-5 research itself was run).
  Reach is per-harness (Codex review corrected the earlier "every harness" claim): **Claude** reads
  `skills/` natively; **Cursor** and **OpenCode** receive a copy via `adapt` (`adapt.rs:221-256`); **Codex**
  gets no skills copy — it reaches the skill only through the `AGENTS.md` contract (`adapt.rs:188-195`).
- **`skills/resume/SKILL.md`** — the resume routine the long-running-agent post found load-bearing: on a
  fresh session, **`gatekeeper memory read` → read `git log` → run a smoke/build check → only then act**.
  Reading the handoff is necessary but not sufficient; verifying state catches *undocumented* broken state
  before the agent builds on it. Pairs with one-slice-per-session (the plan gate's tiny-commit breakdown,
  [[surgical-changes-only]]).
- **Compact Instructions in `AGENTS.md`** — a short "Compact Instructions" block telling Claude Code's
  auto-compaction to preserve handoff-relevant state (current slice, next step, open decisions), since
  early-conversation detail is dropped first. Complements the harness's compaction rather than fighting
  it; nearly free.
- **RTK proxy docs** — document RTK as the default shell proxy and the install wiring (the
  `UserPromptSubmit`/command-rewrite hook), per ROADMAP. Documentation + an opt-in install step only; no
  `gatekeeper` surface.
- **House-stack domain skills** — the `code-review` critic skill + `review` gate were pulled forward and
  delivered 2026-06-05 ([ADR-0006](../adr/0006-code-review-gate.md)); remaining domain skills are
  authored as `skills/<name>/SKILL.md`, no new machinery.

## Acceptance criteria (checked in the verify note)

1. **Research gate exists *and* blocks design.** With no `docs/research/*<slug>*.md`:
   `check research --feature S` exits `1`, **and** `check design --feature S` exits `1` with the
   research-first message *even when a spec exists* (the sequence-lock). After a research note exists,
   `check research` exits `0` and `check design` falls through to its normal spec check. Missing
   `--feature` exits `2`.
2. **Handoff round-trips.** `… memory write --feature S --date 2026-06-08` (body on stdin) writes
   `memory/artifacts/S.handoff.md` with stamped `created`/`branch`/`head_sha`; in a fresh session
   `… memory read --feature S` prints the artifact byte-for-byte (modulo the trailing newline) and exits
   `0`; `read` on an unknown slug exits `1`.
3. **Write hygiene holds, on the rendered artifact.** A secret in the **body** (e.g. an
   `AWS_SECRET_ACCESS_KEY`-shaped token) *and* a secret-shaped value reachable via a stamped field make
   `… memory write` exit `1` and write **nothing** (target file absent after the refusal) — proving the
   scan runs over the rendered artifact, not just the stdin body.
4. **Format template present.** `memory/TEMPLATE.handoff.md` exists, is tracked, and parses as a valid
   artifact (frontmatter + the body sections above) — the format example, not a security measure.
5. **`list` is read-only and accurate.** After writing two artifacts, `… memory list` shows both with
   their `kind`/`created`/`status`; it mutates nothing.
6. **No new dependency; suite stays green.** `gatekeeper/Cargo.toml` and `Cargo.lock` are unchanged
   (ADR-0009 §1); the existing suite plus new `memory.rs`/gate tests pass; `cargo clippy -- -D warnings`
   and `cargo fmt --check` are clean.
7. **`code-review` subagent returns findings against the plan** (carried from the ROADMAP verify; the
   gate exists since 2026-06-05, re-asserted here for the Phase-5 sequence).
8. **"Done" is tied to verify evidence.** `… memory write` with `status: done` exits `1` when
   `verified_by` is empty **or** names a non-existent verify note; it succeeds only when `verified_by`
   resolves to an existing `docs/verify/*<slug>*.md`. Defaulted/omitted `status` is `in-progress`.
9. **Protection guards the tree, not its siblings, and resists aliases.** After the dir-prefix extension
   (surface 3): `scan --check-path memory/artifacts/<slug>.handoff.md` exits `1`; `memory/TEMPLATE.handoff.md`
   and `memory/artifacts-evil/x.md` exit `0` (no prefix collision); and the protected verdict is unchanged
   under an absolute in-repo path, a `..` alias that resolves into `memory/artifacts/`, and a trailing
   slash. (Symlink-following and case-folding are **out of scope** — `is_protected` is lexical; recorded
   as a residual, not a tested guarantee.)
10. **Input is validated.** `memory write` with an invalid `--feature` (fails `validate_id`, e.g.
    `../escape`), a malformed `--date`, or a body that opens a second `---` frontmatter block exits
    non-zero and writes nothing.
11. **Bash residual is documented, not silently passed.** The verify note records that a Bash redirection
    into `memory/artifacts/` is *not* blocked by the tool hook (only raised by the heuristic tamper rule in
    `rules.toml`), so the limitation is an explicit, reviewed decision rather than an unstated gap.

## Non-goals (this phase)

- **Semantic recall** — no embeddings/ANN/knowledge-graph/SQLite (ADR-0009 §1). Recall is read-by-slug;
  smarter recall is a future ADR if the corpus ever outgrows a handful of per-feature artifacts.
- **A "negative-lesson" poisoning defence** (A-MemGuard-style multi-memory consensus) — it assumes a
  retrieval layer we are declining; left to a spike (ADR-0009 consequences, research open-question A6.1).
- **Trust-scoring / LLM-confidence gating of memory content** — explicitly rejected (ADR-0009 §3); write
  hygiene is mechanical, not model-judged.
- **Auto-writing handoffs from a hook** — `memory write` is an explicit, opt-in action this phase, so the
  agent (or the human) decides when to checkpoint; a Stop-hook auto-handoff is deferred
  (instinct: [[surgical-changes-only]]).
- **Committing per-feature artifacts** — they are gitignored runtime state (ADR-0009 §4); only the
  protocol + template are tracked.
- **A query/ledger layer** — `memory list` is a plain directory read, not a searchable index; if a
  row-oriented task ledger is ever needed it is the JSON-companion flip in ADR-0009 §1, a later phase.
- **A separate `compaction` artifact kind** — a handoff written before context fills serves the same
  purpose, so this phase ships one kind (`handoff`). The Compact *Instructions* in `AGENTS.md` already
  guide the harness's automatic compaction; a distinct artifact kind would be a name (and concept)
  collision without a difference. Add a kind axis only if a genuinely different artifact shape appears
  (same flip discipline as the JSON-companion question).
- **Closing the Bash / symlink / case residuals** — surface 3 blocks the file-editing tools and raises the
  Bash bar with a tamper rule, but a determined shell redirection, a symlink alias, or a case-folded path
  can still reach `memory/artifacts/`. These are recorded limitations (criterion 11), not in-scope to close
  this phase ([[surgical-changes-only]]); the real guarantee is "the editing tools can't clobber it."
