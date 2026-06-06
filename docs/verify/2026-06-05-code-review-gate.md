# Verify: Code-review critic gate

- **Date:** 2026-06-05
- **Feature slug:** code-review-gate
- **Branch:** `feat/code-review-gate`
- **Design:** docs/specs/2026-06-05-code-review-gate.md (approved)
- **Plan:** docs/plans/2026-06-05-code-review-gate.md (R2)

This note records the command run and the output observed for each acceptance criterion in the
spec (§ Acceptance criteria), per the `verify-before-done` gate.

## Quality gates (run from repo root)

```
$ cargo test --manifest-path gatekeeper/Cargo.toml
test result: ok. 36 passed; 0 failed   (bin unittests: 4 main + 2 json + 19 review::tests + 11 review::gate_tests)
test result: ok. 1 passed; 0 failed    (tests/cli_review.rs integration binary)
# 37 passed across 2 suites; 0 failed; doctests 0

$ cargo fmt --manifest-path gatekeeper/Cargo.toml --check
# exit 0 (no diff)

$ cargo clippy --manifest-path gatekeeper/Cargo.toml --all-targets -- -D warnings
# Finished; no warnings (all targets, incl. test modules)
```

`--all-targets` is the stricter form (lints the `#[cfg(test)]` modules too, where a redundant
import would surface); plain `cargo clippy -- -D warnings` (the future CI command) is a subset and
also passes.

## Acceptance criteria → evidence

| Spec criterion | Evidence (test / command) | Result |
|---|---|---|
| `check review --feature <slug> [--base <ref>]` exists; in `--help` + `//!` | `cargo run -- check review` (no flag) → usage error, exit 2; `--help` and `//!` list the line | exit 2 observed |
| Fresh pass → exit 0, `PASS review gate: <path>` | `review::gate_tests::fresh_pass_exits_zero`; CLI `review_gate_runs_from_nested_subdir` printed `PASS review gate: …/topo_cli_…/docs/reviews/2026-06-05-code-review-gate.md` | pass |
| Stale HEAD → exit 1 | `gate_tests::stale_head_exits_one` | pass |
| Dirty worktree (outside `docs/reviews/`) → exit 1 | `gate_tests::dirty_outside_reviews_exits_one` (modified tracked file); `gate_tests::untracked_outside_reviews_exits_one` (untracked file) | pass |
| Artifact doesn't self-dirty | `gate_tests::fresh_artifact_does_not_self_dirty` (asserts `-uall` shows the artifact, gate still 0) | pass |
| Wrong base → exit 1 | `gate_tests::wrong_base_exits_one`; `gate_tests::divergent_branch_uses_fork_point_as_base` (real fork: `BASE`==merge-base passes, `BASE`==HEAD rejected) | pass |
| Not a repo / unresolvable `--base` → exit 1 | `gate_tests::not_a_repo_exits_one`; `gate_tests::unresolvable_base_exits_one` | pass |
| Ambiguity (2 artifacts name HEAD) → exit 1, prints paths | `gate_tests::ambiguous_two_artifacts_same_head_exits_one` | pass |
| Abbreviated SHA (HEAD/BASE) → exit 1 | `tests::abbreviated_head_sha_rejected`, `tests::abbreviated_base_sha_rejected` | pass |
| Line-1 `VERDICT: fail` → exit 1 | `gate_tests::fail_verdict_exits_one`; `tests::fail_doc_parses_as_fail` | pass |
| Pass with blockers → exit 1 | `tests::pass_with_a_blocking_item_rejected` | pass |
| Fail-closed line 1/2/3 | `tests::bad_verdict_keyword_rejected`, `tests::malformed_base_line_rejected` | pass |
| Heading count (0 or >1 blocking) → exit 1 | `tests::zero_blocking_headings_rejected`, `tests::two_blocking_headings_rejected` | pass |
| Missing dimension / empty subsection → exit 1 | `tests::missing_standards_dimension_rejected`, `tests::empty_specplan_dimension_rejected` | pass |
| Comment in parsed region (incl. unclosed) → exit 1 | `tests::comment_in_blocking_section_rejected`, `tests::unclosed_comment_in_blocking_section_rejected` | pass |
| No false-fail on honest quoting | `tests::honest_quoting_of_verdict_does_not_false_fail` | pass |
| BOM / CRLF / trailing-whitespace handled | `tests::bom_and_crlf_header_handled` | pass |
| Sample-template validity (literal spec pass/fail examples) | `tests::spec_pass_sample_parses`, `tests::spec_fail_sample_parses` | pass |
| Missing `--feature` → exit 2 | `cargo run -- check review` → `gatekeeper: --feature <slug> is required`, exit 2 | exit 2 observed |
| `cargo test` covers all cases, incl. nested-subdir invocation | `tests/cli_review.rs::review_gate_runs_from_nested_subdir` runs the compiled binary from `…/src/deep/nested`, proving `git -C <framework_root>` | pass |
| `skills/code-review/SKILL.md` exists with required content | `cargo run -- list` prints the `code-review` description; file present | pass |
| `hooks/skill-rules.json` routes `code-review` (require) | `printf 'please review this before merge' \| cargo run -- activate` → `- code-review [require]` | pass |
| verify/finish reference `verify → review → finish`; README/METHODOLOGY/ROADMAP updated | grep checks: METHODOLOGY sequence/table/Pillar 1; README gate row; verify→code-review; finish-branch entry+commit-on-merge; ROADMAP Phase 1.5 + reconciled note (stale "Only Phase 0" gone) | pass |
| ADR records the four decisions | `docs/adr/0006-code-review-gate.md` present (a–d) | pass |

## Notes

- Baseline before this work: `6 passed` (4 `main.rs` + 2 `json.rs`). After: `37 passed`
  (+19 parser, +11 gate, +1 CLI integration).
- The gate's clean-tree check uses `git status --porcelain --untracked-files=all` so an untracked
  directory is not collapsed to a bare `docs/` (which would slip past the `docs/reviews/` filter);
  a failed `git status` is fail-closed.
- Residual (documented, accepted): a fully-subverted critic emitting a clean `pass` for the correct
  head/base on a clean tree is undetectable by any parser (ADR 0006; spec threat model).
