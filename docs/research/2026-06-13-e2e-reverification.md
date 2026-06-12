# Research — end-to-end re-verification (Phase 12)

- **Date:** 2026-06-13
- **ROADMAP:** Phase 12 (lines 413–427); closes Track 2 (Distribution). Depends on Phases 8–11 (all
  shipped) + Phase 9 (`adapt` v2, shipped v0.9.0).
- **Branch:** `feat/phase12-e2e` (worktree `topology-phase12`)

## The question

ROADMAP Phase 12: "Wipe `react-weather-app`'s current install; run both scopes (`--global`, then
`--project`); record the verify artifact" proving **five consumer-visible outcomes**:
1. the agent sees the operating contract in its context (project `CLAUDE.md` import / managed block);
2. `gatekeeper check …` runs bare in a session (via `GATEKEEPER_BIN`);
3. the `UserPromptSubmit` and `PreToolUse` hooks fire;
4. the project's own pre-commit blocks a planted secret;
5. a design artifact lands in `<project>/.claude/topology/specs/`.

What exists to build this on, and what has to be decided?

## Findings

**F1 — `react-weather-app` does not exist on disk.** Not under `~/Codes/AgentTools/`, `~/Codes/`, or
`/tmp/`. So "wipe its current install" is not literally executable. The phrase names a *reference
project* to prove consumer outcomes on; the asset itself was never committed (it's an external demo
target). ⇒ **Decision needed (Q1):** stand up a genuine reference-project fixture instead of
depending on a missing external repo.

**F2 — a substantial e2e harness already exists** (`scripts/test-payload-e2e.sh`, 35.7K, run by
`just test-e2e`, already in the CI `installer` job). It drives the **real** `install.sh` in both
scopes (`bash -s -- --project … --harness none --yes` and `--global …`) and asserts installer
*mechanics*: payload layout (`_assert_payload_layout`), `.git/hooks/pre-commit` installed +
executable, `.gitignore` updated, doctor VERSION probe, legacy-clone rescue, commit-hint. Helpers
`_make_fixture` (git-init a repo) and the `pass`/`fail` counters are reusable.

**F3 — the gap is the *outcomes*, not the mechanics.** The existing harness uses `--harness none`
for most cases and never asserts the five *consumer-visible* outcomes with `--harness claude`:
contract-in-context (the Phase 9 import/managed block), `GATEKEEPER_BIN` bare invocation, the two
hooks firing, the project pre-commit vetoing a planted secret, and a design artifact resolving to
`<project>/.claude/topology/specs/`. Phase 12 is exactly this missing layer.

**F4 — local runs can produce real evidence.** From a checkout (worktree), `install.sh` builds the
payload locally (`build-payload.sh`) and, with `--build-from-source`, builds the binary — no network.
The existing harness already runs offline (`TOPOLOGY_RELEASE_BASE_URL` `file://` pattern). So the
verify artifact can carry a **real captured transcript**, not a hypothetical.

**F5 — no new binary behavior.** All five outcomes are already implemented (Phases 8–11 + Phase 9).
Phase 12 adds a *verification harness* + evidence, not gatekeeper functionality. ⇒ the `tdd` gate has
no production behavior to test-drive; the honest red→green is the **harness proving itself**: its
assertions must FAIL against an un-installed fixture (outcomes absent) and PASS after install — that
guards against a tautological/hollow e2e. ⇒ **Decision (Q4):** no gatekeeper version bump (binary
unchanged); ship the harness + CI wiring + verify artifact.

## Open questions for the design gate

- **Q1 — reference project.** Build a minimal but *genuine* `react-weather-app`-shaped fixture
  (package.json + a source file + git history) so the run mirrors a real consumer project, rather
  than the bare `_make_fixture` README-only repo. Proposal: a small fixture created in the script
  (no external dependency, reproducible offline, CI-safe).
- **Q2 — new script vs. extend `test-payload-e2e.sh`.** Proposal: a **new** `scripts/test-e2e-
  reference.sh` (+ `just test-e2e-reference`, wired into the CI `installer` job) — keeps the
  outcome-level re-verification distinct from the 35K mechanics harness; lower merge risk.
- **Q3 — proving each outcome.** (1) `CLAUDE.md` contains `@.topology/CONTRACT.md`; (2)
  `.claude/settings.json` `env.GATEKEEPER_BIN` is set and `"$GATEKEEPER_BIN" check …` runs with PATH
  scrubbed; (3) run `hooks/skill-activation.sh` (UserPromptSubmit) and `hooks/security-scan.sh`
  (PreToolUse) directly and assert their advisory/deny behavior — "fire" = produce their contract
  output; (4) stage a file with a planted secret, attempt `git commit`, assert non-zero + a scanner
  block line, and that `--no-verify` is the only bypass; (5) `gatekeeper` run from the project
  resolves artifacts root to `<project>/.claude/topology` (doctor) and a design check reads
  `.claude/topology/specs/` — demonstrate by dropping a spec there and resolving it.
- **Q5 — both scopes.** Run `--global` (payload at `~/.topology`-style `TOPOLOGY_HOME`, binary
  resolves, doctor green) AND `--project` (the five outcomes). The project scope carries outcomes
  1/4/5; global proves the shared-install path + binary resolution (outcome 2 substrate).

## Reproduction (verify-gate seed)

On `main`, no artifact proves the five outcomes end-to-end on a real install; `just test-e2e` covers
mechanics only (F3). The Phase 12 harness, run against a fresh fixture *before* install, must fail
all five (outcomes absent), then pass after `install.sh --project … --harness claude` — that red→green
is the verify evidence.
