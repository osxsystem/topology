---
name: verify-before-done
description: Prove a task is actually complete with evidence before claiming done. Use when you believe a feature or fix is finished, before saying "done" or "fixed", and before moving to finish a branch.
---

# Verify Before Done (the verify gate)

"Done" is a claim that requires proof. Never assert completion from a feeling.

## Process

1. **Restate the acceptance criteria** from the design doc.
2. **Demonstrate each one** with a concrete run: command + actual output. For a bug, reproduce the original symptom, then show it resolved on the same input.
3. **Run the full suite**, not just the test you touched.
4. **Record evidence** at `docs/verify/<YYYY-MM-DD>-<feature-slug>.md`: what you ran, what you saw, and which acceptance criterion each item satisfies.

## Gate check

```bash
gatekeeper check verify --feature <feature-slug>
```

Passes when the verification note exists. Then transition to `finish-branch`.

## The bar

If someone asked "how do you know?", your answer is a command they can re-run and an output they can see — not "I'm confident."
