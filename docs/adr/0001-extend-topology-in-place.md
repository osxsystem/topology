# 0001 — Extend Topology in place

- **Status:** Accepted
- **Date:** 2026-06-04

A complete operator system was requested. Topology already ships a tested, dependency-free `gatekeeper`
binary, the design→plan→tdd→verify→finish gate sequence, Bash hooks, and seven process skills, wired
for Claude Code. We will **extend Topology in place** — keep the binary and the gate sequence, and layer
the new pillars (instincts, memory, continuous learning, security scanning, cross-harness adapters)
on top in the same repo — rather than introducing a separate "operator" brand layer or starting a
fresh design.

## Why

Lowest risk and highest reuse: the gate philosophy is already the right substrate, the binary is
small and tested, and the three-language split (Rust/Bash/Markdown) is sound. A rebrand or rewrite
would discard working code for no functional gain.

## Consequences

- New capabilities arrive as `gatekeeper` subcommands plus new Layer-0 directories — no rewrite.
- "Operator" becomes the generic noun spanning instincts/skills/gates/scans, *without* a separate
  umbrella component to build and maintain.
- Considered and rejected: an "Operator umbrella over Topology" (more surface, no benefit) and a fresh
  design reusing only the ideas (throws away tested code).
