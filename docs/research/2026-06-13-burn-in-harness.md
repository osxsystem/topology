# Research — Phase 15 burn-in harness (unblock the warn→block flip with evidence)

- **Date:** 2026-06-13 · **Feature slug:** `burn-in-harness`
- **Source of truth:** ROADMAP Phase 15 (`docs/ROADMAP.md:513-545`), the flip criterion (`docs/adr/0017-tdd-red-green-replay.md:40`, `docs/adr/0018-entropy-scanning.md`), and the shadow sink (`gatekeeper/src/verify.rs:148-216`).
- **Method:** Explore subagent fan-out (grep/Read) + `codebase-retrieval` cross-check. Every factual claim carries a `file:line` citation; inferences are marked ASSUMPTION.

## The question

The user asked to "kick off the Phase 15 tail — flip the two shadow-mode engines to enforcing once burn-in shows <2% false-block." The flip is gated on burn-in *data*. This note establishes whether that data exists, how it is measured, and what is actually missing — i.e. what a "burn-in harness" must build before any flip is defensible.

## Sub-questions and findings (cited)

### 1. The two engines and their enforce switches

- **TDD red-green replay** (ADR-0017). Config flag `[tdd] mode = "history" | "replay"` (default `history`); `let enforcing = cfg.tdd_mode == TddMode::Replay` decides block-vs-warn (`gatekeeper/src/tdd.rs:456`). Verdicts (`Pass`/`Fail`/`Indeterminate`) route through `verify::emit_shadow("tdd","replay",…)` on every path (`tdd.rs:498-588`). In `history` mode the exit code is unchanged (0) and the verdict is logged; in `replay` mode `Fail`→exit 1, `Indeterminate`→exit 2 (fail-closed).
- **Entropy scanner** (ADR-0018). Rules ship `severity = "warn"`; `scan.rs report()` emits `WARN` to stderr and **exits 0** for warn-severity, exit 1 only for `Block` (`docs/research/2026-06-13-entropy-scanner.md:15`). The flip is a per-rule `severity` change `warn`→`block` in `security/rules.toml` (a protected path).

### 2. **The two engines measure burn-in on DIFFERENT channels** (the key finding)

- **TDD replay → `shadow.jsonl`.** `emit_shadow` appends one 8-field JSON line (`ts,gate,check,configured,artifact,command,result,detail`) to `<artifacts_root>/logs/shadow.jsonl` (framework: `docs/logs/shadow.jsonl`, **gitignored**) — fail-silent I/O (`gatekeeper/src/verify.rs:148-216`). `scripts/shadow-stats.sh` aggregates it into a per-(gate,check) table with eval counts and a `would-block%`, lists each `fail` for triage, and prints the flip criterion (`scripts/shadow-stats.sh:1-121`).
- **Entropy → NOT `shadow.jsonl`.** The scanner never calls `emit_shadow`; warn findings only hit stderr (`docs/research/2026-06-13-entropy-scanner.md:15`). Its scoreboard is the secrets bench `gatekeeper/tests/cli_scan_bench.rs` ("the FM5 scoreboard", `:1-30`): negatives must produce no `BLOCK` (WARNs are expected and *not* asserted away), so FP rate is "measured in burn-in, not asserted here" (`:21-25`, `docs/specs/2026-06-13-entropy-scanner.md:19-26`).

ASSUMPTION: nothing currently runs the entropy scanner across full git history to produce the "FP <1 per 10k lines" figure ADR-0018 names — the bench is a fixed 6-negative / 11-positive corpus, not a history sweep. No such sweep script was found under `scripts/`.

### 3. Current burn-in data: effectively zero for both flips

- `docs/logs/shadow.jsonl` holds **15 lines, all `gate ∈ {finish, design, verify}`** — Phase 14 hollow-pass checks. **0 `tdd`/`replay` lines; 0 entropy lines** (entropy can't appear here by design). Verified by reading the file directly.
- Against the criterion **≥50 evals, <2% false-block per gate** (`scripts/shadow-stats.sh:119`, `docs/adr/0017-tdd-red-green-replay.md:40`), TDD replay sits at **0/50**.
- The log is **gitignored** — burn-in data is local and ephemeral per clone; it does not accumulate across machines or survive a fresh checkout.

### 4. The flip criterion (documented, not automated)

- TDD replay: ≥50 evaluations, <2% human-triaged false-block per gate (`docs/adr/0017-tdd-red-green-replay.md:40`; printed by `shadow-stats.sh:119`).
- Entropy: FP <1 per 10k lines on a full-history replay (`docs/adr/0018-entropy-scanning.md`; `docs/specs/2026-06-13-entropy-scanner.md:23-26`).
- There is **no automated "criteria met → flip" logic**; the decision is a human reading the scoreboard and recording the conclusion in a committed note (`shadow-stats.sh:120`).

### 5. Existing artifacts (don't rebuild these)

- Aggregator: `scripts/shadow-stats.sh` (TDD/verify/finish/design channel) — **built**.
- Entropy scoreboard: `gatekeeper/tests/cli_scan_bench.rs` — **built** (fixed corpus, not a history sweep).
- `scripts/metrics.sh` exists (tier/lead-time audit join per the Phase 16 plan) — adjacent, not the flip scoreboard. ASSUMPTION: not the burn-in tool; named for Phase 16 `metrics.sh`.
- ADRs 0017/0018, specs, and per-engine research notes all exist for the engines themselves; **none plan the burn-in harness.**

## What "burn-in harness" (workstream A) must therefore deliver

The gap is **generating and persisting evals**, not aggregating them:

1. **TDD replay evals → 0 today.** A repeatable way to exercise the replay engine over real merge-base/commit pairs so the log accrues toward ≥50, plus a decision on whether burn-in should *replay git history* to bootstrap quickly rather than wait for organic gate runs.
2. **Entropy FP figure → absent.** A full-history (or labeled-corpus) sweep that counts entropy WARN/would-block hits per 10k lines, to produce ADR-0018's number.
3. **Persistence.** `shadow.jsonl` is gitignored and ephemeral — decide whether 50 accumulated evals need a committed store or a one-shot replay that regenerates them.
4. **Unified readout + triage.** Tie both channels to their criteria so the flip decision is a single evidence-backed artifact, not two manual inspections.

## Open decisions to resolve in the design gate

- **D1 — Bootstrap by history replay vs. organic accrual.** Replaying past commits reaches ≥50 evals fast but measures the engine against *already-merged* (mostly-good) work; organic accrual is slower but reflects live false-blocks. (Affects *what* we build.)
- **D2 — Unify entropy into `shadow.jsonl` or keep the bench as its channel.** Unifying gives one scoreboard but adds an `emit_shadow` call to the scanner hot path (three-language-lanes: measurement is Rust-emit + Bash-aggregate). Keeping them split honors the existing design but means two readouts.
- **D3 — Persistence of burn-in data** (gitignored ephemeral log vs. committed corpus/result).
- **D4 — Scope of this slice:** harness that *produces and reads* the data only (flip stays deferred), per the user's choice of workstream A. The flip itself (C) and routing (B) are explicitly out of scope here.

## Scope boundary

This note covers **workstream A (burn-in measurement harness)** only. Path-triggered routing (B) and the warn→block flip (C) are separate gate cycles and out of scope.
