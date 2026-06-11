# Spec — hollow-pass kills + drift-proof CLI surface (v0.5.0)

**Status:** draft

Revision: 4 (2026-06-11) — rev-3 review polish: shadow-env no-op under enforced replay,
first-match-wins runner patterns, SIGKILL group kill, token-boundary allowlist matching,
widened malformed-directive shape; rev 3 resolved rev-2 blockers 1–6 and Q1–Q7 (see
*Resolved decisions*). (The revision marker lives here, not on the `Status:` line, so a future
rev bump cannot disturb the approval commit the human-commit check dogfoods.)

## Goal

Make doc/binary drift structurally unrepresentable (FM3) and close the three cheapest
hollow-artifact holes (FM2: verify, design, finish), with the seven-fixture adversarial suite
that defines "done" for the whole FM2 track. Ships as `v0.5.0`.

Grounding: [research](../research/2026-06-11-hollow-pass-kills.md), the
[remediation roadmap](../plans/2026-06-11-five-failure-modes-roadmap.md) (Phase 1), ROADMAP
Phase 14.

## Non-goals

- **No red-green replay, no entropy rules, no routing changes** — those are Phase 15 substance
  engines. The hollow fixtures (c) `assert!(true)` red commit (Phase 15), (d) "Looks fine"
  review (Phase 17), and (f) synonym-dodged plan (Phase 17) land `#[ignore]`-tagged and stay
  ignored this phase.
- **No default flips, without exception.** Every hardening ships default-off; default-mode gate
  runs never execute artifact-embedded commands (§3). The flip to enforcing is Phase 15, gated
  on shadow data.
- **No clap, no new dependencies** — runtime or dev. ADR-0007's four-dep constraint holds.
  (Unix process-group control uses std's stable `CommandExt::process_group`; no libc.)
- **No rewriting historical verify artifacts**, and **no numeric gate on history** — the legacy
  replay rate is *measured and recorded* as a baseline (§3), not asserted in advance.
- **No Go/jest test-count support this phase** — deferred until a reliable count source
  (`go test -json`) is specced; v0.5.0 ships cargo + pytest patterns plus a config escape
  hatch (§5).

## Shadow convention (applies to §3, §4, §5)

A hardened check that is **side-effect-free** is computed on every gate run; when its config
key is off, the result is emitted as a machine-readable line on stderr and does not affect the
exit code:

```
SHADOW {"gate":"verify","check":"replay","configured":"default","artifact":"docs/verify/x.md","command":null,"result":"static","detail":"2 evidence blocks, 3 commands, all allowlisted"}
```

- One JSON object per line. Fields (all always present): `gate`, `check`,
  `configured` (`"default"` | `"off"` | `"on"` | `"shadow-env"` — distinguishes never-configured
  from explicitly-opted-out so Phase 15 burn-in data is not polluted), `artifact` (path, or
  `null` for finish), `command` (string for per-command lines, else `null`),
  `result` (`"pass"` | `"fail"` | `"skip"` | `"static"`), `detail` (free text).
- **Per-check shadow semantics** (resolving rev-2 blocker 1 — execution is the dividing line):

| Check | Side effects | Default-mode gate run computes | `result` values |
|---|---|---|---|
| design substance floor (§4) | none (text) | full check | pass/fail |
| design approval provenance (§4) | none (git reads) | full check | pass/fail/skip (obstacle) |
| finish zero-test floor (§5) | none (parses output of a command that already ran) | full check | pass/fail |
| verify replay (§3) | **executes commands** | **static analysis only** — never executes: counts evidence blocks, parses/normalizes commands, checks allowlist & metachars | static |

- **Execution trigger for measurement** (restores what rev 2 deleted): the env var
  `GATEKEEPER_SHADOW=replay` makes `check verify` actually execute replay — evidence blocks
  *and* legacy extraction (§3) — emitting per-command `SHADOW` lines with real pass/fail,
  while the exit code remains presence-mode's. When `mode = "replay"` is already enforced,
  the env var is a **no-op**: replay executes and its enforcing exit code stands — the env
  var can never downgrade an enforcing project to presence exit codes (the invoking agent
  controls its own environment). This is the explicit, documented mechanism behind the
  baseline measurement (acceptance 7); it is never implied by a default run.
- **Config strictness:** a *known* key with an invalid value fails the owning gate (exit 2,
  actionable message). A `config.toml` that fails TOML parsing **fails the three hardened
  gates** (`check verify` / `check design` / `check finish`, exit 2) — warn-and-default there
  would silently revert `mode = "replay"` to off, the exact fail-open class this spec rejects;
  non-gate commands keep the existing warn-and-default (their unit test narrows to them).
  *Unknown* keys stay silently ignored (forward compatibility), and `doctor` lists
  unrecognized keys under the known tables to catch typos.

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
old-behavior; the suite tests the hardened mode, not the default). (e) and (g) are distinct
failure classes: unrecognized-summary vs recognized-summary-zero-count. The suite lands as the
**first code commit of the branch, all seven red-or-ignored** — it is the FM2 scoreboard;
un-ignoring is the definition of progress.

