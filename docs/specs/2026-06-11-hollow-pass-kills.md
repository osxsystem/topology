# Spec — hollow-pass kills + drift-proof CLI surface (v0.5.0)

**Status:** draft (rev 2 — maintainer review blockers addressed)

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
- **No default flips, without exception.** Every hardening in this spec — including the design
  substance floor — ships default-off (old behavior) and computes in shadow. The flip to
  enforcing is Phase 15, gated on shadow-run data (<2% false-block on this repo's branches).
- **No clap, no new dependencies** — runtime or dev. ADR-0007's four-dep constraint holds.
- **No rewriting historical verify artifacts.** Replay-mode *enforcement* reads only
  ` ```evidence `-tagged blocks; the shadow *measurement* over legacy artifacts extracts
  commands without editing them (§3).
- **No Go/jest test-count support this phase** — deferred until a reliable count source
  (`go test -json`) is specced; v0.5.0 ships cargo + pytest patterns plus a config escape
  hatch (§5).

## Shadow convention (applies to §3, §4, §5)

Every hardened check is **computed on every gate run** but only **enforced** when its config
key enables it. When not enforced, the result is emitted as a machine-readable shadow line on
stderr and does not affect the exit code:

```
SHADOW {"gate":"verify","check":"replay","artifact":"docs/verify/…","result":"fail","detail":"zero evidence blocks"}
```

One JSON object per line, fields `gate`/`check`/`artifact`/`result`/`detail` — greppable
(`grep ^SHADOW`) and aggregatable (`jq`). `GATEKEEPER_SHADOW=1` is *not* a mode switch; shadow
emission is unconditional when the key is off, so this repo's branches accumulate burn-in data
with zero setup. The Phase 15 default-flip decision reads these lines.

**Config strictness:** a *known* key with an invalid value (e.g. `mode = "replya"`) **fails the
gate that owns it** (exit 2, actionable message) — warn-and-default would silently downgrade
enforcement, the same fail-open class this review rejected. *Unknown* keys stay silently
ignored (forward compatibility, current behavior), but `doctor` gains a check listing
unrecognized keys under known tables to catch typos.

## 1. Hollow fixture suite — `gatekeeper/tests/cli_hollow.rs`

Seven adversarial fixtures, each a scratch framework root (+ `git init` where commit history
matters, reusing the `cli_check.rs`/`cli_review.rs` idiom), each asserting its gate **fails**:

| # | Fixture | Gate | Killed by | This phase |
|---|---------|------|-----------|------------|
| a | spec containing only `Status: approved` | design | substance floor (§4) | un-ignored |
| b | empty verify file | verify | replay fail-closed (§3) | un-ignored |
| c | `assert!(true)` test-only commit | tdd | Phase 15 red-green replay | `#[ignore]` |
| d | review body "Looks fine." | review | Phase 17 judge | `#[ignore]` |
| e | `test_command = "true"` (no recognizable summary) | finish | zero-test floor (§5) | un-ignored |
| f | plan dodging the denylist with synonyms | plan | Phase 17 judge | `#[ignore]` |
| g | runner emitting a **recognized summary with zero tests** (`test result: ok. 0 passed`) | finish | zero-test floor (§5) | un-ignored |

Fixtures enable the hardened behavior explicitly in their scratch `config.toml` (defaults stay
old-behavior; the suite tests the hardened mode, not the default). (e) and (g) are now distinct
failure classes: unrecognized-summary vs recognized-summary-zero-count. The suite lands as the
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

**Block grammar (exact):**

- A line starting `$ ` opens a step; its remainder is the command.
- Zero or more directly following `# expect: <text>` lines: **literal substring** match
  (leading/trailing whitespace of `<text>` trimmed) against the step's combined stdout+stderr.
- `# expect-re: <regex>` — same, but a `regex`-crate pattern.
- Every step must exit 0 **and** satisfy all its expect lines. Any other line inside the block
  is a comment and ignored. A block with no `$ ` line is malformed → gate fails in replay mode.

**Execution model (argv, not shell):**

- The command is split on whitespace into argv and run via
  `Command::new(argv[0]).args(&argv[1..])` from the project root — **no shell ever**.
- Fail-closed rejections (each fails the gate in replay mode): a command whose raw text
  contains any of `| & ; < > $ \` \ ( ) " '` or `=` -prefixed env assignments; a command whose
  raw text does not start with an `allowed_command_prefixes` entry; a step exceeding
  `replay_timeout_secs` (process killed via wait-with-timeout polling, then `kill()`; the
  child is spawned in its own process group so the kill reaches grandchildren).
- Output capture per step is capped at 1 MiB (tail-biased: keep the last 1 MiB — runner
  summaries print last); the cap is also the expect-match window.

**Config:** `[verify] mode = "presence" | "replay"` (**default `presence`** — current
behavior), `replay_timeout_secs = 300`, `allowed_command_prefixes = ["cargo ", "just ",
"git "]`.

**Replay-mode semantics for artifacts without evidence blocks: fail.** Zero blocks = fail
(kills fixture b); there is no warn tier and no legacy carve-out in the gate itself — a project
that flips to replay accepts that re-checking an old feature requires either tagging its
artifact or passing presence mode for that run. Legacy compatibility is handled by the
*measurement*, not the gate: the shadow line for an untagged artifact reports commands
extracted from any fenced block whose `$ `-prefixed lines match the allowlist (measurement
only, never enforcement) — this is what the D2 KPI aggregates.

## 4. Design gate — substance floor + human-commit approval (FM2, shadow)

Two additions to `gate_design_approved`, **both config-gated, both shadow-computed always**
(resolving the rev-1 contradiction — nothing in this spec is always-on):

- **Substance floor**: `[design] substance_floor = true|false` (**default `false`**). When
  enforced: the spec must have ≥2 `## ` headings and ≥1 non-empty body line outside the
  `Status:` line. Zero false positives on all 13 historical specs (min is 4 headings); kills
  fixture (a). Expected to be the safest early flip in Phase 15 given the zero-FP history.
- **Approval provenance**: `[design] approval = "status-line" | "human-commit"` (**default
  `status-line`**). In `human-commit` mode: find the commit that last touched the `Status:`
  line (`git log -L<n>,<n>:<spec-path> --format=%H`, first hit), read its trailers
  (`git show -s --format=%(trailers)`), and fail if any `Co-Authored-By:` value matches any
  pattern in `[design] agent_trailer_patterns` (regex list, **default**
  `["(?i)claude", "(?i)copilot", "(?i)cursor", "(?i)codex", "(?i)gemini", "(?i)devin",
  "(?i)aider", "(?i)\\[bot\\]"]`) — configurable denylist, seeded with the known agent
  ecosystem, not just this repo's own trailer.
- **Fail-closed, no silent downgrade**: when `human-commit` is *enforced* and the check cannot
  run — git lacks `log -L` (<1.8.4), the repo is shallow (`git rev-parse
  --is-shallow-repository`), the spec is untracked, or the `Status:` line has uncommitted
  modifications — the gate **fails** with a message naming the obstacle and the fix
  (unshallow / commit the flip / upgrade git / set `approval = "status-line"` deliberately).
  `doctor` probes all of these ahead of time so the failure is never a surprise. (Shadow
  computation under default config logs the obstacle as `"result":"skip"` instead.)
- Honest residual (documented in the fail message and USER-GUIDE): this defends against
  sycophantic self-approval, not a malicious operator forging authorship.
- Note: Phase 13's own approval commit carries the agent trailer (executed at the maintainer's
  direction). Under `human-commit` it would fail — correct per the threat model; from this
  phase on, approval commits are made by the human directly (this spec's approval is the first
  test of that practice).

