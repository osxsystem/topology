---
name: capture-gotcha
description: Turn a recurring failure or human correction into a permanent operator — capture it to the gotcha ledger with `gatekeeper learn capture`, then once it recurs promote it into an instinct, skill, or security rule. Use when the same mistake happens twice, a gate keeps failing the same way, a human corrects you on something a rule could have caught, or you are asked to "remember this" / "make sure this doesn't happen again".
---

# Capture a gotcha (continuous learning)

A failure that doesn't outlive the session will recur. This skill closes the loop: **capture** the lesson
to the ledger now, **promote** it into a standing operator once it has earned its place. The asymmetry is
the point — capturing is cheap and automation-friendly; promoting writes standing policy, so a human
approves it (ADR-0005).

## When to capture

Capture when a failure is the kind that *recurs*, not a one-off slip:

- A **gate fired** and that same gate has bitten you before (a verify note that missed a break; a plan
  that shipped a placeholder; a force-push the scanner caught).
- A **human corrected you** on something a rule or instinct could have caught ("you edited the file
  without reading it first", "you claimed done without re-running the test").
- You hit the **same mistake twice** in one session.

If it is a genuine one-off, don't capture — the ledger is signal, not a diary.

## How to capture

```bash
gatekeeper learn capture \
  --summary "Unit tests passing is not the verify gate; record a re-runnable end-to-end command and its output" \
  --trigger gate-failure --gate verify --kind instinct --date "$(date +%F)"
```

- `--summary` is the **lesson**, phrased as the *why* — it becomes the promoted operator's body verbatim.
  Write "X breaks when Y, so do Z", not "X broke".
- For a **recurrence, reuse the same `--id`** (an explicit `--id`, or the same summary, which slugs to the
  same id). That makes it *count* in `gatekeeper learn list` instead of splintering into near-duplicates —
  and the count is the signal that it is time to promote.
- `--kind` records where you think it should land (`instinct` | `skill` | `rule`); you can override it at
  promote time. `--trigger` is `gate-failure` | `stop` | `human-correction` | `manual`.

Review what is accumulating:

```bash
gatekeeper learn list      # id  <tab>  occurrences  <tab>  proposed-kind
```

## When and how to promote

Promote when an entry has **recurred** (occurrences > 1) or is plainly standing policy. Route by what kind
of guidance it is — prefer the **weakest** operator that prevents the recurrence:

- **instinct** — a way to *reason* that generalizes, always-on framing → `--kind instinct`.
- **skill** — a *procedure* to follow for a class of task, routed by keyword → `--kind skill`.
- **rule** — a deterministic *pattern* to veto (a secret shape, a dangerous command) → `--kind rule
  --pattern '<regex>'`. A rule needs a real detection pattern; it cannot be inferred from prose.

```bash
gatekeeper learn promote --id verify-skipped-on-green-tests      # prints a diff, then asks for "y"
```

`promote` **prints the operator as a diff and waits for your `y`** before writing — promotion is never
silent. It validates the scaffold against that operator's own loader first, so a promoted instinct already
parses under `gatekeeper instinct list`, a skill shows in `gatekeeper list`, and a rule loads under
`gatekeeper scan`. A new rule starts at `severity = warn` — earn `block` with evidence. The promoted
operator carries `source: ledger:<id>` back to its gotcha.

## Automating capture (optional)

`hooks/learn-capture.sh` is a `Stop` hook that records a gotcha when you set `TOPOLOGY_GOTCHA` during the
turn; wiring it into `.claude/settings.json` is a human action (settings is a protected path). The
always-available path is this skill: recognize the recurrence, run `gatekeeper learn capture`.

## The bar

Capturing is reversible and cheap — do it the moment a recurrence is clear. Promoting is standing policy —
do it deliberately, with a human's `y`, routing to the lightest operator that closes the gap.
