# Plan: doctor probe for stale/dangling settings.json paths

- **Date:** 2026-06-13
- **Feature slug:** doctor-settings-paths
- **Design:** docs/specs/2026-06-13-doctor-settings-paths.md
- **Baseline:** tests green at commit `1fc7f8a` (gatekeeper: 547 passed, 0 failed, 5 ignored)

## Environment note (commits)

A stray untracked `.topology/` (CONTRACT.md only) at the repo root makes the pre-commit hook
treat this self-governed repo as a *governed* project and resolve scan rules from
`.topology/security/rules.toml`, which does not exist → every commit fails closed. Workaround used
for every commit below (keeps the security scan fully active, just pointed at the correct root):

```bash
TOPOLOGY_ROOT="$PWD" git commit -m "…"
```

`cargo test` must run with **`TOPOLOGY_ROOT` unset** — exporting it pollutes the
`cli_design_hardening` scratch-root tests (they control resolution themselves).

## Files

- `gatekeeper/src/doctor.rs` — add `resolve_claude_project_dir` (pure helper) + `probe_settings_paths`
  (advisory probe); wire the probe into `cmd_doctor`; add the helper unit test.
- `gatekeeper/tests/cli_doctor.rs` — add a `write_settings` fixture helper + three integration tests.

## Tasks

### Task 1: Pure `${CLAUDE_PROJECT_DIR}` substitution helper + unit test

- **File(s):** `gatekeeper/src/doctor.rs`
- **Change:**
  1. Change the path import at the top from `use std::path::Path;` to:
     ```rust
     use std::path::{Path, PathBuf};
     ```
  2. Add this function just above the `#[cfg(test)] mod tests` block (after `probe_hooks`):
     ```rust
     /// Resolve the portable `${CLAUDE_PROJECT_DIR}` literal in a settings.json path against the
     /// project root. A path with no literal is returned unchanged. Pure — unit-tested directly so
     /// the "no false positive on a valid portable path" guarantee is exercised at the unit level.
     fn resolve_claude_project_dir(raw: &str, project_root: &Path) -> PathBuf {
         PathBuf::from(raw.replace("${CLAUDE_PROJECT_DIR}", &project_root.to_string_lossy()))
     }
     ```
  3. Add this test inside `mod tests` (after `version_file_parse_error_on_bad_toml`):
     ```rust
     #[test]
     fn resolve_claude_project_dir_substitutes_literal() {
         let root = Path::new("/tmp/proj");
         assert_eq!(
             resolve_claude_project_dir("${CLAUDE_PROJECT_DIR}/hooks/x.sh", root),
             PathBuf::from("/tmp/proj/hooks/x.sh"),
             "portable literal must expand to project_root + suffix"
         );
         assert_eq!(
             resolve_claude_project_dir("/abs/hooks/y.sh", root),
             PathBuf::from("/abs/hooks/y.sh"),
             "a path with no literal must be returned unchanged"
         );
     }
     ```
- **Red→green:** add the test referencing the not-yet-added function → `cargo test resolve_claude_project_dir`
  fails to compile (unresolved name). Add the function → test passes.
- **Test:** `cd gatekeeper && cargo test resolve_claude_project_dir` → expect `test result: ok. 1 passed`
- **Commit:** `TOPOLOGY_ROOT="$PWD" git commit -m "feat(doctor): ${CLAUDE_PROJECT_DIR} path-resolution helper (#52)"`

### Task 2: `probe_settings_paths` advisory probe + wiring + integration tests

