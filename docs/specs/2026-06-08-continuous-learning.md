# Design: Continuous learning — capture-gotcha + approved promotion (Phase 3)

- **Date:** 2026-06-08
- **Feature slug:** continuous-learning
- **Status:** draft

> Grounded in `docs/adr/0005-continuous-learning-capture-gotcha.md` (the accepted decision) and the
> `source: ledger:<entry-id>` contract **pre-reserved** by the instincts spec
> (`docs/specs/2026-06-07-instincts-engine.md`, "`source` format"). Phase 3 was anticipated by Phase 2;
> this doc resolves the remaining shape questions (ledger format, promotion contract, confirmation flow).

## Problem

Lessons from failures do not survive across sessions, so the same mistakes recur. Topology has the three
operator layers a lesson could harden into — **instincts** (Phase 2), **skills**, **security rules**
(Phase 1) — but no path from "we got burned here" to "the system now guards here." Phase 3 closes that
loop with two moves:

- **capture** — append a structured gotcha to a ledger when a gate fails or a human corrects the agent.
- **promote** — turn a recurring ledger entry into a new instinct, skill, or `security/rules.toml` rule,
  **with a human approving every promotion** (ADR-0005).

The asymmetry is deliberate: capturing is cheap, reversible, and may be automated (a Stop hook); promoting
writes *standing policy*, so it is explicit, previewed as a diff, and never silent.

**Success looks like:** a forced gate failure leaves a parseable ledger entry; `gatekeeper learn promote`
scaffolds a *valid* operator (it parses / loads under the same surface that operator already ships —
`instinct list`, `gatekeeper list`, `scan`), prints a diff, and writes only after explicit human
confirmation.

## Constraints

- **Append-only ledger (ADR-0005).** `capture` only ever appends; it never rewrites an existing entry.
  Recurrence is therefore represented by *repeated entries that share an `id`*, not by mutating a counter.
  Promotion does **not** edit the ledger either — the back-link lives on the promoted operator
  (`source: ledger:<id>`), not in the entry.
- **Human approves every promotion (ADR-0005).** `promote` prints a diff and requires an explicit `y`
  on stdin (or `--yes`). No confirmation ⇒ nothing is written, exit 0. `capture` is *not* gated (it is
  low-risk and may run from a Stop/gate-failure hook), so it never prompts.
- **Validate against the operator's own contract — do not fork it.** A scaffolded instinct is validated
  by the **instinct parser**; a scaffolded rule by **`scan::load_rules`**; a scaffolded skill by the same
  frontmatter `gatekeeper list` reads. This realizes the instincts spec's promise that "Phase-3 `promote`
  validates against this **same** contract before writing."
- **Exit codes** (`main.rs:1-17`): `0` success / clean abort, `2` usage / parse / validation error. Ledger
  reads (`list`, `promote`) fail loud (exit 2) on a malformed ledger, naming the offender (mirrors
  `instinct list` / `scan`); a missing ledger is the empty set, exit 0.
- **Zero new dependencies.** No date crate (no chrono): `capture` takes an optional `--date YYYY-MM-DD`
  (a Stop hook supplies `$(date +%F)`); omitted ⇒ the field is omitted. No diff crate: promotions are
  *add-only* (a new file, or an appended `[[rule]]`), rendered as `+`-prefixed lines. The four existing
  crates (`regex`/`serde`/`serde_json`/`toml`) are untouched; **no ADR-0007 amendment.**
- **Self-protection friction (expected, not a blocker).** Editing `gatekeeper/src/main.rs` (the `mod
  learn;` declaration + dispatch arm) trips `protected_paths`; land it as one discrete, reviewable commit.
  `gatekeeper/src/learn.rs` (new) and `gatekeeper/src/instinct.rs` (the small `pub` validators) are **not**
  protected paths. `docs/learn/` is **not** protected (the ledger is evidence, not the safety floor).
  A real `promote --kind rule` writes `security/rules.toml` — a protected path — so that path is
  human-gated at commit by design; tests exercise it only against scratch roots.

