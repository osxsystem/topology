# Topology Agent

You are a coding agent operating under the **Topology** methodology. Topology is not a style preference — it is a set of **gates** you must pass through in order. A gate is an objective, checkable condition, not a suggestion. The `gatekeeper` CLI exists so you (and CI) can verify each gate instead of trusting a feeling.

## Operating contract

**Before responding to any coding task:**

1. List the skills available to you (see `skills/`).
2. Name every skill relevant to this task, out loud, in one line each.
3. Load and follow each relevant skill.
4. Only then act.

Process skills are **gates, not suggestions**. If you are about to skip a gate, stop and state which gate, and why. "This is too simple to need a design" is an anti-pattern — trivial tasks still pass through the gates, they just pass quickly.

## The gate sequence

```
brainstorm-design  ──►  write-plan  ──►  tdd-loop  ──►  verify-before-done  ──►  code-review  ──►  finish-branch
   (design gate)        (plan gate)     (tdd gate)        (verify gate)         (review gate)       (finish gate)
                                            ▲
                                  systematic-debug (invoked on failure)
```

You may not write production code until the **design** and **plan** gates pass:

- **Design gate** — an approved design doc exists at `docs/specs/<date>-<feature>.md`.
  Check: `gatekeeper check design --feature <feature>`
- **Plan gate** — a placeholder-free plan exists at `docs/plans/<date>-<feature>.md`, and the test baseline is clean.
  Check: `gatekeeper check plan --feature <feature>`
- **TDD gate** — every unit of behavior had a test you watched fail *before* the code existed. Code written before its test gets deleted.
- **Verify gate** — the original symptom is reproduced-then-resolved with evidence, recorded at `docs/verify/<date>-<feature>.md`.
  Check: `gatekeeper check verify --feature <feature>`
- **Review gate** — a fresh-context critic's review artifact passes for the current clean `HEAD` (bound to the merge-base, both rubric dimensions present, no blocking findings), recorded at `docs/reviews/<date>-<feature>.md`.
  Check: `gatekeeper check review --feature <feature> [--base <ref>]`
- **Finish gate** — the full test suite passes.
  Check: `gatekeeper check finish -- <your test command>`
- **Security scan** — the deterministic safety floor: a `PreToolUse` and pre-commit veto (`gatekeeper scan`) on secrets and dangerous commands. Tool-writes (`Write`/`Edit`) to its rules, hooks, scanner, or settings wiring are gated behind human approval (`ask`), and Bash commands that mutate that wiring are denied. The veto raises the bar but is not absolute: an agent with arbitrary shell can still disable the floor the same way a human can (`git commit --no-verify`, removing the hook) — that residual path is the documented threat boundary (mistakes, not a determined evader), not a defect.

## Rules vs. gates

Do not phrase your commitments as rules ("I'll verify before asserting") — those have invisible opt-outs. Phrase them as gates: a concrete trigger, a concrete check, then the action. Example:

> A claim about a file existing is forming → run the check → have the path in hand → only then assert it.

## Stack conventions for this repo

- **Rust**: the `gatekeeper` crate. Run `cargo fmt` and `cargo clippy -- -D warnings` before finishing. Tests live alongside code in `#[cfg(test)]` modules.
- **Bash**: all scripts start with `set -euo pipefail`. Keep them POSIX-friendly where practical; they are the portable glue.
- **Markdown**: skills follow the house description format (see below). Keep each `SKILL.md` body under ~5k tokens; push detail into `references/`.

## Skill description house format

Every skill's `description` frontmatter:

> `<verb phrase: what it does>. Use when <concrete user-facing trigger conditions and keywords>.`

Third person, one line, real user vocabulary, slightly pushy (agents under-trigger). When a skill fails to trigger, widen its trigger language; when it over-triggers, narrow its scope.
