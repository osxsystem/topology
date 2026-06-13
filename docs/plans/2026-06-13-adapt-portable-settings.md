# Plan: portable adapt-generated settings.json (#50 + #51)

- **Date:** 2026-06-13 · **Feature slug:** adapt-portable-settings
- **Design:** [docs/specs/2026-06-13-adapt-portable-settings.md](../specs/2026-06-13-adapt-portable-settings.md) (approved)
- **Research:** [docs/research/2026-06-13-adapt-portable-settings.md](../research/2026-06-13-adapt-portable-settings.md)

## Baseline (clean)

`env -u GATEKEEPER_BIN -u TOPOLOGY_ROOT cargo test` (worktree, debug) → **all suites pass, 0 failed**
(281 bin-unit + integration; 3 `#[ignore]`d = the hollow fixtures). The `env -u` scrub is mandatory
locally: a stale inherited `GATEKEEPER_BIN`/`TOPOLOGY_ROOT` perturbs the `cli_doctor` probe. CI has no
such var. **Every test command below is prefixed `env -u GATEKEEPER_BIN -u TOPOLOGY_ROOT`.** Unit tests
run via `--bin gatekeeper` (the crate has no lib target); e2e via `--test cli_adapt`.

## Files to touch

| File | Responsibility |
|------|----------------|
| `gatekeeper/src/adapt.rs` | `merge_claude_settings` → `bin: Option<&str>` (Some=set, None=remove); `build_claude_hooks` → `(framework_root, in_framework: bool)`; `build_claude` upstream-caller fix; `cmd_adapt` claude branch wiring + `disk_ok` rewrite; doc comment; in-file unit tests. |
| `gatekeeper/tests/cli_adapt.rs` | NEW e2e: `dogfood_settings_are_portable`, `readapt_removes_stale_gatekeeper_bin`. Governed tests unchanged. |
| `CHANGELOG.md` | Unreleased note (#50/#51 + the intentional dogfood `GATEKEEPER_BIN` contract narrowing). |

No ADR: this is a bug fix to path generation, not an architectural decision (surgical-changes). No new
deps; pure-Rust; lanes preserved.

## Delegation

Per `delegate-coding-to-sonnet` memory: tests via `test-engineer-tdd`, production code via
`feature-implementer` (fallback: `general-purpose` on Opus reading the agent `.md`, never Sonnet). The
main loop watches each red, validates each green, and commits serially.

## Tasks (TDD order — test first, watch red, implement, watch green)

### Task 1 — `merge_claude_settings(bin: Option<&str>)`: Some sets, None removes (#51 core)

- **Test (test-engineer-tdd), `adapt.rs` `#[cfg(test)]` module:**
  - Update the four existing callers (lines 1425, 1435, 1451, 1462) from `"/fw/bin/gatekeeper"` /
    `"/bin/gk"` to `Some("/fw/bin/gatekeeper")` / `Some("/bin/gk")`.
  - NEW `merge_settings_none_bin_removes_gatekeeper_bin`: existing =
    `json!({"env":{"GATEKEEPER_BIN":"old_path","MY_VAR":"hello"}})`, hooks = `json!({})`, bin = `None`
    → assert `result["env"].get("GATEKEEPER_BIN").is_none()` **and** `result["env"]["MY_VAR"] ==
    "hello"`.
  - NEW `merge_settings_none_bin_absent_env_stays_absent`: existing = `json!({})`, hooks = `json!({})`,
    bin = `None` → assert `result.get("env").and_then(|e| e.get("GATEKEEPER_BIN")).is_none()` (no
    `GATEKEEPER_BIN`; an `env` key may be absent).
- **Watch red:** `env -u GATEKEEPER_BIN -u TOPOLOGY_ROOT cargo test --bin gatekeeper merge_settings`
  → the two NEW tests fail to compile (signature is still `&str`).
- **Impl (feature-implementer), `adapt.rs:155-187`:** change signature to
  `bin: Option<&str>`. Keep `obj.insert("hooks", hooks)`. Replace the env block with:
  ```rust
  match bin {
      Some(b) => {
          let env = obj.entry("env")
              .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
          if let serde_json::Value::Object(env_map) = env {
              env_map.insert("GATEKEEPER_BIN".to_owned(), serde_json::Value::String(b.to_owned()));
          } else {
              let mut env_map = serde_json::Map::new();
              env_map.insert("GATEKEEPER_BIN".to_owned(), serde_json::Value::String(b.to_owned()));
              obj.insert("env".to_owned(), serde_json::Value::Object(env_map));
          }
      }
      None => {
          if let Some(serde_json::Value::Object(env_map)) = obj.get_mut("env") {
              env_map.remove("GATEKEEPER_BIN"); // leaves an empty {} in place by design (G4)
          }
      }
  }
  ```
  Update the doc comment (`adapt.rs:148-154`): replace the "sets `obj["env"]["GATEKEEPER_BIN"] = bin`"
  line with: "`bin = Some(b)` sets `env.GATEKEEPER_BIN = b`; `bin = None` removes it (in-framework
  case), preserving all other `env` keys and leaving an empty `env` object in place."
