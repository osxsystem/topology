# Plan: Continuous learning (Phase 3)

- **Date:** 2026-06-08
- **Feature slug:** continuous-learning
- **Design:** docs/specs/2026-06-08-continuous-learning.md (Status: draft); ADR-0005 (Accepted).
- **Baseline:** tests green on `feat/continuous-learning` branched from `origin/main` (post-PR-#4):
  **126 passed, 2 ignored across 4 suites** (`cd gatekeeper && cargo test`). Confirm before starting.

## Conventions for every task

- **No new dependencies.** Zero crates added; `gatekeeper/Cargo.toml` / `Cargo.lock` and every ADR are
  untouched. The ledger parser is hand-rolled on `std` (the four existing crates are not needed for it).
- **Tests are the per-task gate.** Each task runs `cd gatekeeper && cargo test <filter>` and ends green
  (exit 0). `cargo build` / `cargo test` may print warnings; **`clippy -D warnings` and `cargo fmt --check`
  are enforced once, in the final verify task** (as the prior two plans did).
- **Test style:** the existing std style — `std::process::Command` + `env!("CARGO_BIN_EXE_gatekeeper")`
  for CLI/integration tests (as in `tests/cli_scan.rs`); plain `#[cfg(test)] mod` for unit tests. No
  `assert_cmd` / `predicates`.
- **Self-protection friction (expected, not a blocker).** Editing `gatekeeper/src/main.rs` (Task 3) trips
  `protected_paths`; with no pre-commit hook installed and `bypassPermissions` set in this clone it lands
  without a prompt, but it is still isolated as one discrete, reviewable commit. `gatekeeper/src/learn.rs`
  (new), `gatekeeper/src/instinct.rs` (the `pub` validators), `docs/learn/`, `skills/`, and
  `hooks/learn-capture.sh` are **not** protected paths. Create source files with the Write/Edit tools, not
  Bash redirection (a Bash write into `gatekeeper/src/` matches the `tamper-security-wiring` command rule).
- **Append-only ledger.** `capture` only appends. Recurrence = repeated entries sharing an `id`; nothing
  rewrites an entry, and `promote` does not edit the ledger.
- **Promotion is human-gated.** `promote` prints a diff and writes only on explicit `y`/`--yes`.

## Files

- `gatekeeper/src/learn.rs` — **new.** The whole module: `Trigger` + `Kind` enums, the `Entry` model, the
  hand-rolled ledger parser (`parse_ledger`), `slugify`, the `capture` / `list` / `promote` handlers, the
  diff renderer, the scaffold builders, the `cmd_learn` dispatcher, and `#[cfg(test)]` unit modules.
- `gatekeeper/src/instinct.rs` — **modify.** Promote `validate_id` to `pub`; add `pub fn
  validate_instinct_str(raw: &str) -> Result<(), String>` (a one-line wrapper over `parse_instinct`).
  Both are reused by `learn.rs`; `instinct.rs` is not a protected path.
- `gatekeeper/src/main.rs` — **modify (protected; isolated commit).** Declare `mod learn;`; add the
  `"learn"` dispatch arm; extend `print_help()` and the `//!` block.
- `gatekeeper/tests/cli_learn.rs` — **new.** Integration tests over a scratch framework root (mirrors
  `tests/cli_scan.rs`).
- `docs/learn/README.md` — **new.** The gotcha-ledger format + the capture→promote loop.
- `skills/capture-gotcha/SKILL.md` — **new.** Recognize a recurring failure and route it.
- `hooks/learn-capture.sh` — **new.** Example `Stop` hook calling `gatekeeper learn capture`; wiring it is
  documented in the skill, not auto-applied.
- `docs/ROADMAP.md` — **modify.** Phase 3 → delivered; rewrite the verify criterion as re-runnable commands.
- `docs/verify/2026-06-08-continuous-learning.md` — **new** (Task 8). Verification evidence.

## Tasks

### Task 1: Design docs — spec + plan

- **File(s):** `docs/specs/2026-06-08-continuous-learning.md` (new), `docs/plans/2026-06-08-continuous-learning.md` (new).
- **Change:** Author the spec (problem, constraints, lettered decisions A–F, integration map, acceptance
  criteria) and this plan. ADR-0005 already records the accepted decision; no new ADR.
- **Test:** `cd gatekeeper && cargo test` → still **126 passed, 2 ignored** (docs do not affect tests).
  `./target/debug/gatekeeper check plan --feature continuous-learning` once the binary is built → PASS
  (placeholder-free).
- **Commit:** `docs: spec + plan for continuous learning (Phase 3)`

### Task 2: `learn.rs` — model + ledger parser + `slugify`

- **File(s):** `gatekeeper/src/learn.rs` (new). (Not yet a module; wired in Task 3. Unit tests in this file
  run only after `mod learn;` exists, so this task's test gate is deferred to Task 3's run — the file is
  authored complete here and verified there.)
- **Change:** Define:
  - `enum Trigger { GateFailure, Stop, HumanCorrection, Manual }` with `as_str` / `parse` (unknown ⇒ Err
    naming the value); default `Manual`.
  - `enum Kind { Instinct, Skill, Rule }` with `as_str` / `parse` (unknown ⇒ Err); the promotion target.
  - `struct Entry { id, trigger, gate: Option<String>, kind: Option<Kind>, date: Option<String>,
    source: Option<String>, summary: String }`.
  - `fn parse_ledger(raw: &str) -> Result<Vec<Entry>, String>` — `## <id>` opens a record (id validated
    via `instinct::validate_id`); `- key: value` sets a known field (unknown key ⇒ Err naming id+key);
    `> …` lines accumulate the summary (joined, whitespace-normalized); preamble before the first `## ` is
    ignored; a record with an empty summary ⇒ Err. Strict.
  - `fn slugify(s: &str) -> Option<String>` — lowercase, map non-`[a-z0-9]` runs to `-`, trim hyphens,
    collapse `--`, cap at 64; `None` if the result is empty or fails `instinct::validate_id`.
  - Colocated `#[cfg(test)] mod parse_tests` (a valid multi-entry ledger parses; recurrence keeps both
    entries; unknown field errors; missing summary errors; preamble is ignored) and `mod slug_tests`
    (`"Verify skipped on green tests"` → `"verify-skipped-on-green-tests"`; punctuation collapses; an
    all-symbol string ⇒ `None`).
- **Test (verified in Task 3):** `cargo test learn::parse_tests learn::slug_tests` → all green.
- **Commit:** folded into Task 3 (the module is not linkable until `mod learn;` lands, so authoring +
  wiring + first green run are one coherent commit).

### Task 3: `instinct.rs` validators + `learn.rs` handlers + `main.rs` wiring

- **File(s):** `gatekeeper/src/instinct.rs`, `gatekeeper/src/learn.rs`, `gatekeeper/src/main.rs`.
- **Change (a) — `instinct.rs`:** change `fn validate_id` to `pub fn validate_id`; append
  `pub fn validate_instinct_str(raw: &str) -> Result<(), String> { parse_instinct(raw).map(|_| ()) }`.
- **Change (b) — `learn.rs` handlers:**
  - `pub fn cmd_learn(args: &[String], root: &Path) -> i32` — match `capture` / `list` / `promote`;
    `_ => { eprintln!("gatekeeper learn: expected `capture`, `list`, or `promote`"); 2 }`.
  - `capture`: parse `--summary` (required) / `--trigger` / `--gate` / `--kind` / `--id` / `--date` /
    `--source`; derive `id` from `--id` or `slugify(summary)` (no valid id ⇒ exit 2 asking for `--id`);
    append the formatted block to `docs/learn/ledger.md` (create with a `# Gotcha ledger` title if absent);
    print `captured '<id>' → docs/learn/ledger.md`; exit 0.
  - `list`: `parse_ledger` the file (strict; missing ⇒ empty, exit 0; malformed ⇒ exit 2); aggregate by
    `id`; print `"{id}\t{occurrences}\t{kind-or-dash}"` sorted by id; exit 0.
  - `promote`: parse `--id` (required) / `--kind` (override) / `--priority` (instinct) /
    `--pattern` + `--rule-kind` + `--severity` (rule) / `--yes`; load the entry (unknown ⇒ exit 2);
    resolve kind (`--kind` > entry.kind > exit 2); build + validate the scaffold (see Change (c)); refuse a
    pre-existing non-rule target (exit 2); print the diff to stdout; if not `--yes`, prompt on stderr and
    read one stdin line, abort (exit 0, nothing written) unless `y`/`yes`; write; print the result.
  - `fn render_add_diff(path: &str, body: &str) -> String` — `--- /dev/null` / `+++ <path>` then each
    body line `+`-prefixed (for an appended rule, header `+++ security/rules.toml`).
- **Change (c) — scaffold builders (exact templates):**
  - instinct → `instincts/<id>.md`:
    ```
    ---
    id: <id>
    priority: <priority|medium>
    source: ledger:<id>
    ---
    <summary>
    ```
    validated with `instinct::validate_instinct_str`.
  - skill → `skills/<id>/SKILL.md`:
    ```
    ---
    name: <id>
    description: <summary> Use when this recurring failure is about to repeat.
    ---

    # <id>

    <summary>

    > Scaffolded by `gatekeeper learn promote` from ledger entry `<id>`. Replace this note with the
    > procedure: name the trigger, the action to take, and the bar for done.
    ```
    validated by reading back a non-empty `name:` + `description:` from the frontmatter.
  - rule → append to `security/rules.toml`:
    ```
    [[rule]]
    id = "<id>"
    kind = "<rule-kind|content>"
    severity = "<severity|warn>"
    description = "<summary, quotes escaped>"
    pattern = '<pattern>'
    ```
    validated by writing `<existing rules.toml> + block` to a temp file and calling `scan::load_rules`;
    `--pattern` is required (no pattern ⇒ exit 2).
  - Colocated `#[cfg(test)] mod scaffold_tests`: the instinct scaffold passes `validate_instinct_str`; the
    rule scaffold + a minimal base loads via `scan::load_rules`; a rule scaffold with a broken pattern
    fails to load.
- **Change (d) — `main.rs` (protected):** add `mod learn;` (keeping `instinct`, `learn`, `review`, `scan`
  alphabetical); add `Some("learn") => learn::cmd_learn(&args[1..], &framework_root()),` below the
  `instinct` arm; add the two `learn` lines to `print_help()` and the matching `//!` lines.
- **Test:** `cd gatekeeper && cargo test learn` (unit modules) → green; `cargo test` → **126 + new
  unit** green. No `dead_code` (every `learn.rs` item is reached through `cmd_learn`).
- **Commit:** `feat(gatekeeper): learn capture/list/promote — the gotcha ledger + approved promotion`
  (this commit includes the protected `main.rs` wiring).

### Task 4: `cli_learn.rs` — integration tests

- **File(s):** `gatekeeper/tests/cli_learn.rs` (new).
- **Change:** Mirror `cli_scan.rs`: a `scratch_root(tag)` with `skills/` + `security/rules.toml` (one
  content rule) + `instincts/` markers, and a `run(cwd, args, stdin) -> (i32, String)` helper. Cases:
  - `capture_appends_then_list_counts_recurrence` — capture the same `--id` twice; `learn list` shows it
    once with occurrence count `2`.
  - `capture_writes_parseable_entry` — after capture, `docs/learn/ledger.md` contains `## <id>` and the
    summary; a second distinct id yields two list rows.
  - `promote_instinct_passes_instinct_list` — capture (`--kind instinct`), `promote --id … --yes`, assert
    `instincts/<id>.md` exists with `source: ledger:<id>`, and `gatekeeper instinct list` (exit 0) lists it.
  - `promote_skill_appears_in_gatekeeper_list` — `promote --kind skill --yes`; `gatekeeper list` output
    contains `<id>` and the description.
  - `promote_rule_loads_under_scan` — `promote --kind rule --pattern '\bFIXME-SECRET\b' --yes`; the planted
    string then blocks under `gatekeeper scan --content` (exit 1), proving the rule loaded and matches.
  - `promote_requires_confirmation` — feed `n\n`; assert `instincts/<id>.md` is **not** created, exit 0.
  - `promote_unknown_id_exits_2` and `promote_rule_without_pattern_exits_2`.
  - `list_on_malformed_ledger_exits_2` — write an entry with an unknown `- bogus:` field; exit 2.
- **Test:** `cd gatekeeper && cargo test --test cli_learn` → all cases green.
- **Commit:** `test(gatekeeper): cli_learn integration — capture, list, and gated promotion`

### Task 5: The ledger README + the capture-gotcha skill + the Stop hook

- **File(s):** `docs/learn/README.md` (new), `skills/capture-gotcha/SKILL.md` (new),
  `hooks/learn-capture.sh` (new).
- **Change:**
  - `docs/learn/README.md`: document the entry format (the `## <id>` block, every field, the `>` summary),
    the append-only + recurrence rule, the three promotion targets and their validation, and the
    capture→promote loop with copy-pasteable `gatekeeper learn …` commands.
  - `skills/capture-gotcha/SKILL.md`: frontmatter `name: capture-gotcha` + a `description` ending in a
    "Use when …" clause (so `gatekeeper list` and the router pick it up); body covering *when* a failure
    is worth capturing (a gate fired, a human corrected you, the same mistake twice), *how* to capture
    (reuse an existing `id` for a recurrence so it counts), and *when/how* to propose a `promote`
    (human-approved), routing each gotcha to instinct vs skill vs rule.
  - `hooks/learn-capture.sh`: a small `Stop` hook reading the harness JSON on stdin and calling
    `gatekeeper learn capture --trigger stop --date "$(date +%F)" --summary "…"`; a header comment shows
    the `.claude/settings.json` `Stop` wiring (documented, not applied).
- **Test:** `cd gatekeeper && cargo test` → unchanged green; `./target/debug/gatekeeper list` shows
  `capture-gotcha` with its description; `bash -n hooks/learn-capture.sh` → exit 0.
- **Commit:** `docs(learn): ledger README, capture-gotcha skill, and example Stop hook`

### Task 6: ROADMAP — Phase 3 delivered

- **File(s):** `docs/ROADMAP.md`.
- **Change:** Mark Phase 3 delivered in the Mermaid diagram, the section heading, and the status table;
  rewrite the verify criterion as the re-runnable commands from the acceptance criteria (capture writes an
  entry; `promote` produces a valid instinct/skill/rule that loads under `instinct list` / `gatekeeper
  list` / `scan`; promotion requires explicit confirmation). Point "Evidence" at the Task-8 verify note.
- **Test:** `cd gatekeeper && cargo test` → unchanged green; `rg "Phase 3" docs/ROADMAP.md` shows the
  delivered marker.
- **Commit:** `docs(roadmap): mark Phase 3 (continuous learning) delivered`

### Task 7: Verify — clippy/fmt clean, full suite, confirmation demo

- **File(s):** `docs/verify/2026-06-08-continuous-learning.md` (new).
- **Change (a) — quality gates (first enforcement this run):** `cd gatekeeper && cargo fmt --check` →
  exit 0; `cd gatekeeper && cargo clippy --all-targets -- -D warnings` → exit 0.
- **Change (b) — full suite:** `cd gatekeeper && cargo test` → green; record the new total (baseline 126
  plus the `learn` unit tests and the `cli_learn` cases) and the 2 ignored perf tests.
- **Change (c) — confirmation demo:** capture a gotcha, run `promote` once answering `n` (assert the
  operator file is absent), once with `--yes` (assert it exists and loads under its surface). Record both
  transcripts — this is the evidence for "promotion requires explicit human confirmation."
- **Change (d) — write the verify note:** the exact commands, their output (test counts, clippy/fmt clean,
  the two promote transcripts), and each acceptance criterion confirmed with a re-runnable command.
- **Test:** the verify note's commands are themselves the test; all exit 0 / green as stated.
- **Commit:** `test(learn): verify note — suite green, clippy/fmt clean, gated-promotion demo recorded`

<!-- No placeholder tokens anywhere in this plan; the plan gate rejects them. -->
