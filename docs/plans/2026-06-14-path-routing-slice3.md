# Plan: path-triggered routing — Slice 3 (router eval harness)

- **Date:** 2026-06-14
- **Feature slug:** path-routing
- **Design:** docs/specs/2026-06-14-path-routing.md (approved; "Router eval harness: ≥50 labeled prompts … recall ≥0.90 on `require` skills, precision ≥0.80").
- **Baseline:** main `ab7eb5a` (slices 1+2 merged); full suite 563/0 green.
- **Scope:** a deterministic eval that measures the **keyword** router against a labeled corpus, wired into the cargo suite (so it runs in `just check` → CI). No protected-surface edits (new test + fixture only).

## Honesty contract (R1 + evidence-over-assertion)

The corpus is **intent-labeled** (what a human would want routed for each prompt), not derived to trivially match the router's keywords. I will **measure recall/precision first**, then:
- If the router meets recall ≥0.90 (`require`) and precision ≥0.80 → assert those thresholds (ships as the design specifies).
- If it does not → the test asserts the **measured baseline** as a regression floor, the verify/review docs state the gap and the aspirational target, and I flag it for the maintainer (a real finding — the keyword router needs work, a Phase-17 "measurement + ratchets" item — NOT thresholds gamed to pass, NOT a silent router rewrite).

The corpus stays human-reviewable (one JSON object per line, `prompt` + `expect`).

## Files
- `gatekeeper/tests/fixtures/routing-eval.jsonl` — NEW. ≥50 lines `{"prompt": "...", "expect": ["skill", …]}` (`expect` = skills a human would want routed; `[]` for prompts that should route nothing).
- `gatekeeper/tests/cli_route_eval.rs` — NEW. Loads the corpus + the real `hooks/skill-rules.json`; for each prompt spawns `gatekeeper activate` (stdin), parses the `- <skill> [<enf>]` lines; computes recall over `require`-skill expectations and precision over all router outputs; asserts the (measured-or-target) floors.
- `docs/USER-GUIDE.md` — only if `cli_doc_sync` requires it (no new subcommand, so likely no change).

## Tasks

### Task 1: author the eval corpus
- **File:** `gatekeeper/tests/fixtures/routing-eval.jsonl`.
- **Change:** ≥50 intent-labeled lines spanning all 11 skills' domains + negatives. Each: a realistic prompt + the skills a human would expect routed. Cover: clear single-skill prompts, multi-skill prompts (e.g. "design and implement"), and no-skill prompts (small talk) for precision. Provenance comment is not possible in JSONL — document the labeling rationale in the verify doc.
- **Test:** `python3 -c "import json,sys;[json.loads(l) for l in open('gatekeeper/tests/fixtures/routing-eval.jsonl') if l.strip()]"` → exit 0 (every line valid JSON); line count ≥50.
- **Commit:** `test(routing): intent-labeled router eval corpus (>=50 prompts)`

### Task 2: eval harness measuring recall/precision (RED→GREEN)
- **File:** `gatekeeper/tests/cli_route_eval.rs`.
- **Test first:** write `router_meets_recall_precision_floor` — build a `scratch_root` carrying the repo's real `hooks/skill-rules.json`; read the corpus from `CARGO_MANIFEST_DIR/tests/fixtures/routing-eval.jsonl`; for each line, run `gatekeeper activate` with the prompt on stdin; parse routed skills + enforcement; accumulate: recall numerator/denominator over `(prompt, expected require-skill)` pairs, precision numerator/denominator over `(prompt, routed-skill)` outputs; `assert!(recall >= R)` and `assert!(precision >= P)`. Run → **RED** (test absent / harness not built).
- **Measure:** run once to print the actual recall/precision (eprintln the numbers). Set `R`/`P` per the honesty contract (≥ the design targets if met; else the measured floor, rounded down, with a `// MEASURED BASELINE — target is 0.90/0.80; gap tracked in verify doc` comment).
- **Green + commit:** `test(routing): router eval harness — recall/precision floor`.

### Task 3: full suite + lints
- **Test:** `cargo test` all green (incl. the new eval); `cargo fmt --check`; `cargo clippy --all-targets -- -D warnings`; `cargo test --test cli_doc_sync`.
- **Commit:** none unless a fixup is needed.

## After this plan
Verify (record the measured recall/precision + corpus rationale + any gap vs 0.90/0.80) → review (fresh-context) → finish → PR. If the router missed the target, the PR + a maintainer note flag it as the open ratchet item; the harness still ships as the regression guard.
