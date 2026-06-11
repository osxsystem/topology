# Research — hollow-pass kills + drift-proof CLI surface (Phase 14, v0.5.0)

## Problem

The v0.4.0 adversarial audit showed two structural weaknesses beyond scan coverage
([remediation roadmap](../plans/2026-06-11-five-failure-modes-roadmap.md), Phase 1; ROADMAP
Phase 14):

- **FM3 — doc/binary drift.** Help text, README, USER-GUIDE, and the dispatch match are four
  hand-maintained copies of the same command surface. The v0.4.0 escape (`39710a0`, a usage fix
  stranded off the tag) was this class: nothing diffs the copies, and nothing runs at the tag.
- **FM2 — hollow artifacts.** The verify, design, and finish gates check *existence and
  sequence*, not substance: an empty verify file, a spec containing only `Status: approved`,
  and `test_command = "true"` all pass today.

This note grounds the Phase 14 fixes on the current tree (verified 2026-06-11, post-v0.4.1).

## CLI dispatch surface today (FM3)

- **Nine `USAGE_*` constants** in `gatekeeper/src/main.rs:227-265`: `ACTIVATE`, `LIST`,
  `CHECK` (shared by all eight `check <gate>` variants), `SCAN`, `INSTINCT`, `ADAPT`, `LEARN`,
  `MEMORY`, `DOCTOR`. Five are `pub(crate)` and reused by their modules.
- **Top-level dispatch** is a hand-rolled match on `args.first()` (`main.rs:64-107`): nine
  subcommand arms plus `--version`/`-V`, `--help`/`-h`/empty, and an unknown-command arm
  (exit 2). Only `check` has nested dispatch in `main.rs` itself (`cmd_check`,
  `main.rs:478-582`, eight gate arms); `scan`/`instinct`/`adapt`/`learn`/`memory` delegate to
  their modules' own arg handling.
- **Global help** is *not* a constant: `print_help()` (`main.rs:111-140`) assembles a separate
  multi-line literal, calling `version::tool()` (= `env!("CARGO_PKG_VERSION")`) and
  `version::rules_schema()` at runtime. So the same command line exists in up to four places:
  `print_help()`, a `USAGE_*` constant, README.md, USER-GUIDE.md.
- **Flag hygiene helper**: `check_help_or_unknown(sub, args, known_flags, usage) -> Option<i32>`
  (`main.rs:267-299`) — scans args up to the first `--`, returns `Some(0)` on `--help`/`-h`,
  `Some(2)` on an unknown `-`-prefixed flag, `None` to proceed. Every subcommand calls it; a
  dispatch table must preserve this exact 0/1/2 exit contract.
- **Characterization net**: `gatekeeper/tests/cli_help_flags.rs` (~360 LOC) pins `--help`/`-h`
  and unknown-flag behavior for every subcommand via raw `std::process::Command` +
  `env!("CARGO_BIN_EXE_gatekeeper")` (no assert_cmd; the suite uses no test-only crates).

## Where the command surface is documented (sync-test targets)

- **README.md**: quick-start examples (~lines 52-65) and the gate table (~56-64) with
  `gatekeeper check finish -- <cmd>` syntax.
- **docs/USER-GUIDE.md "Command reference"** (~lines 293-442): markdown tables, one command per
  row, backtick-quoted (`| `gatekeeper list` | what it does |`); separate tables for gates
  (`Gate | Command | Passes when`), scan, instincts, adapt, learn, memory, doctor. The
  backtick-quoted `gatekeeper …` cell is mechanically extractable with the existing `regex` dep.
- **CI**: `ci.yml` has two jobs — `gate` (just check + `cargo run -- check docs`) and `network`
  (non-blocking deny/links). **`release.yml`** has `version-guard` (tag vs `Cargo.toml` vs
  `plugin.json` vs `marketplace.json` — versions only, no doc-content check), then
  matrix `build`, `payload`, `release`. The doc-sync test must run in `gate` and in
  `version-guard` to die at the tag.

## Gate internals today (FM2)

- **Verify gate** (`main.rs:520-527` → `gate_doc_exists`): passes iff any
  `docs/verify/*<slug>*.md` exists. Content is never read — an empty file passes (hollow
  fixture b).
- **Design gate** (`gate_design_approved`, `main.rs:815-840` + `spec_is_approved`,
  `main.rs:754-790`): file exists + any line normalizing to `status: approved`. A one-line spec
  passes (fixture a). No git inspection — the approving commit's authorship is invisible
  (Phase 13's approval *was* executed by the agent at the maintainer's direction; the trailer
  records this honestly).
- **Finish gate** (`gate_finish`, `main.rs:870-927`): runs the CLI-override command via direct
  `Command::new(cmd[0])` (no shell) or config `test_command` via `sh -c`; checks **exit code
  only** — output is not captured. `test_command = "true"` passes (fixture e); a run that
  executes zero tests passes (fixture g).
