# Verify — path-triggered routing Slice 3 (router eval harness)

- **Date:** 2026-06-14 · **Feature slug:** path-routing
- **Design:** [docs/specs/2026-06-14-path-routing.md](../specs/2026-06-14-path-routing.md) · **Plan:** [docs/plans/2026-06-14-path-routing-slice3.md](../plans/2026-06-14-path-routing-slice3.md)

Scope: Slice 3 — a deterministic eval that measures the keyword router against an intent-labeled corpus, with CI-gating recall/precision floors. Completes workstream B.

## The measured result (the headline)

`cargo test --test cli_route_eval -- --nocapture`:
```
router eval: recall=0.956 (43/45 require-skill expectations), precision=0.921 (58/63 routed outputs); floors recall>=0.9 precision>=0.8 (target 0.90/0.80)
test result: ok. 1 passed
```

**The shipped router clears both ROADMAP targets with margin** — recall **0.956 ≥ 0.90**, precision **0.921 ≥ 0.80**. The honesty contract's "if met → assert the design targets" branch applies: the test asserts the real 0.90/0.80 floors (not a lowered baseline), and the router passes them honestly.

## Corpus rationale (R1 mitigation)

`gatekeeper/tests/fixtures/routing-eval.jsonl` — 55 prompts, **intent-labeled** (the skill(s) a disciplined Topology agent should run for that prompt), not reverse-derived from the router's keywords. Coverage: clear single-skill prompts across all 10 routed skills, multi-skill prompts (e.g. "Plan and then implement…" → write-plan + tdd-loop + brainstorm-design), and 6 no-route negatives (small talk) that exercise precision. One object per line, fully human-reviewable so labels can be adjusted later.

- **Recall** is computed over `(prompt, expected *require*-skill)` pairs (per the ROADMAP "recall on require skills"); **precision** over all `(prompt, routed-skill)` outputs (penalizing over-firing). The require/suggest split is read from the shipped `skill-rules.json`.
- **Known misses (within tolerance):** 2/45 require-skill expectations unrouted and 5/63 routed outputs unexpected — driven by the keyword router's shared keywords (e.g. "implement" routes both brainstorm-design and tdd-loop). These are inherent to a deterministic keyword backstop and stay above the floors; sharpening them is a Phase-17 measurement/ratchet item, not a blocker.

## Acceptance criteria, demonstrated

- **≥50 labeled prompts.** 55 valid JSONL lines (validated with `python3 -c "json.loads"`). ✔
- **Eval measures the LIVE shipped router.** The harness copies the repo's real `hooks/skill-rules.json` into a scratch root and runs the compiled `gatekeeper activate` per prompt. ✔
- **recall ≥0.90 (require), precision ≥0.80.** Measured 0.956 / 0.921; asserted as the floors. ✔
- **Wired into CI.** It is a `cargo` test → runs under `just check` → the Offline gate. No `.github` edit needed. ✔
- **Deterministic backstop; no new deps; no semantic layer.** Reuses serde; spawns the existing binary. ✔
- **Full suite + lints.** `cargo test` → 564 passed, 0 failed; `cargo fmt --check` clean; `cargo clippy --all-targets -- -D warnings` clean; `cli_doc_sync` green (no new subcommand). ✔

## Note

Slice 3 touches no protected path (new test + fixture only) — committed normally, no `--no-verify`. Workstream B (path-triggered routing + router eval) is now complete across slices 1-3.
