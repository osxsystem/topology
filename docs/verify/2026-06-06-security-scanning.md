# Verify: Security scanning (the deterministic safety floor)

- **Date:** 2026-06-06
- **Feature slug:** security-scanning
- **Branch:** `feat/security-scanning`
- **Design:** docs/specs/2026-06-06-security-scanning.md (approved 2026-06-06)
- **Plan:** docs/plans/2026-06-06-security-scanning.md
- **ADR:** docs/adr/0007-security-scanner-dependencies.md (Accepted)

This note records the command run and the output observed for each acceptance criterion in the
spec, per the `verify-before-done` gate.

## Quality gates (run from `gatekeeper/`)

```
$ cargo test
# 76 passed across 3 suites (bin unittests + tests/cli_scan + tests/cli_review); 2 ignored
#   (the scan::perf_report evidence tests, run explicitly below)

$ cargo fmt --check
# exit 0 (no diff)

$ cargo clippy --all-targets -- -D warnings
# Finished; no warnings (all targets, incl. test modules)

$ cargo build --manifest-path gatekeeper/Cargo.toml   # from repo root
# Finished dev profile
```

`--all-targets` is the stricter form (lints the `#[cfg(test)]` modules too); plain
`cargo clippy -- -D warnings` (the future CI command) is a subset and also passes.

## Acceptance criteria → evidence

| Spec criterion | Evidence (test / command) | Result |
|---|---|---|
| Versioned `rules.toml` loads + validates; fail-loud on defects | `scan::load_tests` (9): bad schema_version, unknown field, bad kind/severity, dup id, uncompilable pattern names the id, `rule="*"` without value, etc. | pass |
| Planted AWS key **blocked**; clean input **passes**; value never emitted | `scan::match_tests::blocks_planted_aws_key` (redacted hint `AKIA…<len=20>`, raw key absent), `clean_input_passes` | pass |
| Span-scoped `[[allow]]` exempts only the matched span | `scan::match_tests::allow_is_span_scoped` (example key allowed; a second real key on the same line still blocks) | pass |
| Non-UTF8 / NUL bytes scanned (byte regex) | `scan::match_tests::matches_non_utf8_bytes` | pass |
| CRLF doesn't hide a secret; line number correct | `scan::match_tests::crlf_content_still_detected` (reports `f:2`) | pass |
| Linear-time matcher (no ReDoS); deterministic ceilings | `scan::match_tests::perf_5mib_under_generous_ceiling`, `perf_partial_match_storm_stays_linear` (both < 2s) | pass |
| `scan --content` blocks a planted key, passes clean (stdout silent) | `cli_scan::content_blocks_planted_key_and_passes_clean`; manual from repo root: planted key → `BLOCK aws-access-key-id … (redacted: AKIA…<len=20>)` exit 1; clean → exit 0 | pass |
| `scan --cmd` runs content **and** command rules; safe variants pass | `cli_scan::cmd_rules_block_the_dangerous_and_pass_the_safe` (`curl\|sh`, `rm -rf /`, force push, `--no-verify`/`-n` block; `--force-with-lease`, scoped `rm`, ordinary cmd pass; secret in a command string blocks) | pass |
| `scan --check-path` flags protected files only; missing arg → exit 2 | `cli_scan::check_path_flags_protected_only` | pass |
| `scan --staged` blocks a staged secret; clean passes | `cli_scan::staged_blocks_planted_secret`, `staged_clean_passes` | pass |
| Integrity pass catches delete/rename of a protected file (ACDMRT) | `cli_scan::staged_integrity_blocks_delete_of_protected`, `staged_integrity_blocks_rename_away_of_protected` | pass |
| Binary/unscannable blob blocked unless allowlisted; over-cap blocked then allow_blob passes | `cli_scan::staged_binary_blob_blocks_unless_allowlisted`; `scan::staged_unit::over_cap_blocks_then_allowlisted_passes` (8-byte cap → block, then pinned by `blob_oid` → pass) | pass |
| Symlink scans the target-string blob, never follows to the pointee | `cli_scan::staged_symlink_scans_target_string_not_pointee` | pass |
| Submodule gitlink (mode 160000) skipped, not errored | `cli_scan::staged_submodule_gitlink_not_recursed` | pass |
| Every staged blob scanned (not just the first) | `cli_scan::staged_many_blobs_all_scanned` (30 clean + 1 secret → block) | pass |
| `scan --hook` Bash `curl\|sh` → one `deny` JSON, exit 0 | `cli_scan::hook_bash_curl_pipe_sh_denies` (exactly one `hookSpecificOutput`) | pass |
| Clean Bash → silent allow (empty stdout) | `cli_scan::hook_clean_bash_is_silent` | pass |
| `\uXXXX`-escaped payload decoded before scan (no raw-byte evasion) | `cli_scan::hook_unicode_escaped_payload_is_decoded_and_denied` | pass |
| Malformed / deeply-nested event fails closed → exit 2, no decision JSON | `cli_scan::hook_deep_nesting_fails_closed` | pass |
| Write/Edit to a protected path → `ask` | `cli_scan::hook_write_protected_path_asks` | pass |
| Edit/MultiEdit/`replace_all` reconstruct the post-edit file and catch a completed secret | `cli_scan::hook_edit_completes_secret_across_unchanged_text`, `hook_multiedit_reconstructs_and_denies`, `hook_replace_all_applies_to_every_occurrence` | pass |
| `security-scan.sh` end-to-end: deny / allow / ask, fail-closed | manual: `curl\|sh` event → `deny` JSON exit 0; `ls` → empty stdout; protected Write → `ask`; missing/erroring binary → `deny` (fail-closed) | pass |
| `pre-commit.sh`: staged secret aborts (exit 1); clean passes (exit 0); fail-closed | manual in a scratch repo: planted key → `BLOCK …` + override message, exit 1; clean → exit 0; no binary → "unavailable", exit 1 | pass |
| `install.sh` registers PreToolUse matcher + installs git pre-commit | static: `grep '"PreToolUse"'`, `'Bash\|Write\|Edit\|MultiEdit'`, `'pre-commit.sh'` all present; throwaway-copy run advertises PreToolUse and installs a **copy** of the hook into `.git/hooks/pre-commit` (a stable copy, not a symlink to the mutable worktree file; live `.git` untouched) | pass |
| `json.rs` retired; routing on `serde_json` unchanged | `ls src/json.rs` → absent; `routes_on_keyword` green; manual `printf 'plan this' \| … activate` → `- write-plan [require]` | pass |
| `security-scanning` skill present + routed | `cargo run -- list` → description starts "The deterministic safety floor"; `printf 'scan for secrets' \| … activate` → `- security-scanning [require]` | pass |
| ADR-0007 recorded + index row | `docs/adr/0007-security-scanner-dependencies.md` present; README row added | pass |
| ROADMAP Phase 1 → delivered; README/AGENTS note the floor | `grep 'delivered' docs/ROADMAP.md` (Phase 1 row); README `scan` gate row; `grep 'safety floor' AGENTS.md` | pass |

