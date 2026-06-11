# Plan — hollow-pass kills + drift-proof CLI surface (v0.5.0)

Executes the [spec](../specs/2026-06-11-hollow-pass-kills.md); grounding in the
[research note](../research/2026-06-11-hollow-pass-kills.md) and the
[remediation roadmap](2026-06-11-five-failure-modes-roadmap.md) Phase 1 / ROADMAP Phase 14.
Branch: `feat/hollow-pass-kills` (worktree `topology-phase14`). One commit is constitutionally
human — and per spec §4 it must be *made* by the human this time, not executed by the agent at
direction, because the approval-provenance check it approves would reject the old practice.

| # | Task | Files | Owner | Acceptance |
|---|------|-------|-------|------------|
| 1 | Docs commit: spec (decisions D1-D5 recorded, `Status: draft`) + research note + this plan | `docs/specs/…`, `docs/research/…`, `docs/plans/…` | agent | research/plan gates pass |
| 2 | Red scoreboard commit: `cli_hollow.rs` with all seven fixtures `#[ignore]`-tagged (suite green by default, baseline stays clean) | `gatekeeper/tests/cli_hollow.rs` | agent | `cargo test --test cli_hollow -- --ignored` shows exactly 7 failures — proof the gates pass hollow artifacts today; default run green |
| 3 | Approval commit: flip `Status: draft → approved` (one line) | `docs/specs/…` | **human** | design gate passes; the commit carries **no** agent co-author trailer (it becomes the dogfood case for task 7) |
| 4 | Dispatch table (spec §2) + ADR-0014: replace the match + nine `USAGE_*` constants with `static SUBCOMMANDS`; `print_help()` iterates the table | `gatekeeper/src/main.rs`, `docs/adr/0014-*.md`, `docs/adr/README.md` | agent | `grep -c 'const USAGE' main.rs` → 0; `cli_help_flags.rs` output byte-identical (modulo corrected `check` usage); ADR linked |
| 5 | Doc-sync test (spec §6): `cli_doc_sync.rs` + wire into `ci.yml` gate job and `release.yml` version-guard | `gatekeeper/tests/cli_doc_sync.rs`, `.github/workflows/*` | agent | test green locally; both workflow files run it; deliberately desynced README line fails it (shown in verify artifact, then reverted) |
| 6 | Verify evidence replay (spec §3): `[verify]` config table, evidence-block parser, allowlist fail-closed, zero-block fail, `GATEKEEPER_SHADOW=1` log-only path; un-ignore fixture (b) | `gatekeeper/src/main.rs`, `gatekeeper/src/config.rs` | agent | fixture (b) green un-ignored; replay-mode unit/integration tests per spec acceptance 4 |
| 7 | Design hardening (spec §4): substance floor (always-on) + `[design] approval = "human-commit"` (git log -L + trailer check) + doctor git-capability probe; un-ignore fixture (a) | `gatekeeper/src/main.rs`, `config.rs`, `doctor.rs` | agent | fixtures (a) green; task 3's real approval commit passes `human-commit` mode on this branch (dogfood); agent-trailer fixture fails it |
| 8 | Finish zero-test floor (spec §5): capture output, runner-summary parsing, `[finish] require_test_count`; un-ignore fixtures (e), (g) | `gatekeeper/src/main.rs`, `config.rs` | agent | fixtures (e), (g) green un-ignored; spec acceptance 6 tests pass |
| 9 | Docs + version: USER-GUIDE (three config tables, evidence format, shadow env), CHANGELOG `v0.5.0`, bump `Cargo.toml`/`plugin.json`/`marketplace.json` to 0.5.0 | `docs/USER-GUIDE.md`, `CHANGELOG.md`, version files | agent | `gatekeeper check docs` passes; doc-sync test still green (it now guards these edits) |
| 10 | Shadow KPI run: `GATEKEEPER_SHADOW=1` replay over existing `docs/verify/` artifacts, record ≥90% green on allowlisted commands (spec D2); verify artifact **written in the new evidence format** (dogfood), then review artifact as branch tip; all gates; merge; tag `v0.5.0`; delete branch local+remote | `docs/verify/…`, `docs/reviews/…` | agent (merge/tag after human OK) | full suite + `just check` green; all eight spec acceptance criteria check off; release workflow (incl. new version-guard doc-sync) green at the tag |

Commit-order constraints: task 2 (red scoreboard) precedes tasks 6-8 (fixes) — history must
show the holes existed before the kills. Task 3 precedes task 4 (no production code before the
design gate passes). Tasks 6, 7, 8 touch disjoint gate functions and may interleave, but each
un-ignores its fixture in the same commit as its fix. Task 10's review artifact is the branch
tip, committed alone after `gatekeeper check review` passes (branch-tail convention).

Out of scope (spec non-goals): red-green replay, entropy rules, routing, default flips,
fixtures (c)/(d)/(f) beyond landing them ignored, any new dependency, edits to historical
verify artifacts.