## 2. Dispatch table + ADR-0014 (FM3)

Replace the hand-rolled match (`main.rs:64-107`) and all nine `USAGE_*` constants with one
table:

```rust
struct SubcommandSpec {
    name: &'static str,        // "check verify" — two-word names allowed
    usage: &'static str,       // "gatekeeper check verify --feature <slug>"
    synopsis: &'static str,    // one-line description for help
    known_flags: &'static [&'static str],
    handler: fn(&[String]) -> i32,  // thin wrappers; roots/config loaded inside, as today
}
static SUBCOMMANDS: &[SubcommandSpec] = &[ /* every command, incl. each check gate */ ];
```

**Dispatch mechanics (Q7):** longest-prefix match — try `"{args[0]} {args[1]}"` against the
table first, then `args[0]`. Group-level behavior for `check`: bare `check` → group usage
(the check rows of the table) + exit 2; `check --help` → same text, exit 0; `check <unknown>`
→ error line + group usage, exit 2 — all matching today's observable behavior. Handlers are
thin wrapper fns adapting existing `cmd_*`/`gate_*` functions to the uniform signature;
they keep computing roots/config internally exactly as today.

**Behavioral contract (replaces rev-2's unsatisfiable byte-identity):** the instrument is
`cli_help_flags.rs` (exit codes + substring assertions) — it must stay green unmodified,
plus `cli_doc_sync.rs` (§6). Sanctioned, enumerated diffs vs today's output — recorded in the
verify artifact with before/after captures:
1. gate ordering in help normalized to table order (today `print_help()` and `USAGE_CHECK`
   disagree — verify/tdd swapped);
2. column padding normalized;
3. per-gate `--help` prints that gate's one-line usage instead of the full eight-line
   `USAGE_CHECK` block.
Anything outside this list is a regression. `check_help_or_unknown`'s contract — `Some(0)` on
help, `Some(2)` on unknown flag, `None` to proceed, scanning stopping at the first `--` — is
preserved exactly.

Decision recorded as **ADR-0014 "dispatch table over clap"** (four-dep constraint, ADR-0007).
Acceptance: `grep -c 'const USAGE' gatekeeper/src/main.rs` → **0**.

## 3. Verify gate — evidence replay (FM2, shadow)

New fenced-block format in verify artifacts:

````markdown
```evidence
$ cargo test --manifest-path gatekeeper/Cargo.toml --test cli_hollow
# expect: test result: ok
```
````

**Block grammar (exact):**

- A line starting `$ ` opens a step; its remainder is the command.
- Zero or more directly following directive lines bind to that step:
  `# expect: <text>` — **literal substring** match (`<text>` trimmed) against the step's
  combined output; `# expect-re: <regex>` — a `regex`-crate pattern, compiled with `(?m)`.
- **Malformed-directive rule (Q5):** inside an evidence block, any line matching
  `^#\s*[\w-]+\s*:` that is not a recognized directive (the widened shape also catches
  `# expect :`-style near-misses), and any directive line not directly following a step or
  another directive, makes the block **malformed** → gate fails in replay mode. Silent
  demotion to comment is the same downgrade class config strictness rejects. Lines not
  matching that shape are comments and ignored; the USER-GUIDE evidence-grammar section warns
  that `# <word>:`-shaped lines are **reserved** inside evidence blocks (an innocent
  `# note: flaky on CI` fails the gate — deliberate fail-closed). A block with no `$ ` line
  is malformed.
- Every step must exit 0 **and** satisfy all its expect lines.

**Execution model (argv, not shell):**

- The command is split on whitespace into argv and run via
  `Command::new(argv[0]).args(&argv[1..])` from the **project root** — no shell ever.
  Commands must therefore be repo-root-runnable; for this repo that means
  `cargo … --manifest-path gatekeeper/Cargo.toml` or `just …` (there is no root
  `Cargo.toml` — rev-2's own example violated this; fixed above).
- Fail-closed rejections (each fails the gate in replay mode): raw text containing any of
  `` | & ; < > $ ` \ ( ) " ' `` ; commands prefixed by `NAME=value` environment assignments;
  a command whose leading argv tokens do not match an `allowed_command_prefixes` entry; a
  step exceeding `replay_timeout_secs`.
- **Allowlist matching is token-boundary, not raw-prefix:** each allowlist entry is split on
  whitespace into tokens; a command is allowed iff its leading argv tokens equal some entry's
  tokens exactly (`cargo test` matches `cargo test --manifest-path …` but **not**
  `cargo testfoo`). Trailing whitespace in entries is irrelevant after tokenization.
- **Timeout kill (Q1, Unix-only):** the child is spawned with std's stable
  `CommandExt::process_group(0)`; on timeout the group is killed by spawning
  `kill -9 -- -<pid>` as argv (std-only, no libc; SIGKILL, not the default SIGTERM — a child
  that ignores TERM would still orphan). On non-Unix targets only the direct child is
  killed — documented residual.
- **Output capture:** stdout and stderr are drained by two reader threads and merged
  line-granular into one transcript (tee semantics where streaming applies, §5); expect
  patterns match this merged transcript with `(?m)` anchoring. Capture is tail-capped at
  1 MiB; when truncation occurred, any expect failure message says
  `(output truncated to last 1 MiB)`.

**Config:** `[verify] mode = "presence" | "replay"` (**default `presence`**),
`replay_timeout_secs = 300`, `allowed_command_prefixes = ["cargo test", "cargo run", "just",
"git diff", "git log", "git show", "git status"]` — the default `git` entries are read-only
subcommands (a bare `"git"` entry would admit `git push`); projects widen the list
deliberately. Matching is token-boundary (above).

**Replay-mode semantics: fail-closed.** Zero evidence blocks = fail (kills fixture b); a
malformed block = fail; no warn tier, no legacy carve-out in the gate.

**Legacy measurement (extraction, never enforcement):** under `GATEKEEPER_SHADOW=replay`,
artifacts without evidence blocks get commands extracted from any fenced block:
each `$ `-prefixed line, **normalized** by stripping a trailing annotation segment starting at
` →` or ` #` (rev-2 extraction shattered on `→ exit 0` / inline `# from repo root`
annotations); what still fails the metachar/allowlist screen is recorded as
`result:"skip"`, executable lines as per-command pass/fail. The baseline number this produces
is **recorded** in the verify artifact (acceptance 7) — it is not a pass/fail gate.
USER-GUIDE documents: replay re-executes evidence on every enforcing run — evidence commands
must be read-only/idempotent (Q3), which the read-only default allowlist enforces for git.

## 4. Design gate — substance floor + human-commit approval (FM2, shadow)

Two additions to `gate_design_approved`, both config-gated, both shadow-computed (they are
side-effect-free):

- **Substance floor**: `[design] substance_floor = true|false` (**default `false`**). When
  enforced: the spec must have ≥2 `## ` headings and ≥1 **body line** — non-empty after trim,
  not a heading, not the `Status:` line, not inside an HTML comment (exact predicate). Zero
  false positives on all 13 historical specs; kills fixture (a).
- **Approval provenance**: `[design] approval = "status-line" | "human-commit"` (**default
  `status-line`**). In `human-commit` mode:
  1. Read the spec **as committed**: `git show HEAD:<spec-path>`; locate the first line
     matching `spec_is_approved`'s normalization (first match wins, mirroring
     `spec_is_approved`). Computing the line number from the worktree file is the rev-2
     fail-open hole — uncommitted edits above the line silently retarget `log -L`.
  2. **Fail closed if the spec file has any uncommitted diff** (`git diff --quiet -- <path>`
     and `git diff --cached --quiet -- <path>`), is untracked, or the repo is shallow.
  3. `git log -L<n>,<n>:<spec-path> --format=%H` on the committed line; take the first
     commit; read trailers via `git show -s --format=%(trailers)`.
  4. Fail if any `Co-Authored-By:` value matches any pattern in
     `[design] agent_trailer_patterns` (regex list, default
     `["(?i)claude", "(?i)copilot", "(?i)cursor", "(?i)codex", "(?i)gemini", "(?i)devin",
     "(?i)aider", "(?i)\\[bot\\]"]`).
