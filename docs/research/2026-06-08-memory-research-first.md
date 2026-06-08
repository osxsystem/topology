# Research: Memory + research-first hardening (Phase 5)

- **Date:** 2026-06-08
- **Feature slug:** memory-research-first
- **Question:** For Topology's file-based agent memory (one fact per markdown file with frontmatter +
  a `MEMORY.md` index loaded each session) under a *research-first* discipline, what does the field's
  prior art say about (1) memory architectures, (2) research-first/retrieval-first patterns,
  (3) failure modes & hardening, and (4) the competitive landscape — and can we **steal** the design of
  the locally-cloned `agentmemory` repo (`~/Codes/AgentTools/agentmemory/`) and adapt it into Phase 5?

> Two evidence tracks, kept separate on purpose (instinct: [[evidence-over-assertion]]). **Part A** is a
> web deep-research pass: every claim was adversarially verified by a 3-vote panel (a claim survives only
> if it is *not* refuted ≥2/3); refuted claims are listed so we don't repeat them. **Part B** is a
> first-hand read of the `agentmemory` source on disk — every mechanism is cited as `file:line`, and
> README marketing numbers are flagged where the code doesn't back them. Where the two tracks touch the
> same fact (agentmemory's benchmark numbers), they **agree**: do not trust those numbers.

---

## Part A — Web deep-research (cited, adversarially verified)

Run shape: 5 angles → 24 sources fetched → 115 claims extracted → 25 verified (3-vote) → **17 confirmed,
8 killed**. Confidence tags below are the panel's, not mine.

### A1. Agent memory architectures

- **Our index-plus-topic-file layout is the production reference design, not an outlier [HIGH].**
  Claude Code's own memory is a per-project directory with a `MEMORY.md` *entrypoint that "acts as an
  index … to keep track of what's stored where"* plus optional topic files. The first **200 lines or
  25KB of `MEMORY.md`, whichever comes first, loads each session** — a concrete budget to design around;
  the 25KB truncation is confirmed real production behavior (not just spec).
  Sources: `code.claude.com/docs/en/memory`, `/how-claude-code-works`, `anthropics/claude-code#57574`.
  → *Design constraint for us:* keep `MEMORY.md` under ~200 lines / 25KB; our one-line-per-memory index
  convention is correct and is a budget decision, not a style choice.

- **File-based / local-first is a legitimate competitive architecture [HIGH].** `agentmemory` runs on
  SQLite + an "iii-engine" with **zero external DBs**, explicitly contrasted vs Mem0 (Qdrant/pgvector)
  and Letta/MemGPT (Postgres). Confirms file-based is a real choice. Source: `github.com/rohitg00/agentmemory`.
  ⚠️ **Do NOT cite** agentmemory's headline recall numbers (R@5=95.2%, Mem0 68.5%, Letta 83.2%) — they
  **failed verification** (votes 0-3 and 1-2). Part B independently confirms these are *self-measured /
  in-house* benchmarks.

- **Cross-session automated memory is an established 2026 pattern [HIGH]**, independent of Claude Code
  (OPENDEV, arXiv 2603.05344). Caveat: most non-Claude-Code systems don't disclose file-vs-vector
  substrate, so the file-vs-vector recall/ranking tradeoff at scale stays under-evidenced.

### A2. Research-first / retrieval-first patterns

- **The phased agentic loop is real prior art [HIGH].** Claude Code's loop is explicitly
  **gather context → take action → verify results**, making context-gathering a first-class phase; docs
  recommend separating research from coding for complex problems as producing better results.
  Source: `code.claude.com/docs/en/how-claude-code-works`.

- **Read-only enforcement is the concrete mechanism [HIGH].** Plan Mode restricts the agent to read-only
  tools and requires an approved plan before any mutation. → *Our analog:* gate memory **writes/edits**
  behind a read-and-plan phase. Source: same.

- **Cite-before-you-answer measurably improves grounding [HIGH].** Amazon "Cite Before You Speak" (SIGIR
  2025): grounding 83.86% → 95.46%, **~13.83% relative** (≈11.6pp absolute). Source: `arxiv.org/html/2503.04830v3`.
  ⚠️ Precision: that is **relative**, not pp. Do **not** claim citation *prevents* hallucination
  architecturally — the "mechanical overlap cannot hallucinate" / "100% refusal signal" claims both
  **failed verification** (0-3, 1-2). Citation helps probabilistically; it is not a guarantee.

### A3. Failure modes & hardening (the highest-leverage area)

- **Memory poisoning is the dominant documented failure mode [HIGH].** Achievable via **query-only
  interactions**, no privileged access, using three named MINJA techniques (NeurIPS 2025) — *bridging
  steps*, *indication prompts*, *progressive shortening*; >95% injection / >70% attack success across
  GPT-4o-mini, Gemini-2.0-Flash, Llama-3.1-8B. Sources: `arxiv.org/html/2601.05504v2`, `/2503.03704`.

- **It is context-dependent, which defeats per-entry auditing [HIGH].** Injected records look harmless in
  isolation, activate maliciously only in context (e.g. "always prioritize urgent-looking emails");
  detectors miss ~66% of poisoned entries. → *Our exposure:* per-markdown-file review — the natural way
  to audit one-fact-per-file — is exactly the review poisoning defeats.
  Sources: `arxiv.org/html/2510.02373`, `/2503.03704`, `/2512.16962`, `/2605.15338`.

- **Poisoning self-reinforces [HIGH]:** a corrupted outcome stored as precedent amplifies the error *and*
  lowers the threshold for future attacks → early prevention ≫ later cleanup. Source: `arxiv.org/html/2510.02373`.

- **💡 Cheapest effective mitigation — pre-populate with legitimate examples [MEDIUM]:** attack success
  drops from ~62% ASR (empty memory) to as low as 0–6% (memory seeded with correct examples). *Single
  non-peer-reviewed preprint, EHR domain, small models — strong hint, not settled.* Nearly free to adopt:
  ship the memory store seeded with trusted reference facts, never empty. Source: `arxiv.org/html/2601.05504v2`.

- **⚠️ Trust-score sanitizers can fail catastrophically [MEDIUM]:** they act as *confidence* filters, not
  *security* filters — well-phrased attacks pass because the model is sure. One study: 82 entries accepted
  at max trust 1.0, 54 confirmed poison. A "continuous trust-scoring rejects all poison" claim was
  **refuted (0-3)**. Don't gate memory writes on a single LLM-confidence score. Source: `arxiv.org/html/2601.05504v2`.

- **Working defense pattern — A-MemGuard [HIGH]:** compare reasoning paths across multiple *related*
  memories (flag deviation from consensus) → store anomalies as "negative lessons" in a separate
  dual-memory store → run a pre-execution check that retrieves similar lessons and triggers replanning.
  (>95% attack reduction is self-reported.) Caveat for us: assumes multiple retrievable related memories
  + structured reasoning paths — does **not** map cleanly onto one-fact-per-markdown-file (open question).
  Source: `arxiv.org/html/2510.02373`.

- **Unbounded growth — concrete decay/compaction [HIGH/MEDIUM]:**
  - *Adaptive context compaction* (OPENDEV): five token-pressure stages (70% warn → 80% observation
    masking → 85% fast prune → 90% aggressive → 99% LLM summarize-the-middle, recent kept verbatim);
    ~54% peak-context reduction. Source: `arxiv.org/pdf/2603.05344`.
  - *Read-time temporal grounding* (SSGM): Weibull decay `w(Δτ) = exp(−(Δτ/η)^κ)` where Δτ = time since
    **last successful retrieval** (so hot facts stay fresh); prune below `θ_fresh` *before* the entry
    reaches context. Source: `arxiv.org/html/2603.11768v1`.

- **Failure taxonomy for hardening coverage [MEDIUM]** (one framework, not consensus): **stability**
  (semantic/procedural/goal drift), **validity** (hallucinated recall, temporal obsolescence),
  **efficiency** (retrieval latency, index bloat), **safety** (poisoning, privacy leakage).
  Sources: `arxiv.org/html/2603.11768v1`, `/2602.19320`.

### A4. Competitive landscape (weakest-covered area)

- **Claude Code** — fully verified above: file-based, `MEMORY.md` index + topic files, 200-line/25KB budget.
- **agentmemory** — verified file-based/local-first (SQLite + iii-engine, 0 external DBs); recall
  benchmarks **refuted**. (Part B is the full first-hand read.)
- **Mem0** (Qdrant/pgvector), **Letta/MemGPT** (Postgres) — only survived as architectural contrasts; no
  verified mechanics.
- **Cursor, OpenAI memory, LangChain/LangGraph** — **not covered by any surviving claim.** Targeted
  follow-up needed if we want these.

### A5. Caveats & refuted claims (do not repeat)

- **Source-strength asymmetry:** strongest = first-party Claude Code docs + the peer-reviewed Amazon
  paper. The most actionable *numbers* (62%→0–6% ASR, trust-filter failure, ~54% compaction, Weibull
  decay, 4-category taxonomy) are each a **single non-peer-reviewed preprint**, often small models /
  narrow domain. Treat as case studies. A-MemGuard ">95%" and OPENDEV figures are self-reported.
- **8 refuted claims to avoid:** agentmemory recall numbers; "citation verification cannot hallucinate"
  guarantees; mechanical-overlap zero-false-positive; citation-first 100%-refusal-signal; continuous
  trust-scoring as a reliable poisoning defense; SSGM truth-maintenance poisoning prevention.
- Claude Code docs verified live **2026-06-08**; fast-moving space.

### A6. Open questions

1. Best poisoning defense for a **pure file-based** system with no embedding/retrieval layer (A-MemGuard
   assumes retrievable related memories + reasoning paths).
2. Safe **automatic fact extraction + dedup** — verified techniques not surfaced, and dedup itself can be
   an injection vector.
3. File-vs-vector recall tradeoff at scale — under-evidenced (the benchmarks that would answer it were
   refuted).
4. Cursor / OpenAI / LangGraph memory mechanics — not covered.

---

## Part B — `agentmemory` codebase deep-dive (first-hand, `file:line`)

Read on disk at `~/Codes/AgentTools/agentmemory/`. ~37,800 LOC, 174 source files, monorepo-ish layout
(`src/`, `packages/`, `integrations/`, `plugin/`, `.claude-plugin/`, `.codex-plugin/`).

### B1. Primary purpose

A **persistent, searchable memory system for AI coding agents** that kills session amnesia: it silently
captures tool use via lifecycle hooks, compresses observations into facts/patterns, indexes them, and
injects relevant memory at session start. Works across 15+ harnesses via MCP / REST / native plugins.
Evidence: `README.md:1-50`, `AGENTS.md:1-10`, `src/index.ts:160-215`.

### B2. Core mechanics (end to end)

- **Storage substrate — VERIFIED file-based SQLite via iii-engine, "0 external DBs".** Everything routes
  through iii's KV state module. `iii-config.yaml:10-16` → `adapter.name: kv`, `store_method: file_based`,
  `file_path: ./data/state_store.db`. `src/index.ts:219` → `const kv = new StateKV(sdk)`. 40+ KV scope
  paths in `src/state/schema.ts:1-75` (`mem:sessions`, `mem:obs:${sessionId}`, `mem:memories`,
  `mem:graph:nodes/edges`). No Postgres/Redis/vector DB.
- **Ingestion pipeline** (`src/functions/observe.ts:43-150`): raw observation → SHA-256 dedup over a
  5-min window (`:64-78`, `dedup.ts`) → privacy strip (`:80-87`, `privacy.ts`) → store raw → **LLM
  compress** (`compress.ts:67-120`) *or* zero-LLM `compress-synthetic.ts`. Compression output:
  `type, title, facts[], narrative, concepts[], files[], importance(1-10)` (`compress.ts:44-65`).
  **Automatic fact extraction = yes**, via XML-parsed `facts[]`.
- **Retrieval — triple-stream hybrid + RRF** (`src/state/hybrid-search.ts:38-127`): BM25 keyword
  (`:82`) + vector cosine (`:91-98`, local `all-MiniLM-L6-v2` default, 6 providers optional) + graph
  entity traversal (`:100-115`), fused via **RRF (k=60)**, session-diversified (max 3/session). Optional
  cross-encoder reranker (`reranker.ts`, off by default).
- **Lifecycle — 4 tiers** (`README.md:869-880`, `consolidate.ts:65-120`): Working (raw) → Episodic
  (session summary) → Semantic (facts/patterns) → Procedural (workflows). Consolidation runs on a timer
  (`consolidation-pipeline.ts`, default 2h).
- **Decay/eviction** (`evict.ts:23-29`): `staleSessionDays:30`, `lowImportanceMaxDays:90`,
  `lowImportanceThreshold:3`, `maxObservationsPerProject:10_000`. Ebbinghaus-style retention scoring
  (`retention.ts`); lessons strengthen on access, decay on time (`lessons.ts`).
- **Data model** (`src/types.ts`): `RawObservation` (`:30-44`) → `CompressedObservation` (`:46-64`) →
  `Memory` (`:83-105`, with `type, strength, version, supersedes[], isLatest, forgetAfter?, agentId?`).

### B3. Design patterns

- **iii-engine as the whole runtime** (`AGENTS.md:1-10`, `README.md:1158-1200`): every stateful op is a
  registered iii function (Worker/Function/Trigger). It *replaces* the conventional stack — Express→iii
  HTTP triggers, SQLite+pgvector→iii KV, Socket.io→iii streams, pm2→iii supervision. 50+ `mem::*`
  functions registered in `src/index.ts:235-334`.
- **Hook pattern — two flavors** (`AGENTS.md:93-97`): *context-injecting* hooks (SessionStart,
  PreToolUse, PreCompact) `await` and write recalled context to stdout; *telemetry* hooks (PostToolUse,
  Stop…) fire-and-forget + force-exit so they never block the agent.
- **Surfaces:** MCP server with 53 tools (8 core always-on, 45 opt-in via `AGENTMEMORY_TOOLS=all`;
  `mcp/tools-registry.ts`), REST (128 endpoints, `triggers/api.ts`), iii SDK (Py/Rust/Node), a real-time
  viewer (`viewer/server.ts`, `:3113`), and 15 agent skills.
- **Knowledge graph** (`functions/graph.ts:1-150`): 13 entity types, 17 edge types, precomputed
  `GraphSnapshot` for 75K+ nodes, temporal edges (`tcommit/tvalid/tvalidEnd`).

### B4. Strengths / bottlenecks / risky choices

- **Strengths:** zero external deps; auto-capture via hooks (no manual `remember()`); hybrid retrieval;
  multi-agent coordination (leases/signals/mesh); 4-tier consolidation + decay; live observability.
- **Bottlenecks:** large-graph enumeration hits iii's 8s invocation timeout (`graph.ts:36-41`); vector
  index rebuild "can take HOURS" on provider switch (`index.ts:449-470`); embedding cost/rate limits;
  SessionStart context-injection burns tokens; auto-compress hits the LLM per PostToolUse.
- **Risky choices:** graph dedup merges by `(type, name)` → silent merges on naming drift
  (`schema.ts:37`); soft-fail to BM25-only when embeddings are down stores the obs *without* a vector,
  needing a full rebuild later (`search.ts:94-98`); OTEL at 10% sampling to avoid a 137GB-log feedback
  loop (`iii-config.yaml:49`).
- **Marketing vs code:** README's R@5=95.2% etc. trace to `eval/runner/longmemeval.ts` + an **in-house**
  `coding-agent-life-v1` corpus — **self-measured, not independently verified** (matches Part A's
  refutation). Token-savings math (170K vs 650K tokens/yr) doesn't reconcile with the captured workload
  (888K tokens / 35h) in `README.md:1267` — extrapolation is hand-wavy.

---

## Part C — Can we steal it for Phase 5? Verdict.

**Steal the *patterns*, not the *stack*.** agentmemory's value to us is its battle-tested *lifecycle and
hardening model*; its retrieval power is welded to SQLite + an embedding index + the iii runtime, none of
which port to plain markdown files without becoming a different product (instinct:
[[weakest-enforcement-that-works]] — take the cheapest mechanism that delivers the behavior).

### C1. Directly portable to one-fact-per-markdown + `MEMORY.md` (do these in Phase 5)

1. **Seed memory with legitimate examples, never ship empty** — the single cheapest poisoning mitigation
   (Part A: 62%→0–6% ASR). Cross-checks with agentmemory's "synthetic/no-LLM" baseline path.
2. **Importance + freshness frontmatter → eviction policy.** Adopt agentmemory's thresholds as defaults:
   `importance: N (1-10)`, drop if `<3` and older than 90d; track `lastAccessedAt`/`accessCount` and
   apply Weibull/Ebbinghaus read-time decay keyed to **last successful retrieval** (Part A SSGM +
   B `retention.ts`). Frontmatter + a cron/hook, no DB.
3. **Two-flavor hook contract** (`AGENTS.md:93-97`): context-injecting hooks write recalled memory to
   stdout at SessionStart; telemetry hooks fire-and-forget. Maps onto Topology's existing hook layer.
4. **Dedup window before write:** SHA-256 of (normalized fact) against a small recent-writes list, 5-min
   window — prevents duplicate `.md` files. Pure file op.
5. **Privacy/secret strip before persisting** — port the regex set from `privacy.ts`.
6. **4-tier naming as a conceptual lifecycle** for our files: raw session log → `session.md` (episodic) →
   `MEMORY.md` facts (semantic) → recipe/instinct files (procedural). No code, just discipline.
7. **`agentId` + `commits[]` frontmatter** for scoping and reproducibility (cheap, high value across
   harnesses — ties into [[cross-harness-adapters]]).
8. **Gate writes behind read-and-plan** (Part A Plan Mode) — the research-first analog: no memory write
   without a preceding recall/plan step.

### C2. Needs adaptation / partial

- **RRF fusion** is *algorithmic*, not storage-bound — we can fuse `grep`/BM25 + frontmatter-link "graph"
  + (optionally) tiny precomputed embeddings. Worth a spike, but plain BM25-over-files may be enough at
  our scale.
- **Consensus + negative-lesson poisoning defense (A-MemGuard):** the *negative-lesson store* (a separate
  "things that went wrong" memory checked pre-action) ports as a markdown file; the *multi-memory
  consensus* part assumes a retrieval layer we don't have → open question A6.1.

### C3. Do NOT port (welded to SQLite/embeddings/iii runtime)

- Vector index + cosine ANN search (needs FAISS/hnswlib + runtime rebuild).
- Knowledge graph with temporal-edge traversal at scale (needs a graph store; frontmatter links are a
  hand DAG, not queryable).
- Mesh/P2P sync via leases+signals (use `git` for our sync instead).
- Sketches/crystallize action graphs, query-time facet indexing, real-time WebSocket viewer.

### C4. Recommended Phase 5 scope (from C1, in priority order)

1. Seed-with-legitimate-examples + privacy strip on write (cheapest, highest security ROI).
2. Frontmatter `importance`/freshness + read-time decay & eviction (bounds growth — our `MEMORY.md`
   25KB/200-line budget makes this load-bearing, per A1).
3. Write-time dedup window.
4. Read-and-plan gate on memory writes (research-first enforcement).
5. *(spike)* negative-lesson store; *(spike)* RRF over grep+links.

> Net: `agentmemory` is a strong **idea quarry**, not a dependency. We lift its hardening defaults and
> hook/lifecycle shape into plain markdown; we leave its SQLite/embedding/iii machinery behind. Every
> "stolen" item above is implementable without adding a database.

---

## Part D — Long-running-agent harness sources (follow-up, 2026-06-08)

A second pass on three first-party Anthropic sources, all about *long-running agent harnesses* — the
exact problem Phase 5's memory + research-first layer addresses. These both confirm the design and
challenge one decision.

Sources:
- `code.claude.com/docs/en/how-claude-code-works` (the loop, compaction behaviour, subagent isolation)
- `github.com/anthropics/claude-quickstarts/tree/main/autonomous-coding` (runnable two-agent pattern)
- `anthropic.com/engineering/effective-harnesses-for-long-running-agents` (the design rationale)

### D1. What they confirm

- **A handoff artifact is the right primitive.** *"The key insight here was finding a way for agents to
  quickly understand the state of work when starting with a fresh context window, which is accomplished
  with the `claude-progress.txt` file alongside the git history."* → our `gatekeeper memory write/read`.
- **Compaction alone is insufficient — you need a manual structured summary on top.** *"Compaction isn't
  sufficient … supplement automatic compaction with manual structured summaries written by agents
  themselves."* This is the standing justification for a memory protocol existing *alongside* Claude
  Code's built-in compaction (how-claude-code-works: auto-compaction "clears older tool outputs first,
  then summarizes … detailed instructions from early in the conversation may be lost").
- **Research/explore-before-act is gated.** Plan Mode = read-only tools + approve-before-execute;
  *"separate research from coding … produces better results than jumping straight to code."* → research
  gate (ADR-0009 §2).
- **Security floor matches.** The quickstart's bash allowlist + filesystem restriction + security hook
  (`security.py`, `.claude_settings.json`) is the same shape as `gatekeeper scan` as a PreToolUse hook.

### D2. What they challenge — JSON vs markdown for machine-updated state

The engineering post is explicit: *"We landed on using JSON for this, as the model is less likely to
inappropriately change or overwrite JSON files compared to Markdown files. This protects critical state
from accidental modification."* Their machine-updated **status ledger** (`feature_list.json`, 200 items,
all initially *failing*) is JSON on purpose; only the **prose** (`claude-progress.txt`) is free text.

