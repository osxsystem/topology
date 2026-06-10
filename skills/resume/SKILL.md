---
name: resume
description: Reload a handoff artifact and verify real repo state before continuing work on a feature; gate artifacts live under the artifacts root (`docs/` here, `.claude/topology/` in a governed project). Use when picking up a feature across sessions, re-entering a worktree, or starting a fresh session on an in-progress branch.
---

# Resume (the startup gate)

A handoff artifact tells you what the *last session* believed was true. Belief is not state.
Verify before you act.

## Process

### 1. Load the handoff artifact

```bash
gatekeeper memory read --feature <slug>
```

This prints the YAML frontmatter (machine contract) and the Markdown body (prose context).
Read both. Note the recorded `status`, `branch`, `head_sha`, and `verified_by` fields.

If the command exits non-zero (`memory/artifacts/<slug>.handoff.md` not found), stop: ask the
user for the feature slug or have them write a fresh handoff with `gatekeeper memory write`.

### 2. Verify actual git state

```bash
git log --oneline -10
git status
```

Compare the HEAD SHA and branch in the handoff to what `git log` shows. If they diverge, note
the gap and resolve it before touching any code. The handoff records where the *writing* session
left off; the repo may have advanced or been amended.

### 3. Run a smoke / build check

```bash
cargo test
```

(Or whatever the relevant build command is for the current module — see `AGENTS.md` for the
project's canonical test command.) Run it now, before doing anything else. This catches
undocumented broken state that the handoff body might not mention. A handoff that says
`status: in-progress` and shows clean output means nothing if the suite is red.

Do not skip this step because the handoff says the build was green. That was then. Verify now.

### 4. Only then act

With the handoff loaded, git state reconciled, and a green build confirmed, you have a reliable
baseline. Begin the next task.

## Disciplines

### One slice per session

Pick **one** well-defined task from the plan and complete it fully — including a commit — before
starting another. A session that touches five tasks partially is harder to hand off than a session
that closes one task cleanly.

### Never self-assert `done`

`status: done` in a handoff artifact requires two conditions enforced by the gatekeeper:

1. **`--verified-by <verify-slug>` must be non-empty.** This is the slug of the verification
   evidence.
2. **`docs/verify/*<slug>*.md` must exist on disk.** `gatekeeper memory write --status done`
   is refused if that note is absent.

The sequence:

```bash
# 1. Produce the evidence note via the verify-before-done skill (it writes
#    docs/verify/<date>-<slug>.md), then confirm the gate sees it:
gatekeeper check verify --feature <slug>

# 2. Only after the note exists, write the final handoff.
gatekeeper memory write \
  --feature <slug> \
  --date <YYYY-MM-DD> \
  --status done \
  --verified-by <verify-slug> \
  < body.md
```

If you have not produced `docs/verify/*<slug>*.md` yet, see the
[`verify-before-done`](../verify-before-done/SKILL.md) skill. Do not attempt to force
`status: done` without it — the write will be refused.

## Cross-references

- [`skills/research-first/`](../research-first/) — the research-first skill documents the
  required explore-before-design protocol that precedes the work you are resuming.
- [`memory/README.md`](../../memory/README.md) — full frontmatter contract, update path, and
  template reference for handoff artifacts.