- **File(s):** `gatekeeper/src/doctor.rs`, `gatekeeper/tests/cli_doctor.rs`
- **Red step (watch it fail):**
  1. In `doctor.rs`, add a stub below `resolve_claude_project_dir`:
     ```rust
     fn probe_settings_paths(_project_root: &Path) {}
     ```
  2. In `doctor.rs`, wire it into `cmd_doctor` immediately after the orphaned-replay-worktree probe
     call `probe_orphaned_replay_worktrees();` (just before the `// ── Summary` block):
     ```rust
     // ── .claude/settings.json stale paths (advisory) ─────────────────────────
     // Warn (never FAIL) when a hook command or GATEKEEPER_BIN path in the project's
     // settings.json no longer exists on disk — catches the worktree-portability break
     // before it surfaces as a runtime PreToolUse hook error. Issue #52.
     probe_settings_paths(&crate::project_root());
     ```
  3. In `cli_doctor.rs`, add the fixture helper (after the `run_with_env` fn, before the first test):
     ```rust
     /// Write a `.claude/settings.json` into `root` with one PreToolUse hook `command` and an
     /// optional `env.GATEKEEPER_BIN`. Used by the settings-path probe tests.
     fn write_settings(root: &Path, hook_command: &str, gatekeeper_bin: Option<&str>) {
         let claude = root.join(".claude");
         fs::create_dir_all(&claude).unwrap();
         let env_block = match gatekeeper_bin {
             Some(b) => format!("\"env\": {{ \"GATEKEEPER_BIN\": \"{b}\" }},"),
             None => String::new(),
         };
         let json = format!(
             "{{\n  {env_block}\n  \"hooks\": {{\n    \"PreToolUse\": [\n      \
              {{ \"hooks\": [ {{ \"type\": \"command\", \"command\": \"{hook_command}\", \
              \"timeout\": 30 }} ] }}\n    ]\n  }}\n}}"
         );
         fs::write(claude.join("settings.json"), json).unwrap();
     }
     ```
  4. In `cli_doctor.rs`, add the three tests at the end of the file (before the final `}` if the file
     ends in a module, else at top level — it is top level here):
     ```rust
     // ── settings.json stale-path probe (advisory, issue #52) ──────────────────

     #[test]
     fn doctor_warns_on_stale_settings_hook_path() {
         let root = scratch_root("settings_stale_hook");
         write_settings(&root, "/nonexistent/topology/hooks/security-scan.sh", None);
         let (code, out) = run(&root, &["doctor"]);
         assert_eq!(
             code, 0,
             "stale settings path is advisory and must not change the exit code; out:\n{out}"
         );
         assert!(
             out.contains(
                 "hook command path does not exist: /nonexistent/topology/hooks/security-scan.sh"
             ),
             "doctor must WARN naming the stale hook path; got:\n{out}"
         );
         let _ = fs::remove_dir_all(&root);
     }

     #[test]
     fn doctor_no_warn_on_resolvable_portable_hook_path() {
         let root = scratch_root("settings_portable_ok");
         // scratch_root() created hooks/test-hook.sh; reference it via the portable literal.
         write_settings(&root, "${CLAUDE_PROJECT_DIR}/hooks/test-hook.sh", None);
         let (code, out) = run(&root, &["doctor"]);
         assert_eq!(code, 0, "out:\n{out}");
         assert!(
             out.contains("settings.json paths: ok"),
             "a resolvable portable hook path must report ok; got:\n{out}"
         );
         assert!(
             !out.contains("WARN: hook command path"),
             "a resolvable portable hook path must not WARN; got:\n{out}"
         );
         let _ = fs::remove_dir_all(&root);
     }

     #[test]
     fn doctor_warns_on_stale_gatekeeper_bin() {
         let root = scratch_root("settings_stale_bin");
         write_settings(
             &root,
             "${CLAUDE_PROJECT_DIR}/hooks/test-hook.sh",
             Some("/nonexistent/gatekeeper/target/release/gatekeeper"),
         );
         let (code, out) = run(&root, &["doctor"]);
         assert_eq!(code, 0, "out:\n{out}");
         assert!(
             out.contains(
                 "GATEKEEPER_BIN path does not exist: \
                  /nonexistent/gatekeeper/target/release/gatekeeper"
             ),
             "doctor must WARN naming the stale GATEKEEPER_BIN path; got:\n{out}"
         );
         let _ = fs::remove_dir_all(&root);
     }
     ```
  5. Run the three tests → they **fail** (stub prints nothing: no `ok`/`WARN` lines).