## Performance evidence (`scan::perf_report`, `--ignored`)

```
$ cargo test --bin gatekeeper perf_report -- --ignored --nocapture
scan latency us: p50=15 p95=17 p99=20      # target: p95 < 150 ms, p99 < 250 ms
staged N=1: 21 ms
staged N=10: 153 ms
staged N=100: 1375 ms
```

- **Latency:** p95 = 17 µs, p99 = 20 µs — ~4 orders of magnitude under the 150/250 ms targets.
- **Staged scaling:** ≈linear at ~14 ms/blob (N=100), dominated by the per-blob `git` subprocess
  spawns (`cat-file -s`, `show`, `ls-files`), not the matcher. The architecture guarantees linearity
  (independent per-blob loop, no shared state). The interim per-blob git calls are the queued Q2
  `--raw` single-enumeration redesign's target.

## Notes

- Baseline before this work (Task 1): `42 passed` across 2 suites. After: `76 passed` across 3
  suites (+ the new `tests/cli_scan.rs`), with the 2 `json.rs` unit tests removed when the module was
  retired (Task 10) and the routing behavior re-covered on `serde_json`.
- The `reason` field on `[[allow]]` / `[[allow_blob]]` is human documentation: it is accepted and
  validated by `deny_unknown_fields` but never read by logic, so it carries `#[allow(dead_code)]`
  with a clarifying comment (clippy-clean by intent, not suppression of a real defect).
- Honest scope (spec threat model): **history is the strong net** — every staged blob is scanned at
  commit. The **working-tree veto is partial**: it covers `Bash` commands and tool-writes
  (`Write`/`Edit`/`MultiEdit`), not content a `Bash` command itself writes to disk — that is caught
  at the pre-commit boundary. The threat model is *mistakes, not evasion*; a determined operator can
  still `git commit --no-verify` at their own terminal (a human action, by design).
