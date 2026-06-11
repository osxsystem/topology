# Plan — day-zero containment (v0.4.1)

Executes the approved [spec](../specs/2026-06-11-day-zero-containment.md); grounding in the
[research note](../research/2026-06-11-day-zero-containment.md) and the
[remediation roadmap](2026-06-11-five-failure-modes-roadmap.md) Phase 0 / ROADMAP Phase 13.
Branch: `feat/day-zero-containment` (worktree `topology-phase13`). Owner column says who makes
the commit — two commits are constitutionally human.

| # | Task | Files | Owner | Acceptance |
|---|------|-------|-------|------------|
| 1 | Docs commit: spec (decisions recorded, `Status: draft`) + research note + this plan | `docs/specs/…`, `docs/research/…`, `docs/plans/…` | agent | research/plan gates pass |
| 2 | Red bench commit: `cli_scan_bench.rs` + `secrets-bench/` corpus (6 negative files + README; positives runtime-assembled) | `gatekeeper/tests/…` | agent | `cargo test --test cli_scan_bench` fails at 5/11, missing exactly `jwt-bearer`, `openai-sk-proj`, `anthropic-sk-ant`, `password-labeled`; negatives 0/6 |
| 3 | Approval commit: flip `Status: draft → approved` (one line) | `docs/specs/…` | **human** | design gate passes; `git log -L` on the Status line shows a commit with no agent co-author trailer |
| 4 | Green commit: spec §1 ruleset changes — add `jwt-structural` (block), broaden `openai-key` to `\bsk-(?:[A-Za-z0-9_]+-)*[A-Za-z0-9_]{20,}\b`, add `labeled-secret-assignment` (warn). Protected path: agent edits the working tree, human commits with `git commit --no-verify` (hooks/pre-commit.sh:38 aborts agent commits by design) | `security/rules.toml` | **human** | bench green: 9/11 in-scope by expected rule ids, no regression on the five v0.4.0 classes, negatives still 0/6 |
| 5 | `scripts/metrics.sh`: first-parent walk of `main` since `v0.3.0`; one CSV row per merge (`branch, merge_commit, production_loc, artifact_loc, commits, lead_time_hours`) + one labeled direct-to-main residual row; counting rules per spec §4 | `scripts/metrics.sh` | agent | rows sum to the whole delta since `v0.3.0`; rerunning the script reproduces the committed CSV byte-identically |
| 6 | Baseline research note: CSV output + median commits/branch + artifact:production ratio | `docs/research/2026-06-11-process-weight-baseline.md` | agent | the FM1 denominator is stated as two numbers (expected ≈8 and ≈5:1) |
| 7 | USER-GUIDE note (push-protection bypass-URL flow, three lines) + CHANGELOG `v0.4.1` entry | `docs/USER-GUIDE.md`, `CHANGELOG.md` | agent | `gatekeeper check docs` passes |
| 8 | Enable push protection: `gh api -X PATCH repos/osxsystem/topology -f 'security_and_analysis[secret_scanning_push_protection][status]=enabled'` | repo settings | human or agent (approved in spec §3) | `gh api repos/osxsystem/topology --jq '.security_and_analysis'` reads `"enabled"` |
| 9 | Verify artifact (bench scoreboard before/after, push-protection API read, metrics reproduction) + review artifact; run all gates; merge `feat/day-zero-containment`; tag `v0.4.1`; delete branch local+remote | `docs/verify/…`, `docs/reviews/…` | agent (merge/tag after human OK) | full suite green; all seven spec acceptance criteria check off; tag ships the stranded `39710a0` usage fix |

Commit-order constraint (spec acceptance 1): task 2's bench commit precedes task 4's rules
commit — branch history must show red-then-green. Tasks 5–7 land after task 4; task 8 is
order-independent.

Out of scope (spec non-goals): entropy rules, schema bump, allowlist edits, any `scan.rs`/
`main.rs` change, secret-shaped literals in the tree.