- **Git floor (Q4): require git ≥ 2.15** when `human-commit` is enforced — the binding
  constraints are `%(trailers)` (≥2.13; older git prints the format string literally, which
  would *silently pass*) and `--is-shallow-repository` (≥2.15), not `log -L`. `doctor` probes
  the version and each capability; **unparsable probe output is an obstacle**. Every obstacle
  (old git, shallow, untracked, dirty spec) fails closed when enforced, with a message naming
  the obstacle and the fix; under default config it logs `result:"skip"` instead.
- Honest residual (in the fail message and USER-GUIDE): this defends against sycophantic
  self-approval, not a malicious operator forging authorship.
- Dogfood note (amended at recorded maintainer direction, in-session 2026-06-11): the
  maintainer delegated the approval commit to the agent ("you have full control include git
  push or commit"). It therefore carries the honest agent trailer and serves as the
  **negative dogfood**: `human-commit` mode run against this spec's approval commit must
  *fail* with the agent-trailer message — demonstrating the check detects exactly the
  delegated-approval practice (Phase 13 precedent) it exists to reject. Positive-path
  validation lives in the scratch-repo fixtures (acceptance 5). The revision marker stays
  off the `Status:` line so later agent edits to the header cannot retarget the check
  (rev-2 fragility).

## 5. Finish gate — zero-test floor (FM2, shadow)

- Capture the test command's stdout+stderr while **streaming through** unchanged (two reader
  threads, line-granular tee — same merge semantics as §3). Retained capture tail-capped at
  1 MiB.