- **Watch green:** same command → all `merge_settings*` pass.
- **Commit:** `feat(adapt): merge_claude_settings takes Option bin; None removes GATEKEEPER_BIN (#51)`

### Task 2 — `build_claude_hooks(framework_root, in_framework: bool)` (#50 core)

- **Test (test-engineer-tdd), `adapt.rs` test module:**
  - NEW `build_claude_hooks_governed_uses_absolute`: `let root = fixture("gov");
    let h = build_claude_hooks(&root, false).unwrap(); let s = h.to_string();` → assert
    `s.contains(&root.join("hooks/security-scan.sh").display().to_string())`.
  - NEW `build_claude_hooks_in_framework_uses_project_dir_var`: `build_claude_hooks(&root, true)` →
    assert the `UserPromptSubmit[0].hooks[0].command == "${CLAUDE_PROJECT_DIR}/hooks/skill-activation.sh"`
    and `PreToolUse[0].hooks[0].command == "${CLAUDE_PROJECT_DIR}/hooks/security-scan.sh"`, and
    `!h.to_string().contains(root.to_str().unwrap())`.
  - Update `claude_wires_both_hooks` (`adapt.rs:1087-1100`, G1) — split into two:
    - `claude_wires_both_hooks_governed`: `build_claude_hooks(&root, false)` +
      `merge_claude_settings(None, hooks, Some("/fw/bin/gatekeeper"))` → keep existing asserts
      (UserPromptSubmit, PreToolUse, both `.sh`, `Bash|Write|Edit|MultiEdit`, **contains root**),
      plus `s.contains("GATEKEEPER_BIN")`.
    - `claude_wires_both_hooks_in_framework`: `build_claude_hooks(&root, true)` +
      `merge_claude_settings(None, hooks, None)` → assert `s.contains("${CLAUDE_PROJECT_DIR}/hooks/")`,
      `!s.contains(root.to_str().unwrap())`, `!s.contains("GATEKEEPER_BIN")`.
- **Watch red:** `env -u GATEKEEPER_BIN -u TOPOLOGY_ROOT cargo test --bin gatekeeper build_claude_hooks`
  → NEW tests fail to compile (signature is one-arg).
- **Impl (feature-implementer), `adapt.rs:515-531`:** new signature; build each command via a closure:
  ```rust
  fn build_claude_hooks(framework_root: &Path, in_framework: bool) -> Result<serde_json::Value, String> {
      require_agents_md(framework_root)?;
      let cmd = |name: &str| -> String {
          if in_framework {
              format!("${{CLAUDE_PROJECT_DIR}}/hooks/{name}")
          } else {
              framework_root.join("hooks").join(name).display().to_string()
          }
      };
      let skill_activation = cmd("skill-activation.sh");
      let security_scan = cmd("security-scan.sh");
      Ok(serde_json::json!({ /* same shape as today, using the two vars */ }))
  }
  ```
- **Watch green:** `env -u GATEKEEPER_BIN -u TOPOLOGY_ROOT cargo test --bin gatekeeper build_claude_hooks`
  and `… claude_wires_both_hooks` → all pass.
- **Commit:** `feat(adapt): build_claude_hooks emits ${CLAUDE_PROJECT_DIR} paths in-framework (#50)`

### Task 3 — `build_claude` upstream-caller fix (G2)

- **Impl (feature-implementer), `adapt.rs:535-538`:** replace the body's
  `build_claude_hooks(framework_root)?;` with `require_agents_md(framework_root)?;` (the call existed
  only to trigger the AGENTS.md check; its JSON result was discarded). `build_claude` still returns
  `Ok(Vec::new())`. This removes the only non-claude-branch caller of `build_claude_hooks`, so the new
  two-arg signature is reached only where `in_framework` is known.
- **Watch green (no behavior change — compile + existing coverage):**
  `env -u GATEKEEPER_BIN -u TOPOLOGY_ROOT cargo test --bin gatekeeper` builds, and
  `… --test cli_adapt missing_agents_md_exits_2` still passes (the AGENTS.md hard-error path).
- **Commit:** folded into Task 4's commit (single-line change in the same function family).

### Task 4 — `cmd_adapt` claude-branch wiring + `disk_ok` rewrite (#50 + #51 integration)

