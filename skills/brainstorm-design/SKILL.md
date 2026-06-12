---
name: brainstorm-design
description: Turn a rough idea into an approved written design before any code is written; spec artifact lands under the artifacts root (`docs/` here, `.claude/topology/` in a governed project). Use when starting a new feature, when the user describes something they want built, or when requirements are vague and need clarifying.
---

# Brainstorm & Design (the design gate)

No production code until a design doc exists at `<artifacts-root>/specs/<date>-<feature>.md` and the user has approved it (artifacts root: `docs/` in the framework repo, `.claude/topology/` in governed projects — gate FAIL messages print the resolved path).

## Process

1. **Explore context.** Read the relevant code and docs first. Don't ask what you can find out.
2. **Clarify, one question at a time.** Ask Socratic questions until the goal, constraints, and success criteria are unambiguous. One question per turn — don't dump a questionnaire.
3. **Propose 2–3 approaches.** For each: a one-paragraph sketch, plus trade-offs (complexity, risk, reversibility). Recommend one and say why.
4. **Write the design doc.** Use `references/design-doc-template.md`. Save to `<artifacts-root>/specs/<YYYY-MM-DD>-<feature-slug>.md`.
5. **Get explicit approval.** Present the written doc. Iterate until the user approves.
6. **Commit the doc** before moving on.

## Gate check

```bash
gatekeeper check design --feature <feature-slug>
```

Passes only when the design doc exists **and** contains an explicit approval marker. The gate
teaches the fix on failure; accepted forms (anywhere in the file, case-insensitive):

```
Status: approved
**Status:** approved
- **Status:** approved
```

Add the marker once the human has reviewed and signed off the design. When the gate passes,
transition to `write-plan`.

## Common rationalizations (rebutted)

| Excuse | Reality |
|--------|---------|
| "Too simple to need a design." | Trivial designs are one paragraph. Write it anyway — the gate is cheap. |
| "I'll design as I code." | That's how scope creep and rework happen. The doc is the contract. |
| "The user is in a hurry." | A 5-minute design doc saves the hour spent building the wrong thing. |
