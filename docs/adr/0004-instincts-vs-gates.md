# 0004 — Instincts are a distinct operator class from gates

- **Status:** Accepted
- **Date:** 2026-06-04

Some guidance should always be present and cheap (framing the agent's reasoning); some should hard-stop
the work. Treating everything as a gate over-blocks; leaving everything as prose under-enforces. We
will introduce **instincts** — soft, always-on, reasoning-based nudges — as a distinct operator class,
alongside **skills** (loaded on trigger) and **gates/scans** (hard blocks). Instincts are phrased as
*why*, never as a bare *don't*.

## Why

Reasoning generalizes where commands don't: "Use constructor injection — field injection breaks
testability" transfers to new situations; "NEVER use field injection" does not. This realizes the
"constraints as reasoning" idea and the principle of using the *weakest enforcement that still works*.

## Consequences

- A new `instincts/` directory and `gatekeeper instinct` surface; `activate` injects matching instincts.
- The **enforcement spectrum** — instinct → skill → gate → scan — becomes the decision model for where
  any new operator belongs, and for when to *promote* one to a stronger class.
