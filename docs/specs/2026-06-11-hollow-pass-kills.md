# Spec — hollow-pass kills + drift-proof CLI surface (v0.5.0)

**Status:** draft

## Goal

Make doc/binary drift structurally unrepresentable (FM3) and close the three cheapest
hollow-artifact holes (FM2: verify, design, finish), with the seven-fixture adversarial suite
that defines "done" for the whole FM2 track. Ships as `v0.5.0`.

Grounding: [research](../research/2026-06-11-hollow-pass-kills.md), the
[remediation roadmap](../plans/2026-06-11-five-failure-modes-roadmap.md) (Phase 1), ROADMAP
Phase 14.

## Non-goals

- **No red-green replay, no entropy rules, no routing changes** — those are Phase 15 substance
  engines. The hollow fixtures for them — (c) `assert!(true)` red commit, (d) "Looks fine"
  review, (f) synonym-dodged plan — land `#[ignore]`-tagged and stay ignored this phase.
- **No default flips.** All three gate hardenings ship default-off (old behavior). The flip to
  enforcing is Phase 15, gated on shadow-run data (<2% false-block on this repo's branches).
- **No clap, no new dependencies** — runtime or dev. ADR-0007's four-dep constraint holds.
- **No rewriting historical verify artifacts.** Replay-mode enforcement applies to artifacts
  opted in via the ` ```evidence ` tag; history is measured in shadow, not edited.

## 1. Hollow fixture suite — `gatekeeper/tests/cli_hollow.rs`

Seven adversarial fixtures, each a scratch framework root (+ `git init` where commit history
matters, reusing the `cli_check.rs`/`cli_review.rs` idiom), each asserting its gate **fails**:

| # | Fixture | Gate | Killed by | This phase |
|---|---------|------|-----------|------------|
| a | spec containing only `Status: approved` | design | substance floor (§4) | un-ignored |
| b | empty verify file | verify | replay fail-closed (§3) | un-ignored |
| c | `assert!(true)` test-only commit | tdd | Phase 15 red-green replay | `#[ignore]` |
| d | review body "Looks fine." | review | Phase 17 judge | `#[ignore]` |
| e | `test_command = "true"` | finish | zero-test floor (§5) | un-ignored |
| f | plan dodging the denylist with synonyms | plan | Phase 17 judge | `#[ignore]` |
| g | finish run executing zero tests | finish | zero-test floor (§5) | un-ignored |

Fixtures enable the new behavior explicitly in their scratch `config.toml` (the defaults stay
old-behavior; the suite tests the hardened mode, not the default). The suite lands as the
**first code commit of the branch, all seven red-or-ignored** — it is the FM2 scoreboard;
un-ignoring is the definition of progress.

## 2. Dispatch table + ADR-0014 (FM3)

Replace the hand-rolled match (`main.rs:64-107`) and all nine `USAGE_*` constants with one
table:

```rust
struct SubcommandSpec {
    name: &'static str,        // "check verify"
    usage: &'static str,       // "gatekeeper check verify --feature <slug>"
    synopsis: &'static str,    // one-line description for help
    known_flags: &'static [&'static str],
    handler: fn(&[String]) -> i32,
}
static SUBCOMMANDS: &[SubcommandSpec] = &[ /* every command, incl. each check gate */ ];
```

- `main()` dispatch, `print_help()`, per-command `--help`, and unknown-flag errors all iterate
  the same table; the same string can no longer exist in two states.
- `check_help_or_unknown`'s exact 0/1/2 contract is preserved; `cli_help_flags.rs` is the
  characterization net — run before/after, output byte-identical (one sanctioned exception: the
  `check` usage line corrected by `39710a0`).
- Decision recorded as **ADR-0014 "dispatch table over clap"**: clap would collapse the four
  copies too, but costs a dependency tree against ADR-0007's four-dep constraint; the table is
  ~100 LOC of std.
- Acceptance: `grep -c 'pub const USAGE\|const USAGE' gatekeeper/src/main.rs` → **0**.

## 3. Verify gate — evidence replay (FM2, shadow)

New fenced-block format in verify artifacts (codifies the 9/12 majority `$ command` practice):

````markdown
```evidence
$ cargo test --test cli_hollow
# expect: test result: ok
```
````

- Parser: lines starting `$ ` are commands; following `# expect: <substring-or-regex>` lines
  must match the combined stdout+stderr; command must exit 0.
- Config: `[verify] mode = "presence" | "replay"` (**default `presence`** — current behavior),
  `replay_timeout_secs = 300`, `allowed_command_prefixes = ["cargo ", "just ", "git "]`.
- **Fail-closed**: in replay mode, a command outside the allowlist fails the gate (an artifact
  must not become an arbitrary-execution vector); an artifact with **zero** evidence blocks
  also fails (kills fixture b). Commands run from the project root with the timeout.
- Shadow: `GATEKEEPER_SHADOW=1` runs replay in presence mode too, logging
  `SHADOW-FAIL`/`SHADOW-PASS` per block to stderr without affecting the exit code — this is the
  burn-in data source for the Phase 15 flip.

## 4. Design gate — substance floor + human-commit approval (FM2, shadow)

Two additions to `gate_design_approved`:

- **Substance floor (always on)**: the spec must have ≥2 `## ` headings and ≥1 non-empty body
  line outside the `Status:` line. Zero false positives on all 13 historical specs (min is 4
  headings); kills fixture (a).
- **Approval provenance (config)**: `[design] approval = "status-line" | "human-commit"`
  (**default `status-line`** — current behavior). In `human-commit` mode: find the commit that
  last touched the `Status:` line (`git log -L<n>,<n>:<spec-path> --format=%H`, first hit),
  read its trailers (`git show -s --format=%(trailers)`), and **fail if any
  `Co-Authored-By:` trailer matches `(?i)claude`** — approval must be flipped in a commit
  carrying no agent co-author. Honest residual (documented in the gate's fail message and
  USER-GUIDE): this defends against sycophantic self-approval, not a malicious operator forging
  authorship.
- Portability: `git log -L` needs git ≥1.8.4 — `doctor` gains a capability probe; on failure
  the gate falls back to `status-line` with a stderr warning rather than hard-failing.
- Note: Phase 13's own approval commit carries the agent trailer (executed at the maintainer's
  direction). Under `human-commit` it would fail — correct per the threat model; from this
  phase on, approval commits are made by the human directly (this spec's approval is the first
  test of that practice).