## 5. Finish gate — zero-test floor (FM2, shadow)

- Capture the test command's stdout+stderr while **streaming it through** to the caller
  unchanged (tee semantics — today's UX is real-time output; that stays). Retained capture is
  tail-capped at 1 MiB, same rationale as §3.
- Parse runner summaries, **summing across all matching lines** (cargo prints one per test
  binary):
  - cargo: `^test result: \w+\. (\d+) passed` → count = Σ captures
  - pytest: `(?:^|\s)(\d+) passed` on the final summary line (`=+ .* =+`)
  - **Nothing else in v0.5.0.** Go is deferred (no reliable count without `go test -json`);
    jest likewise. Escape hatch: `[finish] extra_count_patterns = ["<regex with one capture
    group>"]` lets a project add its runner without waiting for a release.
- Config: `[finish] require_test_count = true|false` (**default `false`**). When enforced:
  fail if the summed count is 0 **or** no summary pattern matched (fail-closed — an
  unrecognized runner must be added via `extra_count_patterns`, not waved through). Kills
  fixtures (e) and (g).
- **Scope: both invocation paths.** The floor applies to config `test_command` *and* to
  explicit `gatekeeper check finish -- <cmd>` overrides — otherwise `-- true` bypasses the
  floor and the hardening is theater.
- Shadow: when the key is off, the computed count/recognition result is emitted as a `SHADOW`
  line per the convention above.

## 6. README↔help sync test + release-path enforcement (FM3)

- `gatekeeper/tests/cli_doc_sync.rs` (zero new deps): spawn `gatekeeper --help`, extract the
  command list from the table (§2 makes this trivially well-formed).
- **Parsing scope (tight):** only backtick-quoted `` `gatekeeper …` `` strings inside (a) the
  `## Command reference` section of docs/USER-GUIDE.md (heading-delimited) and (b) the gate
  table in README.md (the table whose header row contains `Gate`). Prose mentions of
  `gatekeeper` elsewhere are out of scope — the test guards the reference surface, not every
  sentence.
- Assert: (1) every help command appears in USER-GUIDE's reference section, (2) every
  documented command in scope parses against the table (no ghost commands), (3) flag spellings
  match.
- Wire into `ci.yml` `gate` job **and** `release.yml` `version-guard` job (checkout + rust
  already present there; add `cargo test --test cli_doc_sync`) — the v0.4.0 escape class
  (`39710a0` stranded off the tag) now fails *at the tag*.

## Resolved decisions (maintainer review, 2026-06-11)

| # | Question | Resolution |
|---|----------|------------|
| D1 | dispatch table vs clap | **table** (ADR-0014); four-dep constraint intact |
| D2 | replay KPI over legacy artifacts | ≥90% measured over **allowlisted commands extracted in shadow** from any fenced block; enforcement reads only tagged blocks (§3) |
| D3 | substance floor always-on? | **No — config-gated + shadow like everything else** (`[design] substance_floor`, default false); "no default flips" holds without exception (§4) |
| D4 | which trailers are agent-authored | **configurable regex denylist** `agent_trailer_patterns`, default seeded with known agents, not Claude-only (§4) |
| D5 | shadow mechanism | **unconditional machine-readable `SHADOW` JSONL on stderr** when a key is off; no env var needed (supersedes rev-1's `GATEKEEPER_SHADOW`) |
| D6 | legacy artifacts in replay mode | **fail** (zero blocks = fail); no warn tier; legacy handled by measurement, not enforcement (§3) |
| D7 | human-commit on old git / shallow / uncommitted spec | **fail closed** when enforced; doctor pre-probes; shadow logs `skip` (§4) |
| D8 | evidence commands shell or argv | **argv, no shell ever**; metacharacters rejected fail-closed; process-group kill on timeout; 1 MiB tail-capped capture (§3) |
| D9 | does the floor cover `finish -- <cmd>` overrides | **yes, both paths** (§5) |
| D10 | Go test-count support | **deferred**; cargo + pytest built-in, `extra_count_patterns` escape hatch (§5) |
| D11 | invalid known config values | **fail the owning gate** (exit 2, actionable); unknown keys stay ignored, doctor flags typos (shadow convention §) |

## Acceptance criteria

1. `cli_hollow.rs` lands red-first (branch history: fixtures commit precedes fixes); at branch
   tip, fixtures (a), (b), (e), (g) un-ignored and green (rejected by their gates); (c), (d),
   (f) `#[ignore]`-tagged with a comment naming the phase that kills them.
2. `grep -c 'const USAGE' gatekeeper/src/main.rs` → 0; `cli_help_flags.rs` byte-identical
   before/after the dispatch refactor (modulo the corrected `check` usage line); ADR-0014
   committed and linked from `docs/adr/README.md`.
3. `cli_doc_sync.rs` green and present in both `ci.yml` and `release.yml` `version-guard`;
   a deliberately desynced doc line fails it (demonstrated in the verify artifact, reverted).
4. With `[verify] mode = "replay"`: a tagged artifact with passing evidence blocks passes;
   zero evidence blocks fails; a non-allowlisted or metacharacter-bearing command fails; a
   step exceeding the timeout fails and leaves no orphan process. Default mode unchanged
   (presence) — full existing test suite untouched.
5. With `[design] approval = "human-commit"`: an agent-trailer approval commit fails with an
   actionable message; a clean human commit passes; old-git/shallow/uncommitted-spec paths
   fail closed with their specific messages. This spec's own approval commit passes the check
   (dogfood). With `[design] substance_floor = true`: fixture (a) rejected.
6. With `[finish] require_test_count = true`: `test_command = "true"`, a recognized
   zero-count summary, and an unrecognized runner all fail — via config `test_command` **and**
   via `-- <cmd>` override; the real suite passes; `extra_count_patterns` admits a custom
   runner. Defaults unchanged.
7. Shadow `SHADOW` JSONL lines emitted for all three checks when keys are off; aggregation
   over this repo's existing `docs/verify/` artifacts shows ≥90% green on allowlisted
   commands (D2 KPI), recorded in the verify artifact.
8. Invalid known config values fail their gate with exit 2; doctor lists unknown keys under
   `[verify]`/`[design]`/`[finish]` and probes git capability/shallowness.
9. `just check`, full `cargo test`, and `gatekeeper check docs` green; CHANGELOG `v0.5.0`
   entry; USER-GUIDE documents the three config tables, the evidence grammar, the SHADOW line
   format, and the deferred-Go note.