**Non-goals (explicitly NOT in Phase 3):**

- No **auto-promotion**. Recurrence is *surfaced* (an occurrence count) but a human decides; nothing
  promotes on a threshold.
- No **ledger mutation** (no `status: promoted` rewrite) — it would break append-only. Provenance is the
  operator's `source: ledger:<id>` back-link, greppable for audit.
- No **settings.json auto-wiring** of the Stop hook. `hooks/learn-capture.sh` ships as ready glue; wiring
  it into a harness is the human's call, documented in the skill. (Editing harness config mid-run is both
  protected and a behavior change to the running agent.)
- No **detection-regex inference**. A `rule` promotion cannot guess a pattern from prose, so it requires an
  explicit `--pattern`; without it, `promote --kind rule` is a usage error.
- No **Phase-4 `adapt`**: promoted operators are authored in the one Markdown/TOML source; fan-out is later.

## Approaches considered

The load-bearing fork is **the ledger's physical shape**, because it dictates how recurrence is counted
and how `promote` addresses an entry.

1. **Single append-only file, entries keyed by `## <id>` (chosen).** `capture` appends a block; the same
   `id` may recur. *Trade-offs:* matches ADR-0005's "append-only Markdown" and the ROADMAP's "append a
   structured gotcha" literally; recurrence is visible by aggregating duplicate ids; one file is the
   natural meaning of "ledger." Cost: a hand-rolled multi-record parser (≈ the instinct frontmatter
   parser, one level up).
2. **One file per gotcha (`docs/learn/<id>.md`), rejected.** Mirrors `instincts/*.md`. *Trade-offs:* a
   second capture of the *same* gotcha collides on the filename, so recurrence forces either a clobber or
   a `-2` suffix dance — exactly the signal we want to keep cheap. A directory of files also reads less
   like a chronological "ledger."
3. **A JSON/TOML ledger, rejected.** Easier to parse, but a binary-grade format for a human-skimmed audit
   log is the wrong ergonomics; Markdown stays readable in a PR diff, which is where promotions are reviewed.

## Decision

### A — Ledger shape: one append-only `docs/learn/ledger.md`, `## <id>` records.

Each entry is a Markdown block the parser reads field-by-field. `capture` writes exactly this; humans may
read it and (carefully) hand-author it. Lines before the first `## ` (a title, intro prose) are preamble
and ignored.

```markdown
## <id>

- trigger: gate-failure        # gate-failure | stop | human-correction | manual
- gate: verify                 # which gate, when trigger=gate-failure (else omitted)
- kind: instinct               # proposed promotion target: instinct | skill | rule (else omitted)
- date: 2026-06-08             # YYYY-MM-DD if supplied (else omitted)
- source: session:2026-06-08   # free provenance string (else omitted)

> The gotcha and its lesson, as one normalized line — the text a promotion seeds the operator body with.
```

Parser rules (hand-rolled, `std` only): a line `^## ` opens an entry (`id` = remainder, validated); within
it, `- key: value` sets a known field (unknown key ⇒ error, naming id+key); `> …` lines are the summary
(joined, whitespace-normalized); blank lines ignored. Required: `id` (heading) + a non-empty summary.
Strict throughout (this is a `list`/`promote` read path); a missing ledger ⇒ empty, exit 0.

### B — Capture is non-interactive; promote is the gated, previewed action.

`capture` appends and exits 0 — it must be safe to fire from a Stop/gate-failure hook with no human in the
loop. `promote` is where the human stands: it prints a unified-diff-style preview to **stdout**, then
prompts on **stderr** and reads one line of **stdin**; it writes only if the line is `y`/`yes` (or `--yes`
was passed). Any other answer, or EOF, aborts with nothing written (exit 0). This is ADR-0005's "explicit,
reviewed action, never silent," enforced by construction.

### C — Promotion targets and their contracts.

