---
name: systematic-debug
description: Diagnose a failure by reproduction and hypothesis-testing instead of guessing. Use when a test won't pass, behavior diverges from expectation, something works in one environment but not another, or after an error or stack trace appears.
---

# Systematic Debug

Guessing-and-poking is not debugging. Work the four phases in order.

## Phase 1 — Reproduce
Get a reliable, minimal reproduction. If you can't reproduce it, you can't fix it — find the trigger first. Capture the exact command, input, and observed vs. expected output.

## Phase 2 — Isolate
Shrink the reproduction until it's minimal. Bisect: git history, inputs, config. Narrow to the smallest failing case.

## Phase 3 — Hypothesize & test
Form ONE falsifiable hypothesis about the cause. Predict what you'd see if it's true. Run the cheapest experiment that would refute it. Refuted → next hypothesis. Don't change two things at once.

## Phase 4 — Fix & prove
Fix the root cause, not the symptom. Add a regression test that fails before the fix and passes after (hand back to `tdd-loop`). Then go to `verify-before-done`.

## Anti-patterns

- Changing code "to see if it helps" without a hypothesis.
- Fixing the symptom (swallowing the error) instead of the cause.
- Declaring it fixed because the error "went away" without understanding why.
