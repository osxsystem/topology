# Plan — hollow-pass kills + drift-proof CLI surface (v0.5.0)

Executes the [spec](../specs/2026-06-11-hollow-pass-kills.md) **at revision 3**; grounding in
the [research note](../research/2026-06-11-hollow-pass-kills.md) and the
[remediation roadmap](2026-06-11-five-failure-modes-roadmap.md) Phase 1 / ROADMAP Phase 14.
Branch: `feat/hollow-pass-kills` (worktree `topology-phase14`). One commit is constitutionally
human — and per spec §4 it must be *made* by the human this time, not executed by the agent at
direction, because the approval-provenance check it approves would reject the old practice.

| # | Task | Files | Owner | Acceptance |
|---|------|-------|-------|------------|
| 1 | Docs commit: spec (decisions D1-D11 recorded, `Status: draft`) + research note + this plan; rev-2/rev-3 review amendments committed as follow-ups on the same artifacts | `docs/specs/…`, `docs/research/…`, `docs/plans/…` | agent | research/plan gates pass |
| 2 | Red scoreboard commit: `cli_hollow.rs` with all seven fixtures `#[ignore]`-tagged (suite green by default, baseline stays clean) | `gatekeeper/tests/cli_hollow.rs` | agent | `cargo test --test cli_hollow -- --ignored` shows exactly 7 failures — proof the gates pass hollow artifacts today; default run green |
| 3 | Approval commit: flip `Status: draft → approved` (one line; the revision marker is deliberately *not* on that line) | `docs/specs/…` | **human** | design gate passes; the commit carries **no** agent co-author trailer (the dogfood case for task 7) |
| 4 | Dispatch table (spec §2) + ADR-0014: replace the match + nine `USAGE_*` constants with `static SUBCOMMANDS`; longest-prefix two-word matching; `print_help()` iterates the table; thin wrapper handlers | `gatekeeper/src/main.rs`, `docs/adr/0014-*.md`, `docs/adr/README.md` | agent | `grep -c 'const USAGE' main.rs` → 0; `cli_help_flags.rs` green **unmodified**; help diffs confined to spec §2's enumerated sanctioned list (before/after captured for the verify artifact); ADR linked |
| 5 | Doc-sync test (spec §6): `cli_doc_sync.rs` with the §6 extraction grammar + scope; wire into `ci.yml` gate job and `release.yml` version-guard as `cargo test --manifest-path gatekeeper/Cargo.toml --test cli_doc_sync` | `gatekeeper/tests/cli_doc_sync.rs`, `.github/workflows/*` | agent | test green locally; both workflows carry the manifest-path invocation; a deliberately desynced README line fails it (shown in verify artifact, then reverted) |
| 6 | Verify evidence replay (spec §3): `[verify]` config table; evidence grammar incl. malformed-directive rule; argv split + metachar/env-assignment/allowlist rejection (read-only git defaults); `process_group(0)` + `kill -- -<pid>` timeout (Unix); 1 MiB tail-capped two-thread merge; default-mode **static-only** shadow lines; `GATEKEEPER_SHADOW=replay` execution path with legacy extraction + annotation-stripping normalization; config-strictness incl. TOML-parse-failure → exit 2; un-ignore fixture (b) | `gatekeeper/src/main.rs`, `gatekeeper/src/config.rs` | agent | fixture (b) green un-ignored; spec acceptance 4 tests pass incl. the booby-trap no-execution check and the no-orphan timeout check |
| 7 | Design hardening (spec §4): substance floor (config-gated `[design] substance_floor`, exact body-line predicate) + `human-commit` approval reading the **committed** spec (`git show HEAD:`), fail-closed on dirty/untracked/shallow/git<2.15/unparsable probes; configurable `agent_trailer_patterns`; doctor probes; un-ignore fixture (a) | `gatekeeper/src/main.rs`, `config.rs`, `doctor.rs` | agent | fixture (a) green; task 3's real approval commit passes `human-commit` mode on this branch (dogfood); agent-trailer, dirty-spec, and old-git fixtures each fail closed with their specific message |
| 8 | Finish zero-test floor (spec §5): streaming tee capture; cargo + fence-anchor-free pytest patterns; `extra_count_patterns`; floor applies to config `test_command` **and** `-- <cmd>` override; SHADOW emission; un-ignore fixtures (e), (g) | `gatekeeper/src/main.rs`, `config.rs` | agent | fixtures (e), (g) green un-ignored; spec acceptance 6 tests pass incl. `pytest -q`-style summary and the `-- true` bypass attempt |
| 9 | Docs + version: USER-GUIDE (three config tables, evidence grammar, read-only/idempotent-evidence requirement, SHADOW schema + jq aggregation, `GATEKEEPER_SHADOW=replay`, deferred-Go note), CHANGELOG `v0.5.0`, bump `Cargo.toml`/`plugin.json`/`marketplace.json` to 0.5.0 | `docs/USER-GUIDE.md`, `CHANGELOG.md`, version files | agent | `gatekeeper check docs` passes; doc-sync test still green (it now guards these edits) |
| 10 | Baseline measurement + close-out: run the documented `GATEKEEPER_SHADOW=replay` loop over `docs/verify/`, aggregate with the documented jq procedure, **record** the numbers (no threshold gate, spec D2) in a verify artifact written in the new evidence format whose own blocks replay 100% green; review artifact as branch tip; all gates; merge; tag `v0.5.0`; delete branch local+remote | `docs/verify/…`, `docs/reviews/…` | agent (merge/tag after human OK) | full suite + `just check` green; all **nine** spec acceptance criteria check off; release workflow (incl. version-guard doc-sync) green at the tag |

Commit-order constraints: task 2 (red scoreboard) precedes tasks 6-8 (fixes) — history must
show the holes existed before the kills. Task 3 precedes task 4 (no production code before the
design gate passes), and nothing may edit the spec's `Status:` line after task 3 (it would
retarget the provenance check task 7 dogfoods). Tasks 6, 7, 8 touch disjoint gate functions
and may interleave, but each un-ignores its fixture in the same commit as its fix. Task 10's
review artifact is the branch tip, committed alone after `gatekeeper check review` passes
(branch-tail convention).

Out of scope (spec non-goals): red-green replay, entropy rules, routing, default flips,
fixtures (c)/(d)/(f) beyond landing them ignored, any new dependency (incl. libc — std
`process_group` only), edits to historical verify artifacts, any numeric gate on the legacy
baseline.
