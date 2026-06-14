# Design: C6 — doctor check for the Co-Authored-By × approval_provenance trailer collision

- **Date:** 2026-06-14
- **Feature slug:** c6-trailer-collision
- **Status:** approved
- **Research:** `docs/research/2026-06-14-c6-trailer-collision.md`

> **Approval provenance note.** This repo runs `[design] approval = "status-line"` (default; no
> `config.toml`), so the `approval_provenance` check is **shadow** here and does not block. Approval is
> recorded per the maintainer's standing autonomy grant for the portability-first experiment (slice #2),
> as in slice #1. The design below was pressure-tested by a 3-evaluator panel (field-report,
> security/integrity, and staff-engineer-simplicity lenses), which converged unanimously on every
> decision axis.

## Problem

The design gate's `approval_provenance` check (`main.rs:1478-1762`) FAILs iff the commit that last
touched a spec's `Status: approved` line carries a `Co-Authored-By:` trailer whose value matches
`design_agent_trailer_patterns` (default `(?i)claude`, …). Its purpose: deter sycophantic agent
self-approval. But the agent harness carries a standing, system-prompt-injected rule — *"End git commit
messages with: Co-Authored-By: Claude …"* — that stamps that exact trailer on **every** commit. A human
who approves a design under `approval = "human-commit"` then has their approval commit read as an agent
self-approval and **blocked**, with a message that does not hint at the harness rule as the cause. This
is a real, unflagged **policy contradiction** (the report's "C6"): two correct-looking policies that
cannot both hold on the approval commit.

The collision is **live** (all recent commits here carry the Claude trailer) but currently **shadow**
(default `status-line` mode). The roadmap plans to flip the default to `human-commit`
(`docs/plans/2026-06-11-five-failure-modes-roadmap.md:110`), at which point it becomes blocking for
everyone. The fix is a discrete, advisory **`gatekeeper doctor` check** that detects the collision and
explains it before a developer hits the baffling FAIL.

## Decision

Add one **advisory `doctor` probe** (`WARN`, never affects exit code) that detects the collision from
the **git history** — the only on-disk evidence of the harness rule.

### Why git-history (and not the alternatives)

| Strategy | Detects canonical harness-injected setup? | Verdict |
|---|---|---|
| **git-history** (sample recent commit trailers) | **Yes** — observes the *effect* (trailers actually landing) | **chosen** |
| file-scan (grep instruction files) | **No** — rule is system-prompt-only, in no file → false-negatives the exact case that bit the dev | rejected |
| config-only (WARN whenever `human-commit` set) | Yes, but with zero evidence → cries wolf on clean human-only repos | rejected as primary |
| check-existing-specs (re-run gate resolution) | No on a fresh project (no approved spec yet) — blind in the prevention window | rejected as primary |

The decisive constraint (research §2): the colliding rule is **harness-injected**, present in **no file**
on disk. Any file-scanning detector false-negatives in the canonical Claude Code setup. git-history reads
the same trailer data the gate reads, works on day one, and confirmed-live here.

### Behavior

A new probe `probe_approval_trailer_collision(project_root, artifacts_root)` in `doctor.rs`:

1. Load typed config: `crate::config::ProjectConfig::load(artifacts_root)` (no new config key —
   `approval` and `agent_trailer_patterns` are already typed and in `KNOWN_DESIGN_KEYS`).
2. **Scope gate:** if `cfg.design_approval != HumanCommit`, print
   `approval trailer collision: n/a (approval=status-line; provenance check is shadow)` and return.
   (Reads the *resolved* mode, so when the roadmap flips the default to `human-commit` the probe
   activates automatically — no code change at flip time.)
3. Sample recent **authored** commits once:
   `git -C <project_root> log -n 20 --no-merges --format=%(trailers)`.
   (Folded `%(trailers)` — the exact rendering the gate reads at `main.rs:1710` — so the probe's
   line-by-line match cannot diverge from the gate's; see review-gate fidelity note.)
   - `--no-merges`: an approval commit edits a spec file — it is never a merge; human PR-merge commits
     would only add noise.
   - On git failure / not-a-repo / empty output: print `approval trailer collision: n/a (git history
     unavailable)` and return.
4. Pass the output to a **pure** decision fn (testable, no shell-out):
   `fn approval_trailer_collision(trailers: &str, patterns: &[String]) -> Option<(String, String)>`
   — mirroring the gate's mechanics exactly: for each line, lowercase, keep only lines starting
   `co-authored-by:`, take the trimmed value after the 15-char prefix, and for each pattern
   `regex::Regex::new(pattern).is_match(value)`. Returns `Some((matched_value, matched_pattern))` on the
   first match; `None` if none. Patterns that fail to compile are skipped (no panic), matching the
   gate's "bad regex is not a positive" stance.
5. Emit:
   - match → **`WARN`** (one line, plain `println!`, bare ASCII tag, no emoji): names the matched value
     and pattern, states that under `approval="human-commit"` the next human approval commit will be
     read as agent self-approval and FAIL the design gate, and gives the remediation (drop the
     always-add-`Co-Authored-By` rule for approval commits, or relax `agent_trailer_patterns`).
   - no match → `approval trailer collision: ok (no agent trailer on recent authored commits)`.

### What this check is — and is NOT (honest framing)

This is a **config-collision forecast**, not an integrity attestation. It detects the standing *policy*
("is this repo in the habit of stamping agent trailers?"), which is the right target for a setup-time
advisory. It does **not** predict the gate's verdict on a specific spec, and it cannot distinguish a
genuine agent self-approval from a benign human-with-courtesy-trailer commit (neither can the gate). An
`ok` line therefore does **not** clear an operator of self-approval. This is precisely why it is a
`WARN` that never escalates toward `FAIL`, and why the window (`-n 20 --no-merges`) is a deliberately
coarse policy-sniff, not a correctness claim.

### Drift control

The ~6-line matcher is **inlined in `doctor.rs`**, pinned by a unit test on the pure fn (mirroring
`version_skew`, `doctor.rs:74,839-865`). It is **not** extracted into a shared helper, because the only
way to dedupe with the gate is to edit `main.rs` (where the matcher lives, `main.rs:1731-1759`), and
`main.rs` is **protected** (`security/rules.toml:223`) — a human `--no-verify` on a security-sensitive
file to dedupe six lines fails the simplicity rubric. The only thing that must not drift is the
*patterns*, and both sites read the **same** `design_agent_trailer_patterns` config, so they cannot
diverge on the matchable set; only the trivial parse mechanics are duplicated, and the unit test pins
those. A future change that genuinely needs convergence can extract the helper and rewire `main.rs` in
one human-approved commit — not pre-paid here on speculation.

## Scope / non-goals

- **In:** one advisory probe in `doctor.rs` + one pure decision fn + unit tests; no new config key; no
  edits to protected files (`main.rs`/`scan.rs`/`rules.toml`/hooks/`Cargo.*`).
- **Out:** changing the gate's behavior or message; resolving the *policy* contradiction itself (scoping
  the harness trailer rule is a methodology change, not this bug fix — the WARN's remediation points at
  it); a config-only OR-fallback (rejected: cries wolf on clean human-commit repos); `check-existing-specs`
  confirmation (deferred — adds specificity but not day-one coverage; can be a later enhancement).

## Test strategy (TDD)

Pure-fn unit tests in `doctor.rs` `#[cfg(test)] mod tests` (extend existing), driving
`approval_trailer_collision`:

1. A trailer block containing `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`
   with default patterns → `Some(("Claude …", "(?i)claude"))`.
2. Clean human trailers (`Signed-off-by: Jane <…>`, `Reviewed-by: …`) → `None`.
3. A non-`Co-Authored-By` trailer whose *value* contains "claude" (e.g.
   `Reviewed-by: claude@x`) → `None` (only the `co-authored-by:` key is examined — mirrors the gate's
   `main.rs:1734`).
4. Case-insensitive key (`co-authored-by:` lowercased input) and a non-default pattern
   (e.g. `["(?i)copilot"]` vs a Copilot trailer) → `Some`.
5. An invalid regex pattern in the list is skipped, and a valid one still matches → `Some` (no panic).
6. Empty input / no trailers → `None`.

(Per house convention, the git shell-out and the `println!` branches are not unit-tested — the decision
logic is extracted into the pure fn that is. Behavior of the scope gate and output lines is exercised at
the verify gate by running `gatekeeper doctor` against the live repo.)
