VERDICT: pass
HEAD: 39b1821b5d6b08b75d5520edbf11c381aa139545
BASE: b03c7eb23c28206afb75e5ff21ef4587b0a77f41

# Review: doctor-settings-paths (2026-06-13)

## Blocking findings
None.

## Non-blocking notes
- `doctor.rs:712-715` — the malformed-JSON branch prints `settings.json paths: skipped (.claude/settings.json is malformed)`, a fourth output line not enumerated in the spec's "Output lines" list (which names only n/a / ok / WARN). It is defensible: graceful, still advisory, and it mirrors the sibling `probe_config_unknown_keys` "skipped (... malformed)" precedent at `doctor.rs:432`. Judgement call, not a spec violation; worth a one-line mention in the spec for completeness.
- `doctor.rs:739` — existence is checked with `Path::exists()`, which follows symlinks and resolves a relative path against the doctor process CWD. The spec already documents this under "Symlinked / relative paths" as acceptable (topology never emits relative hook paths; advisory only). No change needed; recorded for traceability.
- `doctor.rs:701` `probe_settings_paths` returns `()` while the sibling failing probes return `usize`. This is correct (advisory probes `probe_config_unknown_keys` / `probe_orphaned_replay_worktrees` also return `()`), but it means the "all resolve → ok" line is emitted unconditionally even when the only hook entries are non-command matchers; harmless and matches the design's pinned `ok` literal.

## Criteria checked
### Spec/plan
- **AC1 (WARN names the offending hook `command` and `GATEKEEPER_BIN` path)** — `doctor.rs:741-746` pushes a `WARN: hook command path does not exist: <resolved>` line per missing hook command; `doctor.rs:761-765` pushes `WARN: GATEKEEPER_BIN path does not exist: <bin>`. Both name the path and carry the M4 remediation hint. Verified at runtime by the verify note CASE B and by `doctor_warns_on_stale_settings_hook_path` / `doctor_warns_on_stale_gatekeeper_bin` (both pass — ran `cargo test --test cli_doctor`, 17 passed 0 failed).
- **AC2 (no false positive on a resolvable `${CLAUDE_PROJECT_DIR}` path)** — `doctor.rs:739` resolves the literal via `resolve_claude_project_dir` before `.exists()`; `doctor_no_warn_on_resolvable_portable_hook_path` asserts `settings.json paths: ok` and absence of `WARN: hook command path` and passes.
- **AC3 (helper unit test)** — `resolve_claude_project_dir_substitutes_literal` (doctor.rs `mod tests`) checks both the substituted and the literal-free path; ran `cargo test --bin gatekeeper resolve_claude_project_dir` → 1 passed.
- **AC4 (fixture integration tests)** — three tests in `cli_doctor.rs:511-564` built on the new `write_settings` fixture (`cli_doctor.rs:87-103`), following the `doctor_warns_on_orphaned_replay_worktree` scratch-root/subprocess pattern. All three present and passing.
- **AC-advisory (exit code unchanged)** — each WARN test asserts `code == 0`; `probe_settings_paths` returns `()` and never touches `failures`, so the summary at `doctor.rs:386-392` is unaffected. Verify note CASE A/B/C all `EXIT=0`.
- **Scope (no creep beyond #52)** — the diff is exactly: import widened to `PathBuf` (`doctor.rs:6`), one helper, one probe, one wiring line at `doctor.rs:383`, one fixture helper, three tests, plus the four governance docs. The existing runtime-env `GATEKEEPER_BIN` probe (`doctor.rs:188-203`) is untouched, honoring the spec's "do not touch" constraint. `git diff --stat` shows only `doctor.rs` (+111/-1), `cli_doctor.rs` (+74), and docs.

### Standards
- **Three-language-lanes (docs/DEVELOPMENT.md:9 — logic lives in the Rust `gatekeeper` crate)** — all new behavior is in `doctor.rs`; no Bash or Markdown gained executable logic. The probe only reads `.claude/settings.json`; it writes nothing.
- **Advisory-probe convention (matches `probe_config_unknown_keys` doctor.rs:419 / `probe_orphaned_replay_worktrees` doctor.rs:482)** — `probe_settings_paths` returns `()`, prints one or more lines, and the file-absent / malformed branches degrade gracefully exactly like `probe_config_unknown_keys` (n/a vs. skipped-malformed). Genuinely advisory: confirmed it never feeds `failures`.
- **M2 (substitute `${CLAUDE_PROJECT_DIR}` on hook commands only, not on GATEKEEPER_BIN)** — hook commands pass through `resolve_claude_project_dir` (`doctor.rs:739`); the GATEKEEPER_BIN branch uses `Path::new(bin)` directly with no substitution (`doctor.rs:760`). Locked decision honored.
- **B1 (whole-string path extraction)** — `doctor.rs:735-746` treats the entire resolved `command` string as the path, no token split, per the locked decision (topology emits bare paths; a split would only truncate space-containing paths).
- **JSON traversal correctness vs. the real Claude schema** — `hooks` → object-of-events → array-of-matcher-entries → `entry.hooks[]` → `{type, command}`; the probe walks exactly this shape (`doctor.rs:723-738`) and matches the live `.claude/settings.json` (the verify note's live-repo run prints `settings.json paths: ok`, no false positive). Every node access is `.get().and_then(as_object|as_array|as_str)` with `match … continue` — no `unwrap`/`expect`, no index panic, and non-matching shapes are skipped rather than mis-flagged.
- **Evidence-over-assertion (AGENTS.md verify gate)** — the verify note records reproduce-then-resolve with actual CLI output and exit codes (CASE A/B/C) plus the live-repo no-false-positive run; I independently re-ran the four new tests and they pass.
- **Surgical-changes-only / Simplicity (AGENTS.md "Conduct between gates")** — no new dependency (reuses `serde_json`), no new abstraction or config knob, one self-contained block; a staff engineer would not call it over-built. The `git diff` reads as exactly the requested change.
