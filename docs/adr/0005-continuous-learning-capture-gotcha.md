# 0005 — Continuous learning via capture-gotcha + approved promotion

- **Status:** Accepted
- **Date:** 2026-06-04

Lessons from failures don't survive across sessions, so the same mistakes recur. We will **capture**
each gate failure or human correction to `docs/learn/` via `gatekeeper learn`, and **promote**
recurring gotchas into a new instinct, skill, or `security/rules.toml` rule — with a **human approving
every promotion**.

## Why

This closes the loop: a failure becomes a permanent operator exactly where the system got burned. The
human approval step is deliberate — fully automatic promotion would let noise and one-off mistakes
harden into standing policy.

## Consequences

- Depends on instincts (ADR [0004](0004-instincts-vs-gates.md)) and scanning
  (ADR [0002](0002-security-scanning-in-rust.md)) existing as promotion targets — hence their earlier
  roadmap position.
- The ledger is append-only Markdown; promotion is an explicit, reviewed action, never silent.
