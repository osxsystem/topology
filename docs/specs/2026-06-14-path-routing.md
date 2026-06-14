# Design: path-triggered routing + router eval harness (Phase 15 workstream B)

- **Date:** 2026-06-14
- **Feature slug:** path-routing
- **Status:** draft
- **Research:** [docs/research/2026-06-14-path-routing.md](../research/2026-06-14-path-routing.md) · ROADMAP Phase 15 (`docs/ROADMAP.md:537-545`)
- **Approval:** **NOT self-approved.** Unlike workstream A (additive, zero protected edits), B edits the protected enforcement surface (`main.rs`, `.claude/settings.json`, a new always-on PostToolUse hook). I am holding this at the design gate for the maintainer's explicit ratification before any protected-path implementation.

## Problem

Security/process routing today keys on the *prompt's keywords* (`route()`, main.rs:657-685). An agent that edits `hooks/**` or `security/**` without saying so in its prompt gets no skill reminder. Path-triggered routing makes the trigger *what the diff touches*: edit a security-sensitive path → the `security-scanning` skill is surfaced as required, regardless of phrasing. Plus a **router eval harness** to hold both routers to a measurable bar (recall ≥0.90 on `require`, precision ≥0.80).

## Constraints / non-goals

- **Three-language-lanes.** Matching/decision logic in Rust; the hook is thin Bash glue; skill-rules.json is data. No logic in Bash.
- **Advisory, not a veto (D2).** The PostToolUse hook *injects context* ("you touched `hooks/**`; `security-scanning` is required") — it never blocks the tool call. Routing is a reminder; blocking is the scan/gate layer's job (weakest-enforcement-that-works).
- **Minimize protected-surface edits.** New logic lives in an *unprotected* new module; `main.rs` gets only a dispatch row + handler.
- **No new deps** (ADR-0007). Reuse the dep-free glob approach.
- **Non-goals:** a semantic/embedding router (explicitly rejected — offline-first); changing the existing keyword router's behavior; making routing block anything.

## Approaches considered

1. **New `gatekeeper/src/route.rs` module (chosen).** Put `route_by_paths()` + a small dep-free glob matcher in a *new, unprotected* `route.rs`; `main.rs` gets one `SubcommandSpec` row (`route`) + a thin `cmd_route` handler + a `mod route;`. A new `hooks/post-tool-routing.sh` calls `gatekeeper route --paths <touched>` and prints required-skill context; wired as `PostToolUse` (matcher `Write|Edit|MultiEdit`) in `settings.json`. Trade-off: ~25 lines of glob logic duplicated from `scan.rs` (with a parity unit test), but it keeps the new code out of the protected scanner and confines `main.rs` edits to the dispatch row.
2. **Make `scan::glob_match` `pub(crate)` and reuse.** DRY, but edits the protected security scanner (`scan.rs`) for a non-scan reason — higher scrutiny on the most safety-sensitive file. Rejected: the coupling and protected-file churn outweigh ~25 saved lines.
3. **Fold path routing into the existing keyword `route()` in main.rs.** Rejected: bloats a protected file and mixes prompt- and path-matching in one function; the new module is cleaner and less protected-surface.

## Decision

**Approach 1.** Confines protected edits to: a `SubcommandSpec` row + `cmd_route` + `mod route;` in `main.rs`, and the `PostToolUse` block in `settings.json` — both committed with a documented `--no-verify` override under the autonomy grant. Everything substantive (path matching, eval scoring) lives in the unprotected `route.rs` / tests / fixtures / new hook script / `skill-rules.json`.

**Resolved decisions:**
- **D1:** new `route.rs` with its own small glob matcher + a unit test asserting parity with the scanner's documented semantics (trailing-`/` prefix; `*` wildcard).
- **D2:** PostToolUse hook is **advisory** — exit 0 always, prints context to stderr/stdout; never blocks.
- **D3:** `tests/fixtures/routing-eval.jsonl` ≥50 labeled cases (prompt- and path-routing); `cli_route_eval.rs` computes recall/precision and asserts recall ≥0.90 (`require`), precision ≥0.80; wired into `just check`/CI.
- **D4:** new `hooks/post-tool-routing.sh` (not itself protected) + `PostToolUse` wiring in `settings.json` (protected → override).

## Proposed tracer-bullet sequencing (for the plan gate)

1. **Slice 1 — core (testable in isolation):** `pathTriggers` schema in `skill-rules.json` + `route.rs` (`route_by_paths` + glob + parity test) + `route` subcommand in `main.rs` + `cli_route.rs` tests. No hook yet. Delivers `gatekeeper route --paths <p>`.
2. **Slice 2 — wiring:** `hooks/post-tool-routing.sh` + `PostToolUse` in `settings.json`; an integration test that touching a trigger path surfaces the skill.
3. **Slice 3 — eval harness:** `routing-eval.jsonl` (≥50) + `cli_route_eval.rs` recall/precision thresholds + CI.

Each slice is its own TDD cycle; slices 1–2 touch protected paths (override-documented), slice 3 is additive.

## Risks & open questions

- **R1 — eval corpus authoring is judgment-heavy.** ≥50 labeled prompts with correct expected-skill labels is the largest effort and the most subjective. Mislabeled cases would make the thresholds meaningless. Mitigation: derive labels from the *current* skill-rules keywords + the path globs (self-consistent), and keep the corpus reviewable.
- **R2 — PostToolUse adds latency to every Write/Edit.** Must be fast (one `gatekeeper route` call, fail-open). Mirrors the existing PreToolUse budget.
- **R3 — glob duplication drift.** The `route.rs` matcher could diverge from `scan.rs`. Mitigation: the parity unit test pins shared cases; a comment cross-references `scan.rs:498-527`.
- **Open:** should slice 3 (eval harness) ship in this PR or as a follow-up? It is the bulk of the work and is separable from the path-routing capability itself.

## Acceptance criteria

- `hooks/skill-rules.json` gains optional `pathTriggers: { globs: [...] }`; existing keyword routing unchanged (back-compat test).
- `gatekeeper route --paths <p1> [<p2>…]` and `--staged-paths` print routed skills (same `- <skill> [require|suggest]` grammar as `activate`); unknown flags exit 2; `--help` exits 0.
- A new unprotected `gatekeeper/src/route.rs` holds `route_by_paths`; its glob matcher has a parity unit test vs the documented `scan.rs` semantics.
- `hooks/post-tool-routing.sh` is advisory (exit 0 always), prints required-skill context when touched paths match a `pathTriggers` glob, fails open if the binary is absent; wired as `PostToolUse` (`Write|Edit|MultiEdit`) in `.claude/settings.json`.
- Router eval: `tests/fixtures/routing-eval.jsonl` ≥50 labeled cases; `cli_route_eval.rs` asserts recall ≥0.90 (`require`) and precision ≥0.80; runs in `just check`.
- `cli_doc_sync.rs` stays green (the new subcommand documented); full `cargo test` green; `shellcheck` clean; no new deps.
- Protected-path edits limited to the `main.rs` dispatch row/handler/`mod` and the `settings.json` PostToolUse block, each committed with a documented `--no-verify` override.
