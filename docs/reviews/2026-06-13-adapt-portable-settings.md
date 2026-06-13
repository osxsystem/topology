VERDICT: pass
HEAD: 518ba293e5b564895e7a2dbe48968652132d8ddf
BASE: a4a997363fc145fb327f15a4c5c724474ff413c0

# Review: adapt-portable-settings (2026-06-13)

## Blocking findings
None.

## Non-blocking notes
- gatekeeper/src/adapt.rs:821-822 — pre-existing block comment ("hook paths embedded in config point at read_root (the framework)...") is now only accurate for the governed case, not in-framework. Out of scope for this branch: these lines are not in the 8769609..HEAD delta, and the prior fresh-context review already flagged the analogous staleness. Mentioned, not deleted (diff-traceability).
- The `cargo fmt` amend reflowed several multi-line `merge_claude_settings(...)` calls and two `assert!(...)` macros in adapt.rs/cli_adapt.rs onto extra lines. Pure whitespace; `git diff -w 8769609..HEAD` collapses them to nothing.

## Criteria checked
### Spec/plan
- Follow-up delta scope (8769609..HEAD) — `git diff -w 8769609..HEAD` over code/test files reduces to exactly two doc-comment edits in gatekeeper/src/adapt.rs: the `build_claude_hooks` doc (adapt.rs:519-525) and the `build_claude` doc (adapt.rs:553-554). No code, signature, control-flow, or assertion change. The full (non-`-w`) diff adds only `cargo fmt` line-wrapping in adapt.rs + tests/cli_adapt.rs. The docs/reviews/ entry in the diff is this review artifact being overwritten (expected, tracked).
- `build_claude_hooks` doc accuracy — doc says in-framework emits the portable literal `${CLAUDE_PROJECT_DIR}/hooks/<name>.sh`, else absolute rooted at `framework_root`. Matches the `cmd` closure at HEAD (adapt.rs:531-541): `in_framework` → `format!("${{CLAUDE_PROJECT_DIR}}/hooks/{name}")`, else `framework_root.join("hooks").join(name).display().to_string()`.
- `build_claude` doc accuracy — doc says it returns an empty list after the AGENTS.md presence check, hooks JSON built later in the claude branch of cmd_adapt where `in_framework` is known. Matches the body: `require_agents_md(framework_root)?; Ok(Vec::new())`.
- In-framework portable settings (#50/#51 AC) — `bin_opt = if roots_differ { Some(bin.as_str()) } else { None }` (adapt.rs:869-873); e2e `dogfood_settings_are_portable` asserts both hook commands are the `${CLAUDE_PROJECT_DIR}` literal, no absolute clone path baked in, and `env.GATEKEEPER_BIN` absent. Green.
- Stale-pin removal — e2e `readapt_removes_stale_gatekeeper_bin` confirms re-adapt over a settings.json carrying a stale `GATEKEEPER_BIN` clears it; unit `merge_settings_none_bin_removes_gatekeeper_bin` confirms `merge_claude_settings(Some(existing), _, None)` removes GATEKEEPER_BIN while preserving sibling env key `MY_VAR`. Green.

### Standards
- Governed path unweakened — `ac4_settings_no_clobber`, `ac5_gatekeeper_bin_value`, and `adapt_writes_to_project_not_framework` (cli_adapt.rs) are byte-identical between 8769609 and HEAD (their bodies do not appear in the diff). They still assert `env.GATEKEEPER_BIN` present (ac4) and equal to `<framework>/bin/gatekeeper` (ac5) under `roots_differ=true`. No assertion relaxation.
- Drift logic not inverted — `disk_ok` bin_ok arm at HEAD (adapt.rs:910-913): `match bin_opt { Some(b) => cur == Some(b), None => cur.is_none() }`. None-arm = `cur.is_none()`, so a *present* GATEKEEPER_BIN in the in-framework case yields `bin_ok=false` → `disk_ok=false` → drift (re-merge / `--check` DRIFT). Correct direction.
- fmt/clippy clean; doc+fmt delta non-semantic — `env -u GATEKEEPER_BIN -u TOPOLOGY_ROOT cargo fmt --check --manifest-path gatekeeper/Cargo.toml` exits 0. Test suite green: `… cargo test` reports 0 failed across all binaries (286 lib unit tests + integration binaries incl. cli_adapt). No new warnings from the delta.
- Simplicity — the doc comments are the minimal accurate description of the existing in-framework/governed split; no logic or speculative knobs added by this follow-up.
