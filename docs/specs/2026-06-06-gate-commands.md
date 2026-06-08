# Design: Per-stage gate commands

- **Date:** 2026-06-06
- **Feature slug:** gate-commands
- **Status:** approved (2026-06-06)

## Problem

Topology's gates are meant to be **human checkpoints**, but today the only entry point is the
hook-based auto-router: it nudges the agent to *load a skill*, and the agent tends to flow
continuously through the sequence in one turn. A developer who wants to drive **one stage at a
time** — run a stage, review the artifact themselves, approve, then explicitly trigger the next —
has no first-class way to do it. They must hand-instruct "do only X, then stop" every time, which
is easy to forget and inconsistent.

## Goal

A set of **user-invoked Claude Code slash commands**, one per gate, that each run a **single**
stage, run that gate's check, then **STOP** and hand control back to the developer with a suggested
next command. **Additive and non-breaking**: the existing root `skills/` and the auto-routing hook
are unchanged.

## Constraints

- Claude-Code-native commands live in **`.claude/commands/<name>.md`** (a file there creates
  `/<name>`). This is a *different* tree from Topology's root `skills/`, which only the `gatekeeper`
  binary reads — so there is no name collision and no change to gatekeeper. *(Research-confirmed:
  commands and Skills are unified; `.claude/commands/deploy.md` and `.claude/skills/deploy/SKILL.md`
  both create `/deploy`.)*
- Commands must be **user-only** (`disable-model-invocation: true`) so the developer controls the
  cadence and the agent never auto-fires a stage.
- **No new `gatekeeper` (Rust) surface.** Pure Markdown command files that reference the existing
  `skills/<name>/SKILL.md` for the methodology and call the existing `gatekeeper check <gate>`.
- **Non-breaking:** root `skills/`, `hooks/`, and the auto-routing flow stay exactly as they are.

## Approaches considered

1. **Hand-instruct each stage** ("do only X, stop") — works today, but no artifact, easy to forget,
   inconsistent phrasing. Not a durable answer. **Rejected.**
2. **Retrofit the skills to stop-and-hand-off by default** — changes behavior for everyone, even the
   auto-routed flow. The maintainer chose *not* to (commands-only). **Deferred.**
3. **Per-stage user-invoked commands** *(chosen)* — `.claude/commands/<gate>.md`, one per gate, thin
   wrappers over the existing skills + gate checks that stop and suggest the next. The developer opts
   into stepwise by typing `/<gate>`; the continuous flow remains available untouched.

## Decision

Add six command files under **`.claude/commands/`**, named to match the gate skills:
`/brainstorm-design`, `/write-plan`, `/tdd-loop`, `/verify-before-done`, `/code-review`,
`/finish-branch`.

Each command file has:

- **Frontmatter:** `description`; `argument-hint: <feature-slug>`; `disable-model-invocation: true`
  (user-only); `allowed-tools` pre-approving `Bash(gatekeeper *)` plus the stage's own tools
  (e.g. `Bash(cargo *)`/`Bash(git *)` for `tdd-loop`/`finish-branch`).
