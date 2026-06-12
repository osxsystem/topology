---
name: write-plan
description: Decompose an approved design into a placeholder-free, step-by-step implementation plan; artifact lands under the artifacts root (`docs/` here, `.claude/topology/` in a governed project). Use after a design is approved and before writing code, or when the user asks for a plan, breakdown, or task list.
---

# Write Plan (the plan gate)

Turn the approved design into a plan at `<artifacts-root>/plans/<date>-<feature>.md` (artifacts root: `docs/` in the framework repo, `.claude/topology/` in governed projects — gate FAIL messages print the resolved path). No code until the plan gate passes.

## Process

1. **Map the work.** List the files to touch and each file's responsibility.
2. **Decompose into 2–5 minute tasks.** Each task is independently checkable.
3. **Each task must be complete — no placeholders.** Forbidden: "TBD", "implement later", "similar to task N", "add appropriate validation". Every task states:
   - exact file path(s)
   - the complete change (or complete code) to make
   - the test command and its expected output
   - the commit message
4. **Confirm a clean baseline.** Tests pass before you start.
5. **Save** to `<artifacts-root>/plans/<YYYY-MM-DD>-<feature-slug>.md` using `references/plan-template.md`, and commit it.

## Gate check

```bash
gatekeeper check plan --feature <feature-slug>
```

This fails if the plan file is missing **or** contains placeholder tokens. Fix the plan until it passes, then transition to `tdd-loop`.

## Why no placeholders

A placeholder is a deferred decision. Deferred decisions are where agents hallucinate and drift. If you can't write the concrete step now, you don't understand it yet — go back to design.
