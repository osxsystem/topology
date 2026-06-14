# Plan: path-triggered routing — Slice 2 (live PostToolUse hook)

- **Date:** 2026-06-14
- **Feature slug:** path-routing
- **Design:** docs/specs/2026-06-14-path-routing.md (approved; slice 2 = "PostToolUse hook injects required-skill context when edits touch trigger paths").
- **Baseline:** main `2dbd8d1` (Slice 1 merged); full suite 559/0 green.
- **Scope:** make path-routing LIVE — a PostToolUse hook that, when a Write/Edit touches a `pathTriggers` glob, prints the required-skill reminder. Slice 3 (≥50-case eval harness) remains deferred to a maintainer checkpoint.

## Design refinement (plan-level, within the approved design)

The hook needs the touched `file_path` from the PostToolUse JSON. Extracting it in **Rust** (a `route --hook` mode) keeps decision/parse logic out of bash (three-language-lanes); the hook stays thin glue. `route --hook` mirrors `scan --hook`'s JSON handling (scan.rs `HookEvent`/`ToolInput`), but with route.rs's own minimal deserialize struct so the protected `scan.rs` is untouched.

## Files
- `gatekeeper/src/route.rs` — add `route_by_hook_json(stdin) -> Vec<(skill,enforcement)>`: parse `{tool_name, tool_input:{file_path}}`, route by `file_path`. Unit-tested.
- `gatekeeper/src/main.rs` — PROTECTED (override). Extend `cmd_route` to accept `--hook` (read stdin JSON, call `route::route_by_hook_json`); add `--hook` to `known_flags` + usage.
- `hooks/post-tool-routing.sh` — NEW (not protected). Advisory PostToolUse hook: resolve gatekeeper (mirror `skill-activation.sh` resolution + fail-open), pipe stdin to `gatekeeper route --hook`, print result; **always exit 0**.
- `gatekeeper/src/adapt.rs` — NOT protected. Add a `PostToolUse` block (matcher `Write|Edit|MultiEdit`, command `post-tool-routing.sh`) to `build_claude_hooks` (adapt.rs:551-561).
- `gatekeeper/tests/cli_route.rs` — add `route --hook` cases.
- adapt.rs tests (`claude_wires_both_hooks_*`) — update to expect the third hook.
- `docs/USER-GUIDE.md` — document `route --hook` (keep cli_doc_sync green).

## Tasks

### Task 1: route_by_hook_json (RED→GREEN)
- **File:** `gatekeeper/src/route.rs`.
- **Test first:** `route_by_hook_json_extracts_path` — feed `{"tool_name":"Edit","tool_input":{"file_path":"hooks/x.sh"}}` + a rules value with `security-scanning` pathTriggers `["hooks/*"]`; assert it returns `[("security-scanning","require")]`; a JSON with a non-trigger `file_path` (`README.md`) returns `[]`; malformed JSON / missing file_path returns `[]` (never panics). **RED** (fn absent).
- **Change:** add `pub(crate) fn route_by_hook_json(rules:&serde_json::Value, stdin:&str) -> Vec<(String,String)>`: `serde_json::from_str` into a local `#[derive(Deserialize)] struct HookEvent { tool_input: Option<ToolInput> }` / `struct ToolInput { file_path: Option<String> }`; on any parse miss or absent path, return empty; else `route_by_paths(rules, &[fp])`.
- **Test:** green.
- **Commit:** `feat(routing): route_by_hook_json — extract file_path from PostToolUse JSON`

### Task 2: route --hook in cmd_route (RED→GREEN) — PROTECTED main.rs (override)
- **Files:** `gatekeeper/src/main.rs`, `gatekeeper/tests/cli_route.rs`.
- **Test first (`cli_route.rs`):** `route_hook_routes_from_json` — pipe the Edit-on-`hooks/x.sh` JSON to `route --hook` (scratch root with security-scanning pathTriggers); assert stdout has `- security-scanning [require]`, exit 0; a non-trigger JSON → "no skills" line, exit 0; `route` with neither `--paths`/`--staged-paths`/`--hook` still → exit 2. **RED**.
- **Change:** add `"--hook"` to the `route` `known_flags` + usage; in `cmd_route`, branch: if `--hook`, read stdin, call `route::route_by_hook_json`; reuse the same print block.
- **Test:** `cargo test --test cli_route` green.
- **Commit (override):** `feat(routing): gatekeeper route --hook (PostToolUse JSON)` + body `protected-path override (main.rs) authorized under the 2026-06-14 grant`.

### Task 3: post-tool-routing.sh (RED→GREEN)
- **File:** `hooks/post-tool-routing.sh` (new), `gatekeeper/tests/cli_route.rs` or a shell assertion.
- **Test first:** a shell check (in `cli_route.rs` via spawning the script, or a `scripts/test-*.sh`-style assert): pipe the Edit-on-`hooks/x.sh` JSON to `post-tool-routing.sh` with `GATEKEEPER_BIN` set to the built binary and `CLAUDE_PLUGIN_ROOT` at the framework root; assert it prints `security-scanning` and exits 0; malformed stdin → exit 0 (fail-open). **RED** (script absent).
- **Change:** write `hooks/post-tool-routing.sh` mirroring `skill-activation.sh` (binary resolution order + fail-open), piping stdin to `"$GK" route --hook`; always `exit 0`. `chmod +x`.
- **Test:** green; `shellcheck` clean.
- **Commit:** `feat(routing): advisory PostToolUse hook post-tool-routing.sh`

### Task 4: wire PostToolUse in adapt.rs (RED→GREEN)
- **Files:** `gatekeeper/src/adapt.rs`.
- **Test first:** update `claude_wires_both_hooks_in_framework`/`_governed` (rename intent to "all hooks") to also assert a `PostToolUse` entry with matcher `Write|Edit|MultiEdit` and command ending `post-tool-routing.sh`. **RED** (only two hooks today).
- **Change:** in `build_claude_hooks`, add `let post_tool = cmd("post-tool-routing.sh");` and a `"PostToolUse": [ { "matcher": "Write|Edit|MultiEdit", "hooks": [ { "type":"command", "command": post_tool, "timeout": 30 } ] } ]` key to the JSON.
- **Test:** `cargo test adapt::` green.
- **Commit:** `feat(adapt): wire PostToolUse path-routing hook into generated settings`

### Task 5: docs + full suite + lints + dogfood activation
- **Files:** `docs/USER-GUIDE.md` (`route --hook` row).
- **Change:** add the `route --hook` reference row (cli_doc_sync).
- **Test:** `cargo test` all green; `cargo fmt --check`; `cargo clippy --all-targets -- -D warnings`; `shellcheck hooks/*.sh scripts/*.sh`; `cargo test --test cli_doc_sync`.
- **Dogfood:** after build, run `gatekeeper adapt --harness claude` to wire the framework's own (untracked, local) `.claude/settings.json` PostToolUse — verifies the live path end-to-end; not a committed change.
- **Commit:** `docs(routing): document route --hook (cli_doc_sync)`

## After this plan
Verify → review (fresh-context) → finish → PR for Slice 2. Then **checkpoint**: Slice 3 (router eval harness, ≥50 labeled prompts, recall≥0.90/precision≥0.80) awaits maintainer steer on label subjectivity (R1).
