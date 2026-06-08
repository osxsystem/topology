# The gotcha ledger (`docs/learn/`)

Continuous learning (Phase 3, [ADR-0005](../adr/0005-continuous-learning-capture-gotcha.md)): failures and
corrections become permanent operators — the system tightens exactly where it got burned. This directory
is the **ledger**, the append-only record that sits between a failure and the operator it hardens into.

- `ledger.md` — the entries, written by `gatekeeper learn capture`. (Created on first capture; it may be
  absent in a fresh checkout — that is the empty ledger, not an error.)
- This README — the format and the loop.

## The loop

```
failure / correction ──capture──▶ docs/learn/ledger.md ──promote (human "y")──▶ instinct | skill | rule
```

**Capture** is cheap, append-only, and safe to automate (a `Stop` hook). **Promote** writes standing
policy, so it prints a diff and requires an explicit human `y`. Promotion never edits the ledger; the new
operator carries `source: ledger:<id>` as its back-link, so every promotion is auditable.

## Entry format

`capture` appends one block per call. A strict, hand-rolled parser reads them back (no YAML dependency),
so the shape matters:

```markdown
## <id>

- trigger: gate-failure        # gate-failure | stop | human-correction | manual
- gate: verify                 # the gate, when trigger=gate-failure (optional otherwise)
- kind: instinct               # proposed promotion target: instinct | skill | rule (optional)
- date: 2026-06-08             # YYYY-MM-DD, if supplied (optional)
- source: session:2026-06-08   # free provenance string (optional)

> The gotcha and its lesson as one line — the *why*, phrased to generalize. This text becomes the promoted
> operator's body verbatim, so write it as guidance, not as a bug report.
```

What the parser enforces:

- A line `## <id>` opens an entry; `<id>` is kebab-case (`[a-z0-9-]`, 1..=64, no reserved word) — the same
  rule an instinct id obeys, so it promotes cleanly into an `instincts/<id>.md`, a `skills/<id>/`, or a
  rule id.
- `- key: value` sets a field; an **unknown key is an error** (it fails `learn list` / `learn promote`
  loud, exit 2, naming the offender — mirroring `instinct list` / `scan`).
- `> …` lines are the summary (joined, whitespace-normalized). An entry **must** have a non-empty summary.
- Text before the first `## ` (this file's analog title, intro prose) is preamble and ignored.

## Recurrence is append-only

The ledger is **append-only**: a recurrence is the *same `id` captured again*, never an edited counter.
`gatekeeper learn list` aggregates by id and prints the occurrence count — and that count is the signal
that a gotcha has earned promotion:

```
$ gatekeeper learn list
force-push-after-rebase         1   rule
verify-skipped-on-green-tests   3   instinct
```

(Reuse the same `--id` for a recurrence so it *counts* instead of splintering into near-duplicates.)

## Commands

```bash
# Capture — reuse --id for a recurrence so it counts:
gatekeeper learn capture --summary "<the lesson, as the why>" \
  --trigger gate-failure --gate verify --kind instinct --date "$(date +%F)"

# Review what is accumulating (id <tab> occurrences <tab> proposed-kind):
gatekeeper learn list

# Promote a recurring entry — prints a diff, then asks for confirmation:
gatekeeper learn promote --id verify-skipped-on-green-tests
gatekeeper learn promote --id force-push-after-rebase --kind rule \
  --pattern '\bgit\b.*\bpush\b.*--force\b' --severity warn
```

A promoted **instinct** parses under `gatekeeper instinct list`, a **skill** appears in `gatekeeper list`,
and a **rule** loads under `gatekeeper scan` — `promote` validates each against that surface before it
writes. A new rule starts at `severity = warn`; earn `block` with evidence.

See [`skills/capture-gotcha/SKILL.md`](../../skills/capture-gotcha/SKILL.md) for *when* to recognize a
gotcha, and [`docs/specs/2026-06-08-continuous-learning.md`](../specs/2026-06-08-continuous-learning.md)
for the design.
