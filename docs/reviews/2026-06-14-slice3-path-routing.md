VERDICT: pass
HEAD: 19b04d8eeee3e34e05ae0a27e4679310db23f166
BASE: ab7eb5a7d135102e49f9993306690e1bd8412a32

# Review: path-routing Slice 3 — router eval harness (2026-06-14)

A fresh-context critic (no memory of authoring) reviewed the diff and **independently reproduced** the metrics in Python (recall 43/45 = 0.956, precision 58/63 = 0.921), confirming the eval is honest — floors are the real ROADMAP targets (0.90/0.80) set with genuine margin below the measured values, labels are intent-based (router≠label cases exist), the 6 negatives are keyword-free small talk, and no padding. Verdict pass, no blocking findings. The critic's primary nit (precision could false-pass via `0/0 → 1.0` if the router/parser broke) was acted on: the harness now asserts `recall_total > 0` and `prec_total > 0`, failing loudly instead, plus a parser-invariant comment.

## Blocking findings
None.

## Non-blocking notes
- `gatekeeper/tests/cli_route_eval.rs` precision/recall `0/0` false-pass surface — **resolved** in commit `19b04d8` (explicit `> 0` asserts; the recall branch was already an implicit canary).
- `gatekeeper/tests/cli_route_eval.rs` `routed_skills` parses by `- ` line-prefix; robust because `activate` does not echo the prompt — now documented; the `prec_total > 0` guard fails loudly if that ever changes.
- `gatekeeper/tests/cli_route_eval.rs` scratch-root cleanup runs only on the success path; a mid-loop panic leaks a `/tmp/topo_route_eval_<pid>` dir. Cosmetic.

## Criteria checked
### Spec/plan
- ≥50 labeled prompts — satisfied (55 valid JSONL lines; `assert!(cases.len() >= 50)`; critic parsed all 55).
- recall ≥0.90 on require skills — satisfied (`RECALL_FLOOR=0.90`; independently measured 43/45 = 0.956; denominator correctly restricted to `require`-enforcement skills read from the live rules).
- precision ≥0.80 — satisfied (`PRECISION_FLOOR=0.80`; independently measured 58/63 = 0.921 over all routed outputs).
- Measures the LIVE shipped router — satisfied (copies the real `hooks/skill-rules.json`; spawns the compiled `gatekeeper activate`; parses real output, not a hardcoded expectation).
- Wired into CI — satisfied (plain `cargo` test, no `#[ignore]`/feature gate → runs under `just check` → Offline gate).

### Standards
- no-deps (ADR-0007) — satisfied (`Cargo.toml`/`lock` untouched; serde/serde_json pre-existing).
- surgical / no protected-path edits — satisfied (diff is the new test + new fixture + plan + verify; `skill-rules.json` read-only at runtime, not modified).
- honest-eval (not gamed) — satisfied (floors = spec targets with margin; misses disclosed in the verify doc; intent-based labels; negatives unpadded). Critic re-derived the numbers independently.
- metrics-correct — satisfied (critic re-implemented recall/precision and matched the exact counts; recall over require-skill expectations, precision over all routed outputs).