- **Body (the thin wrapper):** "Follow `skills/<name>/SKILL.md` for feature **`$ARGUMENTS`**. Do
  **only** this stage. Produce/update the stage's artifact. Run `gatekeeper check <gate> --feature
  $ARGUMENTS` and show the result. Then **STOP**: present the artifact for my review and tell me the
  next command, `/<next-gate> $ARGUMENTS`. Do **not** begin the next stage."
- **`/code-review` additionally** runs the critic as a fresh context: `context: fork` (an isolated
  subagent with no memory of writing the code), optionally with a different `model:` where the
  harness allows — matching the review gate's fresh-context-critic requirement. The fork writes the
  artifact and runs `gatekeeper check review`, then the main thread reports and stops.

The command body holds **cadence + stop-and-hand-off only**; the actual methodology stays in
`skills/<name>/SKILL.md` (no duplication). `tdd-loop` has no `gatekeeper check` (it is a discipline
gate), so its command stops after the tests go green.

**Distribution:** `.claude/commands/` is vendored into a target project alongside `skills/` and
`hooks/` and is picked up with **no install step**. `scripts/install.sh` copies it; a future Phase 6
plugin package can ship the same files namespaced as `/topology:<gate>`.

## Workflow (the developer's loop)

```
/brainstorm-design count-words   → writes docs/specs/…, check design, STOP → "approve, then /write-plan count-words"
(developer reviews; sets Status: approved)
/write-plan count-words          → writes docs/plans/…, check plan, STOP → "then /tdd-loop count-words"
/tdd-loop count-words            → red → green, STOP → "then /verify-before-done count-words"
/verify-before-done count-words  → writes docs/verify/…, check verify, STOP → "then /code-review count-words"
/code-review count-words         → fresh critic writes docs/reviews/…, check review, STOP → "then /finish-branch count-words"
/finish-branch count-words       → check finish, present merge/PR options, STOP
```

## Risks & open questions

- **Two entry points** (auto-routing hook + commands) could confuse. Mitigated: commands are
  user-only and reference the same skills; no logic is duplicated. Both are documented in `AGENTS.md`.
- **Slug discipline:** `$ARGUMENTS` must match the artifact filename slug; a mismatch yields a clear
  "no doc found" veto. Accepted.
- **Different-model critic** for `/code-review` is harness-dependent; it falls back to same-model
  fresh-context (the review spec's accepted baseline).
- **Cross-harness** (Codex/Cursor/OpenCode) command equivalents are **out of scope** (Phase 4).
- **`getting-started` / `systematic-debug`** are not sequential gates; optional `/getting-started`
  and `/systematic-debug` commands are a possible follow-up, not part of this feature.

## Acceptance criteria

- [ ] Six files under `.claude/commands/` named for the gate skills (`brainstorm-design.md`,
      `write-plan.md`, `tdd-loop.md`, `verify-before-done.md`, `code-review.md`, `finish-branch.md`).
- [ ] Each has `disable-model-invocation: true` and an `argument-hint`, and pre-approves
      `Bash(gatekeeper *)` (plus stage tools where needed) via `allowed-tools`.
- [ ] Each body runs only its stage per `skills/<name>/SKILL.md`, runs the gate check (except
      `tdd-loop`), then STOPS and names the next command.
- [ ] `/code-review` dispatches a fresh-context critic (`context: fork`) that writes the review
      artifact and runs `gatekeeper check review`.
- [ ] In a real session, `/brainstorm-design <slug>` produces the spec, runs `gatekeeper check
      design`, and stops **without** starting `write-plan`.
- [ ] The existing auto-routing + skills behavior is unchanged (non-breaking): no edits to root
      `skills/`, `hooks/`, or the `gatekeeper` binary.
- [ ] `scripts/install.sh` surfaces the `.claude/commands/` stepwise mode in its output, and
      `AGENTS.md` documents it alongside the auto-routed one. (Full cross-project vendoring is Phase 6.)
- [ ] An ADR records the decision (a Claude-Code-native command layer over the skills; user-only;
      additive; no gatekeeper change).

## Research basis

Claude Code custom commands are unified with Skills: `.claude/commands/<name>.md` and
`.claude/skills/<name>/SKILL.md` both create `/<name>`. Load-bearing fields confirmed against current
docs (June 2026): `disable-model-invocation` (user-only), `allowed-tools` (pre-approve, e.g.
`Bash(gatekeeper *)`), `argument-hint`, `$ARGUMENTS`/`$N` substitution, and `context: fork` + `agent`
(run a command in an isolated subagent). Project `.claude/commands/` is picked up with no install;
plugins namespace as `/plugin:name`. The `!`-prefix shell form runs at *expansion* time (before the
stage), so the post-production gate check is a normal model-run `Bash` call, pre-approved via
`allowed-tools`.
