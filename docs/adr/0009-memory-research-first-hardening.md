# 0009 — Memory artifacts as markdown; research as a gated stage

- **Status:** Accepted
- **Date:** 2026-06-08

Phase 5 makes context a managed budget and exploration a gated stage (ROADMAP
[Phase 5](../ROADMAP.md#phase-5--memory--research-first-hardening)). This ADR records the cross-cutting
decisions — what memory *is*, how the research gate works, and the hardening posture — decided after a
deep-research pass on agent-memory prior art and a first-hand read of the `agentmemory` codebase (see
[research](../research/2026-06-08-memory-research-first.md)).

## Decisions

1. **Memory is markdown artifacts; recall is *read*, not *search*. No memory engine.** Handoff
   artifacts are markdown files with YAML frontmatter, read back by path/slug — no vector
   embeddings, no approximate-nearest-neighbour index, no knowledge graph, no SQLite, **no new Cargo
   dependency**. The research surveyed the richer machinery (agentmemory's BM25+vector+graph RRF over an
   iii-engine KV store) and the verdict is to *leave it behind*: ANN earns its keep at thousands-to-
   millions of entries, but a Topology session's memory is a *handful of per-feature artifacts*, where a
   plain read beats any index and an embedding/graph layer would be a database and an ML pipeline bolted
   onto what are otherwise plain files. This is the same stance as
   [ADR-0003](0003-one-markdown-source-per-harness-adapters.md) (one Markdown source) and
   [ADR-0007](0007-security-scanner-dependencies.md) (no new deps), applied to memory
   (instinct: [[constraints-as-reasoning]], [[weakest-enforcement-that-works]]).

   *Amendment (markdown vs JSON for machine-updated state).* Anthropic's long-running-agent harness uses
   **JSON** for its status ledger precisely because *"the model is less likely to inappropriately change
   or overwrite JSON files compared to Markdown files"* — markdown reads as freely-editable prose. We
   keep markdown, because Topology has a mitigation that harness lacks: the model's **file-editing tools**
   (`Write`/`Edit`/`MultiEdit`) are blocked on `memory/artifacts/` by the PreToolUse hook (decision 3), and
   the supported write path is `gatekeeper memory write`, which **stamps** the structured fields. Blocking
   those tools defeats the *accidental prose-clobbering* the JSON finding is actually about — which is the
   failure JSON was chosen to prevent. So the machine-updated state lives in gatekeeper-owned YAML
   frontmatter (`status`, `created`, `head_sha` — authoritative, regenerated on write) and the prose lives
   in the body, buying JSON's protection without a second format. **This is not airtight:** a Bash
   redirection can still write the path (the hook does not parse shell), so the guarantee is "the editing
   tools can't clobber it," not "nothing can" — see decision 3 for the residual and the threat it does and
   does not cover.

   *Flip condition (when JSON wins).* The markdown choice holds because a handoff is a **narrative with a
   few metadata fields**, not a row-oriented ledger. Anthropic reached for JSON for a *tabular* 200-row
   `feature_list.json` the agent updates programmatically and tooling queries — a genuinely different
   shape. If a later phase grows such an enumerable machine-state ledger (per-slice task rows updated by
   the agent), add a companion `<slug>.tasks.json` (via `serde_json`, already a dep — no new substrate)
   **alongside** the markdown handoff, not replacing it: narrative stays markdown, the ledger becomes
   JSON. Absent that shape, markdown+frontmatter is the call.

2. **The `research` gate is a new arm, and the `design` arm is sequence-locked to it.**
   `gatekeeper check research --feature <slug>` is `gate_doc_exists("research", slug)` — it passes iff a
   `docs/research/*<slug>*.md` note exists, exactly as `design` checks `docs/specs/`. But an *independent*
   arm would not actually block design (the existing arms are orthogonal — a real gap Codex caught), so
   the `design` arm is changed to require a research note **first**: it calls `find_doc("research", slug)`
   and fails with a research-first message before its spec check. That makes "explore before you design" a
   deterministic precondition, not a reminder (instinct: [[gates-not-rules]]). The soft layer —
   `skills/research-first/SKILL.md` — tells the agent *how*; the gate is the floor. Reuses the existing
   `find_doc`/`gate_doc_exists` helpers; no new gate machinery.

3. **Memory writes route through `gatekeeper memory write`, reusing the scan pipeline's hygiene — this
   is the whole security story.** The threat that actually fits this design is narrow and concrete: a
   compromised or prompt-injected *prior* session writing a handoff that the *next* session ingests to
   resume. The defence is mechanical, not model-judged — a handoff artifact is emitted by
   `gatekeeper memory write`, which (a) validates `--feature` so the path can't be steered out of
   `memory/artifacts/`, (b) runs the **rendered artifact** (frontmatter + body, so a secret-shaped branch
   name is caught too) through the same secret-**refusal** scan `scan --content` uses — it refuses and
   redacts, it does not silently strip — and (c) ties `status: done` to an existing `docs/verify/` note.
   So no secrets persist (poor exfiltration channel) and the editing tools have no write path to the tree
   (decision 1). **Residual, stated plainly:** the PreToolUse hook does not parse Bash, so a shell
   redirection into `memory/artifacts/` evades the tool block; a heuristic tamper rule in
   `security/rules.toml` raises that bar but does not close it, and unrecognised file-writing tools are a
   further gap. The guarantee is "the editing tools can't clobber it," not "nothing can." We deliberately
   do **not** add an LLM-confidence/"trust score" gate on memory *content*: the research showed such filters
   behave as confidence filters, not security filters, accepting well-phrased poison at maximum
   confidence. Centralising the write is the cheapest place to enforce format and hygiene at once
   (instinct: [[surgical-changes-only]], [[weakest-enforcement-that-works]]).

   *Not imported:* the broader memory-poisoning playbook (MINJA / A-MemGuard, seed-the-store-with-
   legitimate-examples, per-entry trust scoring) targets an **associative semantic store**, where poison
   competes for *retrieval slots* and seeding dilutes it. Our artifacts are read **wholesale** to resume
   a session — there is no associative recall to dilute — so that machinery does not transfer, and we do
   not pretend it does ([research](../research/2026-06-08-memory-research-first.md), instinct:
   [[evidence-over-assertion]]).

4. **Generated artifacts are local runtime state; the format and template are the committed contract.**
   `memory/` (top-level, alongside `instincts/`, `skills/`, `adapters/`) holds the committed protocol:
   the README and a `TEMPLATE.handoff.md` that exists as the *format example* (usability — show the
   frontmatter and section shape), not as a security measure. Per-feature artifacts `gatekeeper` writes
   are session state and are gitignored, not committed — the *round-trip* (write → fresh session →
   `gatekeeper memory read` → resume) is what the Phase-5 verify exercises, not a checked-in artifact.

## Consequences

- Adding "smarter recall" later (semantic search) is a future ADR, not an omission — recorded here as a
  deliberate non-feature for this scale, reversible if the memory corpus ever outgrows a handful of
  per-feature artifacts.
- The research gate changes the canonical sequence to **research → design → plan → … → verify → review**;
  Phase 6 CI can assert the gate ordering without re-deriving it. The sequence is documented in three
  places (METHODOLOGY.md, AGENTS.md, HOW-IT-WORKS.md); all three are updated to drop research's
  `[planned]` marker so the docs match the enforced binary.
- **The `design`→`research` lock binds features designed from Phase 5 onward.** It is a file-existence
  check, so running `check design` for a *pre-Phase-5* feature that shipped without a research note
  (`code-review-gate`, `gate-commands`, `security-scanning`, `continuous-learning`) now returns `1`. This
  is accepted, not backfilled: those features predate the gate and writing retroactive research notes
  would be revisionist. Recorded here so the failure is a known, intended consequence — not a regression.
- One open question the research left unresolved is carried forward: A-MemGuard's strongest poisoning
  defence (multi-memory consensus) assumes a retrieval layer we are declining, so for a read-wholesale
  store the mitigation reduces to decision 3 (controlled writes + mechanical hygiene). A dedicated
  "negative-lesson" defence is left to a spike, noted so it is a decision, not a gap.
- RTK-as-default-shell-proxy and the house-stack domain skills (the other two Phase-5 deliverables) are
  documentation/operator work that does not touch these decisions; they are specified separately.