- Parse runner summaries on the merged transcript. Patterns are tried **in order,
  first-match-wins**: the first pattern with ≥1 match determines the count (summing across
  *its* matching lines — cargo prints one per test binary); later patterns are not consulted
  (cargo's `… 5 passed; 0 failed; … finished in 0.00s` would otherwise also match the pytest
  regex and double-count):
  1. cargo: `(?m)^test result: \w+\. (\d+) passed` → count = Σ captures
  2. pytest: `(?m)(\d+) passed[^\n]* in [0-9.]+s` — **no `===` fence anchor**, so `pytest -q`
    (the common CI invocation) parses too (Q2)
  3. `[finish] extra_count_patterns = ["<regex with one capture group>"]` — escape hatch,
    tried in listed order. **Nothing else in v0.5.0.** Go/jest deferred.
- Config: `[finish] require_test_count = true|false` (**default `false`**). When enforced:
  fail if the summed count is 0 **or** no pattern matched (fail-closed). Kills fixtures (e)
  and (g).
- **Scope: both invocation paths** — config `test_command` *and* explicit
  `gatekeeper check finish -- <cmd>` overrides; otherwise `-- true` bypasses the floor.
- Shadow: when the key is off, the computed count/recognition is emitted as a `SHADOW` line
  (`artifact: null`, `command` set).

## 6. README↔help sync test + release-path enforcement (FM3)

- `gatekeeper/tests/cli_doc_sync.rs` (zero new deps): spawn `gatekeeper --help`, extract the
  command list from the table (§2 makes this trivially well-formed).
- **Parsing scope (tight):** only backtick-quoted `` `gatekeeper …` `` spans inside (a) the
  `## Command reference` section of docs/USER-GUIDE.md (heading-delimited) and (b) the gate
  table in README.md (the table whose header row contains `Gate`). Fenced code blocks are out
  of scope; prose mentions elsewhere are out of scope.
- **Extraction grammar:** within a span, `<placeholder>` and `[optional]` tokens are
  normalized away; a literal `--` separator is kept; comparison is on subcommand words +
  required flag spellings.
- Assert: (1) every help command appears in USER-GUIDE's reference section, (2) every
  in-scope documented command parses against the table (no ghost commands), (3) flag
  spellings match.
- Wire into `ci.yml` `gate` job **and** `release.yml` `version-guard` job as
  `cargo test --manifest-path gatekeeper/Cargo.toml --test cli_doc_sync` (the runner image
  preinstalls cargo; the manifest path is required — there is no root `Cargo.toml`; accepts
  the ~1–2 min cold build at tag time) — the v0.4.0 escape class dies at the tag.

## Resolved decisions (maintainer reviews, 2026-06-11)