## 5. Finish gate — zero-test floor (FM2, shadow)

- Capture the test command's stdout+stderr (today only the exit code is read).
- Parse runner summaries: `cargo test` `(\d+) passed`, `pytest` `(\d+) passed`, `go test`
  `ok\s+\S+`, jest `Tests:\s+.*(\d+) passed` — summing across multiple summary lines (cargo
  prints one per test binary).
- Config: `[finish] require_test_count = true|false` (**default `false`** — current behavior).
  When true: fail if the summed executed-test count is 0 **or** no summary line is recognized
  (fail-closed: an unrecognized runner must be added to the patterns, not waved through).
  Kills fixtures (e) and (g).
- Shadow: under `GATEKEEPER_SHADOW=1`, the count check logs `SHADOW-FAIL` without failing.

## 6. README↔help sync test + release-path enforcement (FM3)

- `gatekeeper/tests/cli_doc_sync.rs` (zero new deps): spawn `gatekeeper --help`, extract the
  command list from the table (§2 makes this trivially well-formed); parse every
  backtick-quoted `` `gatekeeper …` `` command from README.md and docs/USER-GUIDE.md command
  tables; assert (1) every help command appears in USER-GUIDE, (2) every documented command
  parses against the table (no ghost commands), (3) flag spellings match.
- Wire into `ci.yml` `gate` job **and** `release.yml` `version-guard` job (checkout + rust
  already present there; add `cargo test --test cli_doc_sync`) — the v0.4.0 escape class
  (`39710a0` stranded off the tag) now fails *at the tag*.

## Decisions for the maintainer

| # | Decision | Recommendation |
|---|----------|----------------|
| D1 | ADR-0014: dispatch table over clap | **table** — four-dep constraint intact; clap solves nothing the table doesn't |
| D2 | Replay KPI measurement: roadmap says "≥90% of existing verify artifacts replay green" but 2/12 artifacts are narrative-only and ~30% of raw commands are network/env-bound. Restate as: ≥90% of **allowlisted-prefix commands extracted from existing artifacts** replay green in shadow (research shows this filter lands ≈90-100%) | **restate** — measured over allowlisted commands; whole-artifact enforcement applies only to the new tagged format |
| D3 | Substance floor always-on vs config-gated | **always-on** — zero historical false positives; gating it would leave fixture (a) alive in default mode |
| D4 | `Co-Authored-By` match scope: any agent trailer vs `(?i)claude` only | **`(?i)claude` in any Co-Authored-By value** — matches the trailer this repo actually emits; widen later if other agents appear |
| D5 | Shadow mechanism: `GATEKEEPER_SHADOW=1` env (roadmap) vs config-only | **env var** — lets this repo's CI/branches burn in without touching per-project config defaults |

## Acceptance criteria

1. `cli_hollow.rs` lands red-first (branch history: fixtures commit precedes fixes); at branch
   tip, fixtures (a), (b), (e), (g) un-ignored and green (rejected by their gates); (c), (d),
   (f) `#[ignore]`-tagged with a comment naming the phase that kills them.
2. `grep -c 'const USAGE' gatekeeper/src/main.rs` → 0; `cli_help_flags.rs` byte-identical
   before/after the dispatch refactor (modulo the corrected `check` usage line); ADR-0014
   committed and linked from `docs/adr/README.md`.
3. `cli_doc_sync.rs` green and present in both `ci.yml` and `release.yml` `version-guard`.
4. With `[verify] mode = "replay"`: a tagged artifact with passing evidence blocks passes; zero
   evidence blocks fails; a non-allowlisted command fails. Default mode unchanged
   (presence) — full existing test suite untouched.
5. With `[design] approval = "human-commit"`: an agent-trailer approval commit fails the gate
   with an actionable message; a clean human commit passes. This spec's own approval commit
   passes the check (dogfood).
6. With `[finish] require_test_count = true`: `test_command = "true"` and a zero-test
   `cargo test --test nonexistent`-style run fail; the real suite passes. Defaults unchanged.
7. Shadow run (`GATEKEEPER_SHADOW=1`) over this repo's artifacts logs replay results for
   allowlisted commands with ≥90% green (D2 restated KPI), recorded in the verify artifact.
8. `just check`, full `cargo test`, and `gatekeeper check docs` green; CHANGELOG `v0.5.0`
   entry; USER-GUIDE documents the three new config keys, the evidence format, and the shadow
   env var.