- **Test (test-engineer-tdd), `gatekeeper/tests/cli_adapt.rs`:**
  - NEW `dogfood_settings_are_portable` (roots-equal via `run(&root, …)`, `cli_adapt.rs:41`):
    ```rust
    let root = scratch_root("portable");
    assert_eq!(run(&root, &["adapt", "--harness", "claude"]).0, 0);
    let v: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(root.join(".claude/settings.json")).unwrap()).unwrap();
    assert_eq!(v["hooks"]["PreToolUse"][0]["hooks"][0]["command"],
               "${CLAUDE_PROJECT_DIR}/hooks/security-scan.sh");
    assert_eq!(v["hooks"]["UserPromptSubmit"][0]["hooks"][0]["command"],
               "${CLAUDE_PROJECT_DIR}/hooks/skill-activation.sh");
    let s = fs::read_to_string(root.join(".claude/settings.json")).unwrap();
    assert!(!s.contains(root.to_str().unwrap()), "no absolute clone path baked in");
    assert!(v["env"].get("GATEKEEPER_BIN").is_none(), "GATEKEEPER_BIN dropped in-framework");
    let _ = fs::remove_dir_all(&root);
    ```
  - NEW `readapt_removes_stale_gatekeeper_bin` (verify-gate reproduce→resolve, roots-equal):
    ```rust
    let root = scratch_root("stale");
    fs::create_dir_all(root.join(".claude")).unwrap();
    fs::write(root.join(".claude/settings.json"),
        "{\n  \"env\": { \"GATEKEEPER_BIN\": \"/deleted/worktree/bin/gatekeeper\" }\n}\n").unwrap();
    assert_eq!(run(&root, &["adapt", "--harness", "claude"]).0, 0);
    let v: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(root.join(".claude/settings.json")).unwrap()).unwrap();
    assert!(v["env"].get("GATEKEEPER_BIN").is_none(), "re-adapt clears the stale pin");
    let _ = fs::remove_dir_all(&root);
    ```
- **Watch red:** `env -u GATEKEEPER_BIN -u TOPOLOGY_ROOT cargo test --test cli_adapt dogfood_settings_are_portable readapt_removes_stale_gatekeeper_bin`
  → both fail (today's output is absolute + carries the pin).
- **Impl (feature-implementer), `adapt.rs` claude branch (`830-913`):**
  1. After `roots_differ` (`817`), add `let in_framework = !roots_differ;`.
  2. Change the hooks build (`831`) to `build_claude_hooks(read_root, in_framework)`.
  3. Keep `let bin = read_root.join("bin").join("gatekeeper").display().to_string();` (`838`).
     Add `let bin_opt: Option<&str> = if roots_differ { Some(bin.as_str()) } else { None };`.
  4. Rewrite the `disk_ok` closure (`870-881`, G3) to:
     ```rust
     let disk_ok = existing.as_ref().and_then(|v| v.as_object()).map(|obj| {
         let hooks_ok = obj.get("hooks") == Some(&hooks);
         let cur = obj.get("env").and_then(|e| e.get("GATEKEEPER_BIN")).and_then(|b| b.as_str());
         let bin_ok = match bin_opt { Some(b) => cur == Some(b), None => cur.is_none() };
         hooks_ok && bin_ok
     }).unwrap_or(false);
     ```
  5. Change the merge call (`889`) to `merge_claude_settings(existing, hooks, bin_opt)`.
- **Watch green:** the two NEW e2e tests pass; **and** the no-regression governed guards still pass:
  `env -u GATEKEEPER_BIN -u TOPOLOGY_ROOT cargo test --test cli_adapt
  adapt_writes_to_project_not_framework ac4_settings_no_clobber ac5_gatekeeper_bin_value
  claude_writes_hook_settings check_mode_is_idempotent_then_detects_drift`.
- **Commit:** `fix(adapt): portable dogfood settings — ${CLAUDE_PROJECT_DIR} hooks + drop pinned bin (#50, #51)`

### Task 5 — full-suite green + CHANGELOG

- **Impl (main loop), `CHANGELOG.md`:** under `## [Unreleased]`, add a `### Fixed` entry:
  "adapt now generates portable `.claude/settings.json` for the in-framework (dogfood) case —
  `${CLAUDE_PROJECT_DIR}`-relative hook paths and no pinned `GATEKEEPER_BIN`, so the wiring survives
  deletion of a sibling worktree (#50, #51). adapt deletes the adapt-owned `GATEKEEPER_BIN` key in this
  case; all user `env` keys are preserved. Governed downstream projects are unchanged; cross-tree
  generation is tracked in #54."
- **Watch green (finish-gate dry run):** `env -u GATEKEEPER_BIN -u TOPOLOGY_ROOT cargo test`
  → all suites pass, 0 failed.
- **Commit:** `docs(changelog): portable adapt dogfood settings (#50, #51)`

## Gate exits after the loop

- **Verify gate:** record reproduce→resolve at `docs/verify/2026-06-13-adapt-portable-settings.md`
  (symptom: today's roots-equal output embeds the absolute scratch path + a pin that dangles when the
  dir is deleted; resolved: `${CLAUDE_PROJECT_DIR}` form, no absolute clone path, no `GATEKEEPER_BIN`).
  `readapt_removes_stale_gatekeeper_bin` is the executable reproduce→resolve.
- **Review gate:** fresh-context critic at `docs/reviews/2026-06-13-adapt-portable-settings.md`, bound
  to the merge-base, both rubric dimensions, no blocking findings.
- **Finish gate:** `gatekeeper check finish -- env -u GATEKEEPER_BIN -u TOPOLOGY_ROOT cargo test`.