| `--kind`   | target path                | scaffold carries                              | validated by |
|------------|----------------------------|-----------------------------------------------|--------------|
| `instinct` | `instincts/<id>.md`        | frontmatter `id`/`priority`/`source: ledger:<id>` + summary body | the instinct parser (`instinct::validate_instinct_str`) |
| `skill`    | `skills/<id>/SKILL.md`     | frontmatter `name`/`description` + body scaffold | the frontmatter `gatekeeper list` reads (a local check) |
| `rule`     | append to `security/rules.toml` | a `[[rule]]` block (`--pattern` required; `severity` defaults `warn`) | `scan::load_rules` on the whole candidate file |

- **instinct** carries `source: ledger:<id>` — the scheme the instincts spec reserved for exactly this,
  so no Phase-2 retrofit is needed; default `priority: medium` (overridable via `--priority`).
- **skill** is scaffolded to the *minimum valid shape* (`name` + `description` frontmatter so it appears in
  `gatekeeper list`); the body is a neutral procedure scaffold (no banned plan placeholder tokens).
- **rule** appends rather than creating a file (rules live in one TOML). A detection pattern cannot be
  inferred from a prose lesson, so `--pattern <regex>` is required, `--rule-kind {content|command}` selects
  the lane (default `content`), and `severity` defaults to **`warn`** — a promoted rule starts soft and
  earns `block` with evidence (the `weakest-enforcement-that-works` instinct, made literal).

### D — Validate against the shipped contract, don't re-implement it.

`promote` builds the candidate and validates it **before** showing the diff, using the operator's own
loader: `instinct::validate_instinct_str` (new, tiny `pub` wrapper over the existing `parse_instinct`);
`scan::load_rules` (existing `pub`) against a temp copy of the would-be `rules.toml`; for skills, the same
two-field frontmatter read `gatekeeper list` performs. A scaffold that fails its contract is a `promote`
error (exit 2), never a written file. The integration tests then *independently* re-prove validity by
invoking the real `instinct list` / `gatekeeper list` / `scan` over the written artifact.

### E — Exit codes & fail-mode (mirror `scan`/`instinct`).

`capture`: append, exit 0; bad flags/invalid id ⇒ exit 2. `list`: enumerate, exit 0; malformed ledger ⇒
exit 2 naming the offender; missing ledger ⇒ empty, exit 0. `promote`: exit 0 on write or clean decline;
exit 2 on unknown id / unresolved kind / missing `--pattern` / contract failure / a target that already
exists (refuse to clobber — except the `rule` append, where a duplicate id is caught by `load_rules`).

### F — `id` discipline reuses the instinct rules.

An entry `id` is validated by `instinct::validate_id` (promoted from private to `pub`): kebab-case, 1..=64,
no reserved word — so it drops cleanly into an instinct `id`, a skill directory name, or a rule id. `--id`
is optional; when omitted it is `slugify(--summary)` (lowercase, non-alphanumeric runs → `-`, trimmed),
then validated; if slugging cannot produce a valid id, `capture` asks for an explicit `--id` (exit 2).

## Architecture (integration map — confirmed against source)

- **New evidence layer:** `docs/learn/ledger.md` (the gotcha ledger) + `docs/learn/README.md` (the format).
- **New module:** `gatekeeper/src/learn.rs`; declare `mod learn;` at `main.rs:27-29` beside `mod instinct;`
  / `mod review;` / `mod scan;` (kept alphabetical: `instinct`, `learn`, `review`, `scan`).
- **Dispatch:** one arm below the `instinct` arm (`main.rs:~47`):
  `Some("learn") => learn::cmd_learn(&args[1..], &framework_root())`. The module owns its inner
  `capture` / `list` / `promote` match (`_ => usage exit 2`), mirroring `scan::cmd_scan` / `cmd_instinct`.
- **Reuse:** `instinct::validate_id` (now `pub`) + new `instinct::validate_instinct_str`; existing
  `pub scan::load_rules`. `instinct.rs` is not a protected path, so these additions are friction-free.
