# Design: Phase 15 burn-in harness

- **Date:** 2026-06-13
- **Feature slug:** burn-in-harness
- **Status:** approved
- **Approval:** self-approved under the maintainer's 2026-06-14 overnight autonomy grant ("take all commands without my confirm; finish the phase"), after a round of maintainer code-grounded critique that corrected R1 and resolved all five ambiguities. **Honest dogfood note:** `gatekeeper check design` `approval_provenance` will FAIL here because the approving commit carries an agent co-author trailer — this is the documented residual (sycophantic self-approval risk), recorded, not concealed. Consistent with the prior Phase 15 specs (`docs/specs/2026-06-13-entropy-scanner.md`, `…-tdd-replay.md`) approved the same way.
- **Research:** [docs/research/2026-06-13-burn-in-harness.md](../research/2026-06-13-burn-in-harness.md) · ROADMAP Phase 15 (`docs/ROADMAP.md:513-545`)

## Problem

The Phase 15 warn→block flip (TDD replay → `[tdd] mode=replay`; entropy → `severity=block`) is gated on burn-in evidence: **≥50 evaluations and <2% human-triaged false-block per gate** (`docs/adr/0017-tdd-red-green-replay.md:40`), and for entropy **FP <1 per 10k lines on a full-history replay** (`docs/adr/0018-entropy-scanning.md`). Today that evidence does not exist: `docs/logs/shadow.jsonl` holds 15 lines, **0** from TDD replay and **0** from entropy (entropy never writes that log). The aggregator (`scripts/shadow-stats.sh`) is built; the missing piece is **generating the evals**.

Success: a re-runnable harness that produces both engines' false-block numbers from this repo's own history, and a committed report stating each number against its criterion — so the eventual flip (workstream C, out of scope here) is an evidence-backed decision, not a feeling.

## Constraints

- **Scope = produce + read only.** The flip itself (C) and path-triggered routing (B) are separate gate cycles. This slice does not flip any default.
- **Three-language-lanes.** The engines already exist in Rust; measurement is orchestration. This is **Bash glue invoking the existing `gatekeeper` binary** — no new enforcement logic, and critically **no edit to `scan.rs`/`tdd.rs`** (both protected paths). Markdown (the report) is the source of truth for the conclusion.
- **No new deps** (ADR-0007).
- **Reuse, don't rebuild:** `scripts/shadow-stats.sh` already aggregates the `shadow.jsonl` channel; the harness feeds it, it is not replaced.
- **Non-goals:** flipping defaults; unifying entropy into `shadow.jsonl` (decided against — keeps `scan.rs` untouched); automating the flip decision; a semantic/embedding layer.

## Approaches considered

1. **History replay, two scripts, split channels (chosen).** `burn-in-replay-tdd.sh` walks the repo's own merge commits (each merge M gives base=parent¹, feature-tip=parent², the exact `(base, feature)` pair the replay engine wants) and test-bearing commits, runs the replay engine per unit so `shadow.jsonl` accrues `tdd/replay` lines, then defers to `shadow-stats.sh`. `burn-in-entropy-sweep.sh` runs `gatekeeper scan --content` over the working-tree source files, counts entropy `WARN` lines, and divides by total lines/10k. A thin `burn-in-report.sh` (or a section appended to the committed report) prints both against their criteria. Trade-off: reaches real numbers fast from real code; old merge-bases may fail to build → `Indeterminate`/`Skip`, which **honestly do not count** toward the 50 (see Risks).

2. **Organic accrual.** Flip `[tdd] mode` config on this repo and let live gate runs log evals over future branches. Rejected in research (D1): slow (weeks), and the gitignored log doesn't survive fresh clones, so data never durably reaches 50.

3. **Unify entropy into `shadow.jsonl` + one scoreboard.** Add `emit_shadow` to the scanner so both engines share `shadow-stats.sh`. Rejected (D2): edits `scan.rs` (protected path, human-approval-gated) and adds a measurement write to the scan hot path, for a single-readout convenience the split design doesn't need.

## Decision

