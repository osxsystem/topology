---
name: tdd-loop
description: Implement each unit of behavior test-first with a strict red-green-refactor cycle. Use when writing or changing any production code, implementing a planned task, or fixing a bug.
---

# TDD Loop (the tdd gate)

The iron law: **if you didn't watch the test fail, you don't know it tests the right thing.** Production code written before its test gets deleted — not adapted, not kept "as reference". Deleted.

## The cycle (per unit of behavior)

1. **RED.** Write the smallest failing test. Run it. *Watch it fail for the expected reason* (feature missing — not a typo or compile error).
2. **GREEN.** Write the minimum code to pass. Run the full test set. All green.
3. **REFACTOR.** Clean up while staying green.
4. **COMMIT.** One commit per cycle.

Repeat for the next unit. Work the tasks from the plan in order.

## Rust specifics for this repo

- Tests live in `#[cfg(test)] mod tests { ... }` next to the code.
- `cargo test` for the suite; `cargo test <name>` to watch one fail.
- Before finishing: `cargo fmt` and `cargo clippy -- -D warnings`.

## Common rationalizations (rebutted)

| Excuse | Reality |
|--------|---------|
| "Too simple to test." | Then the test is one line. Write it. |
| "I'll add tests after." | Test-after tests pass by construction; they assert nothing. |
| "I already tested it manually." | Ad-hoc ≠ repeatable. Encode it. |
| "I'll keep this code as reference." | You'll adapt it — that's testing-after. Delete it. |

## If a test won't go green

Stop guessing. Switch to `systematic-debug`.