- **New skill:** `skills/capture-gotcha/SKILL.md` — recognize a *recurring* failure and route it to the
  ledger (and, once recurring, to `promote`).
- **Optional glue:** `hooks/learn-capture.sh` — an example `Stop` hook that calls
  `gatekeeper learn capture`; wiring it into `.claude/settings.json` is documented, not auto-applied.
- **Help + docs:** extend `print_help()` and the `//!` block in `main.rs`; flip `docs/ROADMAP.md` Phase 3
  to delivered (status table + verify criterion); record `docs/verify/2026-06-08-continuous-learning.md`.
- **Tests:** `gatekeeper/tests/cli_learn.rs`, mirroring `cli_scan.rs` (scratch root with a `skills/` marker,
  a `security/rules.toml`, and an `instincts/` dir; exec `env!("CARGO_BIN_EXE_gatekeeper")`).
- **No dependency changes:** `Cargo.toml` / `Cargo.lock` untouched.

## Risks & open questions

- **Recurrence is surfaced, not enforced.** A human still decides when "twice" becomes "promote." This is
  intentional (ADR-0005 rejects automatic hardening), but it means the loop tightens only with attention;
  the skill names the trigger so attention is cued.
- **Append-only vs. de-dup.** A noisy capture loop could spam near-duplicate entries. Mitigation: `list`
  aggregates by `id`, and the skill instructs the agent to reuse an existing `id` for a recurrence (so it
  *counts* rather than *splinters*). Cross-id semantic dedup is out of scope.
- **A `rule` promotion needs a real pattern.** Requiring `--pattern` keeps it honest but means rule
  promotion is the least "one-button" path — correct, since a bad pattern is a false-positive hard-block.
  Starting at `severity = warn` blunts that risk.
- **Stop-hook wiring is left to the human.** Shipping the script but not wiring it means the automated
  capture path is opt-in; the skill-driven path (agent runs `capture` on recognizing a recurrence) is the
  always-available one.

## Acceptance criteria

- [ ] `gatekeeper learn capture --summary "…" [--trigger …] [--gate …] [--kind …] [--id …] [--date …]
      [--source …]` appends a parseable `## <id>` entry to `docs/learn/ledger.md` (creating it if absent);
      exit 0. A forced gate failure (capture invoked from the failure path) leaves a ledger entry.
- [ ] `gatekeeper learn list` enumerates entries aggregated by `id` (id + occurrence count + proposed
      kind), sorted; a malformed ledger exits 2 naming the offender; a missing ledger is empty, exit 0.
- [ ] `gatekeeper learn promote --id <id> --kind instinct` writes `instincts/<id>.md` (with
      `source: ledger:<id>`) that **passes `gatekeeper instinct list`**; `--kind skill` writes
      `skills/<id>/SKILL.md` that **appears in `gatekeeper list`**; `--kind rule --pattern <re>` appends a
      `[[rule]]` that **`gatekeeper scan` loads** (scan-load passes).
- [ ] `promote` prints a diff to stdout and writes **only** on explicit confirmation (`y` on stdin or
      `--yes`); a declined promotion writes nothing and exits 0.
- [ ] `promote` fails loud (exit 2) on: unknown id, unresolved `--kind`, missing `--pattern` for a rule,
      a contract-invalid scaffold, or a non-rule target that already exists.
- [ ] **No new dependency:** `Cargo.toml` / `Cargo.lock` unchanged; binary still builds offline.
- [ ] `cargo test` green (new `cli_learn.rs` + colocated unit tests); `cargo fmt --check` clean;
      `cargo clippy --all-targets -- -D warnings` clean.
- [ ] `docs/learn/README.md` documents the ledger format and the capture→promote loop; `docs/ROADMAP.md`
      Phase 3 marked delivered with a re-runnable verify criterion; `skills/capture-gotcha/SKILL.md` exists.