**Approach 1.** It is the lightest operator that produces real evidence (weakest-enforcement-that-works: Bash glue over existing Rust engines, zero protected-path edits), it measures *this repo's actual* false-block behaviour rather than a synthetic corpus, and it honours the existing two-channel design (D2). The two engines keep their native channels — TDD on `shadow.jsonl` via `shadow-stats.sh`, entropy on a dedicated source-tree sweep — joined only at the final report.

**Eval unit & false-block definition.**
- *TDD replay:* one eval = one historical `(base, feature)` pair. The feature was merged, so it is presumed-good; a `Fail` (vacuous-at-base) verdict there is a **candidate false-block** for human triage. `Skip`/`Indeterminate` (command could-not-run: non-allowlisted / spawn-fail / timeout) are **not** evals and are reported as coverage loss. A non-building base is *not* a Skip — it exits nonzero → `Pass` (vacuous-compile-red, R1).
- *Entropy:* one "eval" = the full source sweep; the figure is WARN-hits per 10k scanned lines, reported as candidate FPs for triage (entropy cannot distinguish benign high-entropy blobs from secrets — that's why it ships `warn`).

**Persistence (D3):** the scripts are the source of truth and regenerate the data on demand. The committed artifact is the **burn-in report** at `docs/burn-in/2026-06-14-burn-in-report.md` (distinct from this feature's verify-gate doc), not the raw log.

## Mechanism & resolved decisions (verified against source 2026-06-14)

The replay engine (`gatekeeper/src/tdd.rs`) was read end-to-end to pin the mechanism; the following are confirmed, not assumed:

- **Replaying a historical pair.** For each merge commit M with parents `M^1` (base) and `M^2` (feature tip), the harness does `git worktree add --detach <wt> <M^2>` and runs `gatekeeper check tdd --feature <short-sha> --base <M^1>` with `cwd=<wt>`. The engine resolves `merge-base(--base, HEAD)` and replays the first test-only commit's test files at that base in its own nested worktree (`tdd.rs:330-588`). Synthetic `--feature` slug per merge (short SHA) — `gate_tdd` rejects an empty feature (`tdd.rs:339-342`).
- **D1-config — ephemeral, not committed.** Without a resolvable test command, history mode returns 0 and emits **no shadow line** (`tdd.rs:467-475`). So the harness writes an **untracked `<wt>/docs/config.toml`** carrying `test_command = "cargo test --manifest-path gatekeeper/Cargo.toml --quiet"` plus the repo's existing `allowed_command_prefixes` (a non-allowlisted command → `Indeterminate`, not an eval — `tdd.rs:307-310`). It dies with the worktree; the main repo diff stays scripts + docs only. A committed config that enabled replay-shadow-logging on every live `check tdd` would be a behaviour change toward the flip — out of scope.
- **D2/D3 — eval source is merge pairs only.** Merges with no test-only commit (docs/release merges) hit `return 0` with no shadow (`tdd.rs:478-483`); lone non-merge commits don't present a `(base, feature-range)` with a test-only commit. So the 49 merges are the sole eval source; realistic clean yield is ~30–40, **likely below 50**. The report states `N/50 — criterion NOT met` and never pads with Skips. The flip stays deferred regardless.
- **Dedicated log + idempotency.** The harness writes replayed evals to a **dedicated, gitignored `docs/logs/burn-in-tdd.jsonl`** (artifacts root resolves to `<wt>/docs` under the scratch cwd; the harness concatenates the worktree's emitted lines into this file), **truncated at the start of each run** so re-running never double-counts. The readout is `scripts/shadow-stats.sh docs/logs/burn-in-tdd.jsonl` (it takes a path arg — `shadow-stats.sh:21` — so it is invoked, never modified). This keeps replayed-historical burn-in separate from the organic live `shadow.jsonl`.

## Risks & open questions

- **R1 — eval yield, not build failure, is the limiter (corrected 2026-06-14).** My earlier framing was wrong: a non-building old base exits nonzero → counted as **Pass** ("genuine red"), not `Skip` (`tdd.rs:312-325`); `Indeterminate`/`Skip` only fires when the command *can't run*. So build failure inflates Pass with **vacuous-compile-reds** (the ADR-0017 soft spot) rather than shrinking the count. The real limiter is merges with no test-only commit (`return 0`, no eval). Net: clean yield ~30–40, likely <50. Mitigation: report `N/50 — criterion NOT met` honestly, and **flag the vacuous-compile-red share** so Passes aren't read as all-clean genuine reds. Harmless for *false-block* measurement (we count Fails), but stated, not hidden.
- **R2 — replay cost & disk.** Each eval is a full `cargo test` compile in a fresh worktree (~30 s–2 min + a `target/` per worktree). ~40 evals ≈ tens of minutes and significant disk. Mitigation: `--limit` defaults to a small recent window; widening is documented. `trap` cleanup + `git worktree prune` removes both the scratch worktree and the engine's nested worktree on interrupt (mirrors the engine's RAII guard).
- **R3 — `--content` carries no path**, so entropy `exclude_paths` (path-bearing lanes only) won't suppress lockfiles/SVGs in the sweep. Mitigation: the sweep iterates real files and **skips `exclude_paths` globs itself** before feeding `--content`, so the FP number matches what `--staged`/`--hook` would warn on.
- **R4 — `--content` 5 MiB cap (`scan.rs` `HOOK_INPUT_CAP`).** A source file >5 MiB makes `scan --content` emit `BLOCK oversize-input` and exit 1. The sweep treats that as **skip-with-warning**, not a blocking error, and does not let exit 1 abort the loop.
- **R5 — `set -euo pipefail` vs. expected non-zero exits.** `check tdd` legitimately returns 1/2 and `scan --content` returns 1 on oversize/block; both loops **capture exit codes without aborting** (`rc=0; cmd || rc=$?` pattern), never `&&`-chaining through a gate call.
- **Open:** whether the entropy sweep should walk full git *history* blobs (ADR-0018's literal "full-history") or the current tree. Default to current-tree for this slice (cheaper, representative); history-blob sweep is a follow-up if the tree number is borderline.

## Acceptance criteria

- `scripts/burn-in-replay-tdd.sh` exists, is `shellcheck`-clean, replays a bounded window (`--limit`, recent merges by default) of historical merge pairs via a detached scratch worktree + ephemeral `<wt>/docs/config.toml`, writing `tdd/replay` lines to a dedicated gitignored `docs/logs/burn-in-tdd.jsonl` **truncated at the start of each run** (idempotent re-run), then runs `scripts/shadow-stats.sh docs/logs/burn-in-tdd.jsonl`.
- A `trap` removes the scratch worktree and runs `git worktree prune` on every exit/interrupt; no worktree leak after Ctrl-C.
- Gate exit codes (`check tdd` 1/2; `scan --content` 1) are captured without aborting under `set -euo pipefail`.
- `scripts/burn-in-entropy-sweep.sh` exists, is `shellcheck`-clean, sweeps working-tree source (skipping `exclude_paths` globs itself), treats a >5 MiB file as skip-with-warning (not a loop-aborting error), and prints entropy WARN-hits and a per-10k-line rate.
- Each script, on empty/zero-data input (no merges in window, or no source files after exclusion), prints an informational "0 evaluations" line and exits 0 — never errors, never blocks.
- A committed burn-in **report** at `docs/burn-in/2026-06-14-burn-in-report.md` records, per engine: clean-eval count, Skip/coverage-loss count, **vacuous-compile-red share** (R1), candidate-false-block list for triage, and the number stated against its criterion (TDD: `N/50`, `<2%?`; entropy: `FP per 10k`). It explicitly does **not** flip any default.
- No edits to `scan.rs`, `tdd.rs`, `verify.rs`, `rules.toml`, `shadow-stats.sh`, or any protected path; `git diff --name-only` against the base is `scripts/` + `docs/` only.
- `just shell` (shellcheck) clean; existing `cargo test` suite stays green; no new deps.
