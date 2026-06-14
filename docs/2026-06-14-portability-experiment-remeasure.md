# Portability-first experiment — re-measure & verdict

- **Date:** 2026-06-14
- **Closes:** the portability-first falsifying experiment (slices #1–#4)
- **Question it answers:** a developer field-tested Topology on a Swift/SwiftUI to-do app and concluded
  *"Topology is not worth it."* The maintainer's first instinct was to make the gates advisory (split
  each gate into an optional `/command`). Before changing the gate model, we ran a falsifying experiment:
  ship hardened-portability + bug-fix slices, then **re-measure** whether the ceremony is still "not
  worth it."

## Verdict: **REFINED** — not upheld, not falsified

The "not worth it" verdict is **correct for the regime it was uttered in** (a tiny Swift to-do app —
trivial, low-blast-radius, deadline work) and the experiment does **not** refute that. What the
experiment **falsifies is the generalization** — that the ceremony is not worth it *as such*. On
security- and correctness-critical core work the gates demonstrably paid for themselves, repeatedly.

Three independent analysts (cost-accounting, value-evidence, skeptic) **each independently** put the
**methodology share of the original verdict at ~35%** — closely confirming the pre-experiment hypothesis
of 25–30%. The other ~65% was portability/packaging, which the experiment **demonstrably fixed** (three
real taxes removed). All three concluded **the gates should NOT be made advisory.**

## The objective ledger

Shipped across the 3 built slices: **~788 LOC of production code** against **~1,474 LOC across 14 gate
artifacts** — an artifact-to-code ratio of **~1.87:1**. That ratio *is* the field dev's complaint,
quantified. But it is not uniform, and the non-uniformity is the whole story:

| Slice | Prod LOC | Gate cost | What the gates caught |
|---|---|---|---|
| #1 scan tokenizer (PROTECTED core, security floor) | ~510 | 5 artifacts + **8 review rounds** | **~14 real security bypasses** that a green 45-test suite *and* hand-analysis both missed; rounds 1–7 each failed and drove out a distinct one |
| #2 trailer-collision doctor probe (advisory) | ~175 | 5 artifacts + 2 review rounds (≈2.2:1) | a real verify-gate bug (empty-output mis-report) + a review fidelity divergence |
| #3 replay-allowlist fix (config, non-core) | ~103 | 5 artifacts + 1 review round (**4.5:1**) | research **falsified 3 of 4 pitched fields** (negative LOC — pure win); TDD caught an **FM2 soundness interaction** the design missed |
| #4 per-language recognizers | **0** | 1 research doc | research **deferred it** (opt-in-only, escape hatch exists, protected-core cost, enables an already-deferred flip) |

**The pattern:** gate value tracks **blast radius**, gate cost was **fixed**. Slice #1 (protected core,
security floor) earned every one of its 8 rounds. Slice #3 (~103 LOC, non-protected) did **not** earn 5
separate documents — yet paid the same fixed tax (4.5:1). That mismatch — *fixed-cost ceremony on
variable-value work* — is the legitimate residual the field dev felt (his "C3" cost-benefit objection),
and it is **not eliminated** by the portability fixes.

## Why "advisory gates" is the wrong fix (rejected by the evidence)

The strongest single datum: on Slice #1, a green 45-test suite passed while **seven distinct real
security false-negatives existed** — commands that write protected files slipping past the veto. Only the
**adversarial, fresh-context, multi-round REVIEW gate** drove them out. A lighter "write code, run tests,
ship" process would have shipped every one. Making the review gate optional would forfeit exactly that.
Likewise the RESEARCH gate's highest-ROI move was *not building* (Slice #3's 3 falsified fields, Slice
#4's deferral) — an advisory research step gets skipped under deadline pressure, which is precisely when
that filter is most valuable. **The gates catch things; optional gates catch nothing.**

## Recommendation: proportional ceremony keyed to blast radius

The fix is to make ceremony **a function of blast radius**, which gatekeeper *already computes*
(`[integrity].protected_paths` already classifies `scan.rs`/`main.rs` as core vs `config.rs`/`doctor.rs`
as not). Concretely:

1. **Express lane for low-blast-radius work.** For a change that touches no protected path, no security
   rule, and is under a LOC threshold (e.g. <50–100), collapse research + design + plan into **one short
   combined artifact**; keep **verify** and **review** (1 round). This directly answers "a <100-LOC fix
   shouldn't carry 400+ lines of process doc."
2. **Full sequence stays mandatory for high-blast-radius work** — protected core, the security floor,
   or large diffs. Slice #1 is the proof this lane is worth it.
3. **Keep REVIEW (fresh-context, adversarial) un-skippable** for any change to the security floor /
   enforcement core — clearest proven ROI in the experiment.
4. **Keep RESEARCH's falsify-the-pitch power explicit and cheap** — it is the highest-leverage, lowest-
   cost gate; a one-paragraph "too small / don't build, ship via express lane" is a valid gate output.
5. **Do NOT make any gate advisory.** Scale weight, not optionality.
6. **Treat the remaining portability taxes as packaging bugs** (the Slice #3 pattern: every fail-closed
   allowlist/recognizer should auto-include the operator's own configured command). This is the ~65%
   root cause the original verdict half-detected.
7. **Track the artifact-to-code ratio** as a metric; re-measure on the next small slice once the express
   lane lands — the dev's objection is only *answered* when a <100-LOC fix ships without 400+ lines of
   process.

## Bottom line for the maintainer

- The field report was **substantially a portability/packaging failure** (~65%), now demonstrably
  reducible — three real monoculture/correctness taxes fixed (scanner false-positive, trailer collision,
  replay allowlist).
- The methodology's value is **real and load-bearing** on the work that matters; the gates repeatedly
  caught defects nothing else did.
- The legitimate residual is **proportionality, not validity**. Build the risk-tiered/express lane keyed
  to the protected-path classification you already have. **Do not make the gates advisory** — that was
  the one move the evidence rules out.
