VERDICT: pass
HEAD: 314ba5ebe61209e94d2fdd6e7d4334266c18f68d
BASE: a4a997363fc145fb327f15a4c5c724474ff413c0

# Review: adapt-portable-settings (2026-06-13)

## Blocking findings
None.

## Non-blocking notes
- gatekeeper/src/adapt.rs:821-822 — pre-existing block comment ("hook paths embedded in config point at read_root (the framework)...") is now only accurate for the governed case, not in-framework. Out of scope for this branch: these lines are not in the BASE...HEAD diff, and the prior review already flagged the analogous staleness. Mentioned, not deleted (diff-traceability).

## Criteria checked
### Spec/plan
- Doc-only follow-up delta (8769609..HEAD) — `git diff 8769609..HEAD` touches only gatekeeper/src/adapt.rs (+6/-3), exactly the two doc comments on `build_claude_hooks` (adapt.rs:519-522) and `build_claude` (adapt.rs:553-554). No code or behavior change; the function bodies, signatures, and call sites are byte-identical to 8769609.
- `build_claude_hooks` doc accuracy — doc says in-framework emits the portable literal `${CLAUDE_PROJECT_DIR}/hooks/<name>.sh`, else absolute rooted at framework_root. Matches the `cmd` closure (adapt.rs:530-536): `in_framework` → `format!("${{CLAUDE_PROJECT_DIR}}/hooks/{name}")`, else `framework_root.join("hooks").join(name)`.
- `build_claude` doc accuracy — doc says it returns an empty list after the AGENTS.md check, hooks JSON built later in cmd_adapt where `in_framework` is known. Matches the body (adapt.rs:555-558): `require_agents_md(framework_root)?; Ok(Vec::new())`, with `build_claude_hooks(read_root, in_framework)` invoked in the claude branch of cmd_adapt (adapt.rs:854).
- In-framework portable settings (#50/#51 AC) — `bin_opt = if roots_differ { Some(bin) } else { None }` (adapt.rs:865); e2e `dogfood_settings_are_portable` (cli_adapt.rs) asserts both hook commands are the `${CLAUDE_PROJECT_DIR}` literal, no absolute clone path baked in, and `env.GATEKEEPER_BIN` absent. Green.
- Stale-pin removal — e2e `readapt_removes_stale_gatekeeper_bin` (cli_adapt.rs) confirms re-adapt over a settings.json carrying a stale `GATEKEEPER_BIN` clears it. Unit `merge_settings_none_bin_removes_gatekeeper_bin` confirms `merge_claude_settings(Some(existing), _, None)` removes GATEKEEPER_BIN while preserving sibling env key `MY_VAR`.

### Standards
- Governed path unweakened — ac4 (`ac4_settings_no_clobber`) and ac5 (`ac5_gatekeeper_bin_value`) run with distinct framework/project roots (`roots_differ=true`) and still assert `env.GATEKEEPER_BIN` is present (ac4) and equals `<framework>/bin/gatekeeper` (ac5). `adapt_writes_to_project_not_framework` likewise untouched. The cli_adapt.rs diff is additive only (+35, two new tests); no deletions or assertion relaxations.
- Drift logic not inverted — `disk_ok` None-arm (adapt.rs:902-905): `bin_ok = match bin_opt { Some(b) => cur == Some(b), None => cur.is_none() }`, so a *present* GATEKEEPER_BIN in the in-framework case yields `bin_ok=false` → `disk_ok=false` → drift, triggering re-merge / `--check` DRIFT report. Correct direction. Unit `merge_settings_none_bin_absent_env_stays_absent` covers the absent-env None case.
- Test suite green — `env -u GATEKEEPER_BIN -u TOPOLOGY_ROOT cargo test --manifest-path gatekeeper/Cargo.toml`: all binaries pass, 0 failed (286 lib unit tests + integration binaries incl. cli_adapt). No new warnings introduced by the delta.
- Simplicity — the None/Some(bin) branch in `merge_claude_settings` and the `bin_opt` thread-through are the minimal expression of "drop the pin in-framework, keep it governed"; no speculative knobs added.