- **Green step (implement):** replace the `probe_settings_paths` stub body with:
  ```rust
  /// Advisory probe (issue #52): warn when a path referenced in the project's
  /// `.claude/settings.json` — a hook `command` or `env.GATEKEEPER_BIN` — does not exist on disk.
  /// The portable `${CLAUDE_PROJECT_DIR}` literal in hook commands is resolved against
  /// `project_root` before the check, so a valid portable path produces no warning. Prints only —
  /// never increments doctor's failure count (advisory, not a gate).
  fn probe_settings_paths(project_root: &Path) {
      let settings_path = project_root.join(".claude").join("settings.json");
      let raw = match fs::read_to_string(&settings_path) {
          Ok(s) => s,
          Err(_) => {
              println!("settings.json paths: n/a (no .claude/settings.json)");
              return;
          }
      };
      let val: serde_json::Value = match serde_json::from_str(&raw) {
          Ok(v) => v,
          Err(_) => {
              println!("settings.json paths: skipped (.claude/settings.json is malformed)");
              return;
          }
      };

      let mut warnings: Vec<String> = Vec::new();

      // Hook commands: hooks.<event>[].hooks[].command — resolve ${CLAUDE_PROJECT_DIR} first,
      // then check the whole command string as the path (topology never emits arguments).
      if let Some(events) = val.get("hooks").and_then(|h| h.as_object()) {
          for entries in events.values() {
              let arr = match entries.as_array() {
                  Some(a) => a,
                  None => continue,
              };
              for entry in arr {
                  let hook_list = match entry.get("hooks").and_then(|h| h.as_array()) {
                      Some(h) => h,
                      None => continue,
                  };
                  for hook in hook_list {
                      let cmd = match hook.get("command").and_then(|c| c.as_str()) {
                          Some(c) => c,
                          None => continue,
                      };
                      let resolved = resolve_claude_project_dir(cmd, project_root);
                      if !resolved.exists() {
                          warnings.push(format!(
                              "settings.json paths: WARN: hook command path does not exist: {} \
                               (stale clone/worktree — reinstall the framework or re-run \
                               'gatekeeper adapt --harness claude' to repoint)",
                              resolved.display()
                          ));
                      }
                  }
              }
          }
      }

      // GATEKEEPER_BIN: checked as-is — the env block carries no ${CLAUDE_PROJECT_DIR}
      // interpolation (post #50/#51 it is absent or an absolute path).
      if let Some(bin) = val
          .get("env")
          .and_then(|e| e.get("GATEKEEPER_BIN"))
          .and_then(|b| b.as_str())
      {
          if !Path::new(bin).exists() {
              warnings.push(format!(
                  "settings.json paths: WARN: GATEKEEPER_BIN path does not exist: {bin} \
                   (security-scan.sh will fall back to a repo/PATH build; re-run \
                   'gatekeeper adapt --harness claude' to repoint)"
              ));
          }
      }

      if warnings.is_empty() {
          println!("settings.json paths: ok");
      } else {
          for w in warnings {
              println!("{w}");
          }
      }
  }
  ```
- **Test:** `cd gatekeeper && cargo test --test cli_doctor settings` → expect `test result: ok. 3 passed`
  (the three `doctor_*_settings_*` / `*_gatekeeper_bin` tests).
- **Commit:** `TOPOLOGY_ROOT="$PWD" git commit -m "feat(doctor): warn on stale settings.json hook/GATEKEEPER_BIN paths (#52)"`

### Task 3: Full-suite regression + fmt/clippy gate

- **File(s):** none (verification only).
- **Change:** none.
- **Test (run all three, all must be clean):**
  ```bash
  cd gatekeeper
  cargo fmt --check
  cargo clippy --all-targets -- -D warnings
  cargo test --quiet            # TOPOLOGY_ROOT unset
  ```
  Expect: `cargo fmt --check` silent (exit 0); clippy no warnings; `cargo test` adds 4 new tests
  to the 547 baseline (1 unit in `doctor.rs` + 3 integration in `cli_doctor.rs`) → expect
  `551 passed, 0 failed`. If the number differs, investigate before proceeding.
- **Commit:** none (no diff — this task only gates the work already committed in Tasks 1–2).

## Done criteria (maps to spec acceptance criteria)

- AC1 (WARN names path): `doctor_warns_on_stale_settings_hook_path` + `doctor_warns_on_stale_gatekeeper_bin`.
- AC2 (no false positive on portable): `doctor_no_warn_on_resolvable_portable_hook_path`.
- AC3 (helper unit test): `resolve_claude_project_dir_substitutes_literal`.
- AC4 (fixture integration tests): the three `cli_doctor.rs` tests above.
- AC-advisory (exit code unchanged): `assert_eq!(code, 0, …)` in each WARN test.