- **Git idiom** (for the human-commit check): `review.rs:188-200` —
  `Command::new("git").arg("-C").arg(root).args(...)` returning `Option<String>`, output
  `trim_end()`ed. `tdd.rs` and `scan.rs` use the same shape. Nothing currently reads commit
  authors or trailers; `git log -L<start>,<end>:<path>` / `--format=%(trailers)` would be new
  ground (older-git portability flagged in the roadmap → doctor probe + fallback).
- **Config** (`config.rs`): `<artifacts_root>/config.toml`, parsed with the `toml` crate
  (one of the four deps: regex, serde, serde_json, toml — ADR-0007). Two keys today:
  `base_branch`, `test_command`. Unknown keys silently ignored; missing/malformed file →
  defaults, never a gate failure. New `[verify]`/`[design]`/`[finish]` tables extend this file;
  the silent-ignore behavior means old binaries tolerate new keys.
- **Warn-without-failing idiom** (shadow precedent): `scan.rs` `Severity::Warn` findings print
  `WARN …` to stderr and do not affect the exit code (`report()`, `scan.rs:308-328`).

## Verify-artifact replayability survey (all 12 docs/verify/ files read)

- 12/12 use fenced code blocks; roughly half consistently prefix commands with `$ `
  (initial survey said 9/12; the rev-2 review re-measured 6/12 under a stricter reading —
  either way it is a plurality practice, not a standard); 11/12 annotate expected exit as
  `→ exit N`; 10/12 use `# …` lines for expected output. No language tag on most blocks. Two
  artifacts (installer-v2, one-command-install) are narrative-only — no mechanically
  replayable blocks at all.
- **No format is prescribed anywhere**: `skills/verify-before-done/SKILL.md` demands "a command
  they can re-run and an output they can see" but defines no block syntax. The ` ```evidence `
  format codifies a 9/12 majority practice; it does not match all history.
- Determinism: of ~83 commands across the 12 artifacts, a majority *look* re-runnable
  (cargo/git/just); the failures cluster in network calls (GitHub API, curl), hardcoded dates,
  `mktemp` absolute paths, pinned old versions, and tool-presence assumptions (claude CLI,
  codex). **Caution (rev-2 review, verified):** naive extraction lands far lower (~19-69%
  depending on normalization) — there is no root `Cargo.toml`, so bare `cargo test …` fails
  from the repo root (`--manifest-path gatekeeper/Cargo.toml` required), and inline
  annotations (`→ exit 0`, trailing `# …`) shatter into bogus argv without a stripping pass.
- Implication: no numeric KPI can honestly be asserted over history in advance. The spec
  instead *measures and records* the legacy baseline (extraction + annotation-stripping
  normalization, explicit shadow-replay trigger) and gates only the new-format artifacts this
  phase itself produces; replay-mode enforcement applies only to artifacts written in the
  codified format from v0.5.0 on.

## Spec substance floor (fixture a)

All 13 existing `docs/specs/*.md` have ≥4 `## ` section headings (min 4, median 7-8); 10/13
have a `## Goal`. A floor of "≥2 `## ` headings + ≥1 non-empty body line outside the Status
line" rejects the approved-only spec while passing every spec ever written here — it codifies
practice with zero false positives on history.

## Test/fixture idiom to reuse

`cli_check.rs` / `cli_scan_bench.rs` pattern: `scratch_root(tag)` builds a minimal framework
root in `temp_dir()/topo_<tag>_<pid>` (`skills/`, `AGENTS.md`, `security/rules.toml`);
`run(cwd, args)` / `run_stdin(...)` wrap `std::process::Command`. Hollow fixtures additionally
need `git init` scratch repos (precedent in `cli_review.rs`). Per-fixture `#[ignore]` tags are
the scoreboard idiom: un-ignore as each fix lands.

## Constraints carried into the design

- **Four-dependency constraint holds** (ADR-0007): the dispatch table is hand-rolled
  (`static SUBCOMMANDS: &[SubcommandSpec]`), no clap — recorded as ADR-0014. The doc-sync test
  uses `std::process::Command` + the existing `regex` dep, zero new deps.
- **Fail-closed replay**: evidence blocks execute only allowlisted command prefixes; a
  non-allowlisted command fails the gate rather than being skipped silently (same posture as
  `security-scan.sh`).
- **Shadow-first**: all gate hardenings ship default-off (`presence` / `status-line` /
  `substance_floor = false` / `require_test_count = false`). Side-effect-free checks compute
  on every run and emit machine-readable `SHADOW` JSONL when their key is off; replay
  *execution* is never implicit — it requires the explicit `GATEKEEPER_SHADOW=replay`
  trigger. This data feeds the <2% false-block bar that gates the v0.6.0 default flip
  (Phase 15 dependency).
