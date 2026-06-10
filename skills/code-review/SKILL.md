---
name: code-review
description: Dispatch a fresh-context critic subagent to audit the branch diff against the plan and the repo's standards, then write a commit-bound review artifact (under the artifacts root: `docs/` here, `.claude/topology/` in a governed project) the review gate checks. Use after verify-before-done passes and before finish-branch, or when the user asks for a review, audit, or critique before merge.
---

# Code Review (the review gate)

The author cannot grade their own work — a separate critic must. Dispatch a **fresh subagent**
(no memory of writing the code), preferably a **different model** where the harness allows it.

## Process

1. **Compute the diff scope.** `base = git merge-base <integration-branch> HEAD` (default
   `main`; override with `--base`). The critic reviews `git diff <base>...HEAD` (three-dot).
2. **Dispatch one fresh critic subagent** with the diff, the design doc, the plan, and the repo
   standards (`docs/adr/`, `AGENTS.md`, `METHODOLOGY.md`, `CONTEXT.md` if present).
3. **Review two dimensions separately and document each:**
   - **Spec/plan conformance** — does the diff implement the acceptance criteria and the plan?
     Flag missing, partial, or scope-creep.
   - **Standards conformance** — does the diff follow the cited ADRs / AGENTS / METHODOLOGY?
     Cite the standard.
4. **Require evidence.** Every blocking finding cites `file:line`. No location -> not a blocker.
5. **Seek reasons to FAIL first.** Skip tooling-enforced checks (lint/format/types the `finish`
   gate or linters catch). Distinguish hard violations from judgement calls.
6. **Write the artifact atomically** to `docs/reviews/<YYYY-MM-DD>-<feature-slug>.md`
   (write a temp file, then rename) using the exact grammar below. Never put HTML comments or
   raw diff lines in the machine-parsed regions.

## Artifact grammar (the gate's contract)

Lines 1-3 are the machine header. Use `git rev-parse HEAD` and the computed merge-base verbatim
(full SHAs). A passing review looks exactly like:

```
VERDICT: pass
HEAD: <full 40/64-hex sha from git rev-parse HEAD>
BASE: <full 40/64-hex merge-base sha>

# Review: <feature> (<date>)

## Blocking findings
None.

## Non-blocking notes
- <nits — never gate on these>

## Criteria checked
### Spec/plan
- <acceptance criterion> — <how the diff satisfies it>
### Standards
- <ADR/AGENTS rule> — <conformance evidence>
```

A failing review is identical except line 1 is `VERDICT: fail` and `## Blocking findings` lists
one or more `- <file:line> — <why it blocks>` items instead of `None.`.

## Gate check

```bash
gatekeeper check review --feature <feature-slug> [--base <ref>]
```

Passes only when: the worktree is clean (except `docs/reviews/`), exactly one artifact names the
current `HEAD`, its `BASE` equals the computed merge-base, both rubric dimensions are present, and
the verdict is `pass` with no blocking findings. Every ambiguity fails closed. Then transition to
`finish-branch`.

## The bar

A critic that found problems cannot be rubber-stamped, and a review of stale, dirty, or
wrong-based code cannot be replayed. The parser and git state — not the model's prose — are the
trust boundary.
