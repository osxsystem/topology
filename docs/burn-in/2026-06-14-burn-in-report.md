# Phase 15 burn-in report — TDD replay & entropy scanner

- **Date:** 2026-06-14
- **Feature slug:** burn-in-harness
- **Design:** [docs/specs/2026-06-13-burn-in-harness.md](../specs/2026-06-13-burn-in-harness.md) · **Plan:** [docs/plans/2026-06-14-burn-in-harness.md](../plans/2026-06-14-burn-in-harness.md)
- **Scope:** measurement only. **This report flips no defaults.** It records the burn-in numbers the warn→block flip (workstream C) is gated on, per the shadow-then-enforce doctrine.

## Bottom line

**Neither engine is flip-ready. Both flips stay deferred.** The harness did its job: it produced the gating evidence, and the evidence says "not yet" — emphatically.

| Engine | Criterion | Measured | Verdict |
|--------|-----------|----------|---------|
| TDD red-green replay | ≥50 evals **and** <2% false-block (ADR-0017:40) | **8 evals**, **62.5%** would-block | ❌ both unmet (6× too few evals; ~31× the FP bar) |
| Entropy scanner | FP <1 per 10k lines (ADR-0018) | **20.23 WARN per 10k lines** | ❌ ~20× over |

## TDD red-green replay

**Command:** `bash scripts/burn-in-replay-tdd.sh 49` (env: `GATEKEEPER_BIN`/`TOPOLOGY_ROOT` unset).

```
replayed 49 merge(s); 8 produced a replay verdict
gate  check   evals  pass  fail  skip  static  would-block%
tdd   replay      8     3     5     0       0         62.5%
```

- **49 merge commits replayed; only 8 produced a verdict.** The other 41 had no test-only commit in range, so the engine returns before replay (`tdd.rs:478-483`) — no eval. This is the dominant yield limiter (design R1): **this repo's own history cannot reach the 50-eval bar** — all of it yields 8.
- **3 pass** (`red at merge-base` — genuine), **5 would-block** (`vacuous test: passed at merge-base`).

### Would-block triage (the 5 fails, by source merge)

The injected `merge` field ties each verdict to its PR:

| Merge | PR | Subject | Read |
|-------|----|---------|------|
| `8e19256` | #61 | auto-adapt-on-setup | merged feature → **false-block** |
| `9a84684` | #44 | contract-split | merged feature → **false-block** |
| `a61b0a8` | #43 | installer-v3 | merged feature → **false-block** |
| `87301c0` | #42 | root-resolution-hardening | merged feature → **false-block** |
| `fff8bc3` | #40 | hollow-pass-kills | merged feature → **false-block** |

All five are legitimate features that passed every gate and shipped. The `vacuous at base` verdict fires because replay runs the **whole suite** at the merge-base after checking out the first test-only commit's files; for these PRs the added tests did not fail in isolation at base (structural / bash / docs-heavy changes, or test-and-code split such that the picked test-only commit isn't red against the base). Treating them as **false-blocks** is the conservative, correct read for a flip decision: in `replay` (enforce) mode these merges would have been blocked, and they should not have been.

→ **False-block rate 5/8 = 62.5%**, ~31× the <2% threshold. Even were several reclassified as true catches, the **8 evals « 50** floor alone bars the flip.

## Entropy scanner

**Command:** `bash scripts/burn-in-entropy-sweep.sh` (env unset).

```
files scanned:        256
excluded (path glob): 8
skipped (oversize):   0
total lines:          52887
entropy WARN hits:    107
WARN per 10k lines:   20.23
```

- **107 entropy WARNs across the working tree → 20.23 per 10k lines**, ~20× the `<1` criterion.
- These are dominated by **benign high-entropy tokens**: 40-hex git commit SHAs quoted throughout `docs/` (ADRs, specs, this very report), checksums, and base64 test vectors. Entropy genuinely cannot distinguish them from secrets (ADR-0018 "fundamental limit") — which is exactly why entropy ships `severity = "warn"`.
- Flipping entropy to `block` today would wrongly block 107 commits' worth of content on the current tree alone. **Not flip-ready.**

## Conclusions & follow-ups

1. **Both flips remain deferred.** No `[tdd] mode` change; no `rules.toml` `severity` change. Confirmed: `git diff` on this branch touches only `scripts/`, `docs/`, `justfile`, `CHANGELOG.md`.
2. **The TDD flip is blocked structurally, not just statistically.** This framework's git history is too small (and too doc/bash-heavy) to ever reach 50 replay evals. The harness should be run against **accumulated future history**, or on a **larger downstream governed repo**, before the criterion is reachable. Recorded as the honest limit, not engineered around.
3. **The 62.5% would-block rate is a real signal**, not noise: the whole-suite-at-base replay over-fires on legitimate features whose tests aren't red-in-isolation. ADR-0017 already documents the inverse soft spot (compile-error-red); this run surfaces the over-block direction. Candidate Phase 17 work: isolate the *new test* rather than running the whole suite at base.
4. **Entropy needs path/context narrowing before any block promotion** (e.g. excluding `docs/` SHAs, or a labeled-context gate), or it stays `warn` permanently — an acceptable terminal state.

This is the shadow-then-enforce doctrine working as designed: the gate hardened (the engines exist and log), but the default does not flip until measured evidence clears the bar. Tonight the evidence says it does not.