This is a direct counter to ADR-0009 §1's flat "memory is markdown" framing — but only for the
*machine-updated status*, not the prose. **Resolution (carried into ADR-0009 §1/§3):** Topology has a
mitigation the quickstart lacks — the agent **never hand-edits** a memory artifact; `gatekeeper memory
write` owns the write and **stamps** the structured fields. A file the model does not edit cannot be
clobbered by the model, so the structured fields stay in gatekeeper-owned YAML frontmatter (still
markdown, no new substrate) and are treated as authoritative; the body stays prose.

### D3. The triad that actually made it work (the real lesson)

The file is necessary but not sufficient; three disciplines around it are what mattered:

1. **One slice per session.** *"The agent tended to try to do too much at once — essentially to one-shot
   the app … ran out of context mid-implementation."* Fix: *"work on only one feature at a time."* →
   pairs with Topology's plan-gate tiny-commit breakdown ([[surgical-changes-only]]).
2. **Verify state *before* acting on resume.** *"Start the session by reading the progress notes file and
   git commit logs, and run a basic test on the development server to catch any undocumented bugs."* →
   `memory read` must be wrapped in a read → git-log → smoke-check routine, not just printed.
3. **Never self-assert "done."** *"Claude's tendency to mark a feature as complete without proper
   testing"* was a top failure mode; everything starts *failing* and *"it is unacceptable to remove or
   edit tests."* → tie handoff `status: done` to the `verify` gate / linked evidence.

### D4. Two cheap wins

- **Subagent-delegated research.** how-claude-code-works: *"Subagents get their own fresh context …
  return a summary. This isolation is why subagents help with long sessions."* The `research-first` skill
  should fan heavy exploration out to a subagent whose summary *becomes* the `docs/research/` note the
  gate checks — protects main context and produces the gated artifact (this very report was produced that
  way).
- **Compact Instructions in `AGENTS.md`.** Claude Code honours a "Compact Instructions" block to control
  what survives auto-compaction; one paragraph preserving handoff-relevant state is nearly free and
  complements the harness rather than fighting it.
