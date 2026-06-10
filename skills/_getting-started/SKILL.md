---
name: getting-started
description: Bootstrap the Topology methodology for a coding session; gate artifacts land under the artifacts root (`docs/` here, `.claude/topology/` in a governed project). Use at the start of any coding task, or when the user mentions starting a feature, fixing a bug, planning work, or asks what to do next.
---

# Getting Started with Topology

You have skills. They are mandatory workflows, not suggestions. Before doing anything else on a coding task:

1. **Inventory.** List the skills in `skills/`. Each has a one-line `description` telling you what it does and when to use it.
2. **Route.** Name the skills relevant to the current task. Run `echo "<the user request>" | gatekeeper activate` to see the suggested routing.
3. **Gate.** Identify which gate the task is currently at:
   - No design doc yet → start with `brainstorm-design`.
   - Design approved, no plan → `write-plan`.
   - Plan approved → implement via `tdd-loop`.
   - A test or behavior is failing unexpectedly → `systematic-debug`.
   - Implementation believed complete → `verify-before-done`, then `finish-branch`.
4. **Act.** Follow the skill for the current gate. Do not jump ahead.

## The prime directive

You may not write production code before the **design** and **plan** gates pass. Check them:

```bash
gatekeeper check design --feature <feature-slug>
gatekeeper check plan   --feature <feature-slug>
```

If a check fails, the gate is not passed. Go produce the missing artifact. Do not rationalize past it.

## When in doubt

State, in one line, which gate you are at and which skill you are about to follow. If you are tempted to skip a gate, say so explicitly and name the gate — surfacing the skip is itself a gate.