| # | Question | Resolution |
|---|----------|------------|
| D1 | dispatch table vs clap | **table** (ADR-0014) |
| D2 | legacy replay KPI | **no numeric gate on history** — baseline measured via `GATEKEEPER_SHADOW=replay` extraction+normalization and recorded; this phase's own new-format artifact must be 100% green (§3, acceptance 7) |
| D3 | substance floor always-on? | config-gated + shadow (`[design] substance_floor`, default false) |
| D4 | agent trailers | configurable regex denylist, multi-agent default (§4) |
| D5 | shadow mechanism | side-effect-free checks compute always + `SHADOW` JSONL; **execution requires explicit `GATEKEEPER_SHADOW=replay`** (§ shadow convention) — rev 2 deleted the trigger without replacement; restored scoped |
| D6 | legacy artifacts in replay mode | fail (zero blocks = fail); measurement ≠ enforcement |
| D7 | human-commit obstacles | fail closed when enforced (now incl. dirty/untracked spec, git < 2.15, unparsable probes); shadow logs `skip` |
| D8 | evidence commands | argv, no shell; metachar rejection; token-boundary allowlist (read-only git defaults); `process_group(0)` + spawned `kill -9 -- -<pid>` on timeout (Unix; non-Unix residual documented) |
| D9 | floor covers `finish -- <cmd>` | yes, both paths |
| D10 | Go test-count | deferred; cargo + pytest (`-q`-compatible) + `extra_count_patterns` |
| D11 | invalid config | known-key bad value → owning gate exits 2; **unparsable config.toml → all three hardened gates exit 2**; unknown keys ignored, doctor flags |

## Acceptance criteria

1. `cli_hollow.rs` lands red-first; at branch tip, fixtures (a), (b), (e), (g) un-ignored and
   green; (c), (d), (f) `#[ignore]`-tagged naming their killing phase (c → 15; d, f → 17).
2. `grep -c 'const USAGE' gatekeeper/src/main.rs` → 0; `cli_help_flags.rs` green
   **unmodified**; observed help diffs vs v0.4.1 fall entirely within §2's enumerated list
   (before/after captures in the verify artifact); ADR-0014 committed and linked from
   `docs/adr/README.md`.
3. `cli_doc_sync.rs` green and wired into both `ci.yml` and `release.yml` `version-guard`
   (with `--manifest-path`); a deliberately desynced doc line fails it (demonstrated, then
   reverted).
4. Default-mode `check verify` **executes nothing** (verified: a booby-trapped evidence
   command in a fixture leaves no side effect in presence mode). With `mode = "replay"`:
   passing tagged artifact passes; zero blocks fails; malformed directive fails;
   non-allowlisted, metachar-bearing, and env-assignment commands fail; a sleeping step
   times out, fails, and leaves no orphan process (Unix process-group kill).
5. With `[design] approval = "human-commit"`: agent-trailer approval fails; clean human
   commit passes (scratch-repo fixture); old-git / shallow / untracked / **dirty-spec**
   paths each fail closed with their specific message. With `substance_floor = true`:
   fixture (a) rejected. This spec's own approval commit — agent-executed at recorded
   maintainer direction, honest trailer — **fails** the check (negative dogfood, §4).
6. With `require_test_count = true`: `test_command = "true"`, a recognized zero-count
   summary, and an unrecognized runner all fail — via config **and** via `-- <cmd>`;
   `pytest -q`-style summaries parse; `extra_count_patterns` admits a custom runner; the real
   suite passes. Defaults unchanged.
7. `SHADOW` JSONL emitted for all **four** checks per the schema (incl. `configured` field);
   env-free default runs emit `result:"static"` for verify (no execution);
   the legacy baseline is produced by the documented `GATEKEEPER_SHADOW=replay` loop over
   `docs/verify/`, aggregated with the documented `jq` procedure, and **recorded** (numbers,
   no threshold) in this phase's verify artifact, whose own evidence blocks replay 100%
   green.
8. Config strictness: invalid known value → owning gate exit 2; unparsable `config.toml` →
   `check verify`/`design`/`finish` exit 2 (non-gate commands keep warn-and-default; their
   unit test narrowed accordingly); doctor lists unknown hardened-table keys, probes git
   version/capabilities/shallowness, and treats unparsable probe output as an obstacle.
9. `just check`, full `cargo test`, and `gatekeeper check docs` green; CHANGELOG `v0.5.0`;
   USER-GUIDE documents the three config tables, the evidence grammar, the
   read-only/idempotent-evidence requirement, the `SHADOW` schema + aggregation procedure,
   `GATEKEEPER_SHADOW=replay`, and the deferred-Go note.
