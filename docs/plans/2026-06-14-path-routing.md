# Plan: path-triggered routing — Slice 1 (core capability)

- **Date:** 2026-06-14
- **Feature slug:** path-routing
- **Design:** docs/specs/2026-06-14-path-routing.md (Status: approved)
- **Baseline:** tests green at branch `phase15-path-routing` HEAD (no Rust touched yet; full suite 553/0 as on `main`).
- **Scope of THIS plan:** Slice 1 only — `pathTriggers` schema + `route.rs` + `gatekeeper route --paths/--staged-paths` CLI + `cli_route.rs` tests. **Slices 2 (PostToolUse hook + settings.json wiring) and 3 (≥50-case eval harness) are deferred to a maintainer checkpoint** and will get their own plans; they are out of scope here, not placeholders.

## Files
- `hooks/skill-rules.json` — add `pathTriggers: { globs: [...] }` to `security-scanning` (unprotected data).
- `gatekeeper/src/route.rs` — NEW, unprotected. `route_by_paths()` + a dep-free `path_glob_match()` + unit tests.
- `gatekeeper/src/main.rs` — PROTECTED (override). Add `mod route;`, one `SubcommandSpec` row for `route`, and a `cmd_route` handler.
- `gatekeeper/tests/cli_route.rs` — NEW. Functional tests for the subcommand.

## Tasks

### Task 1: pathTriggers schema in skill-rules.json
- **File:** `hooks/skill-rules.json`.
- **Change:** add to the `security-scanning` object, as a sibling of `promptTriggers`:
  ```json
  "pathTriggers": {
    "globs": ["hooks/*", "security/*", "gatekeeper/src/scan.rs", ".claude/settings.json", "*secret*"]
  }
  ```
- **Test:** `python3 -c "import json;json.load(open('hooks/skill-rules.json'))"` → exit 0 (valid JSON).
- **Commit:** `feat(routing): pathTriggers globs for security-scanning`

### Task 2: route.rs — path glob matcher (RED→GREEN)
- **File:** `gatekeeper/src/route.rs` (new).
- **Test first:** in `#[cfg(test)] mod tests`, add `path_glob_match_parity`: assert `path_glob_match("hooks/x.sh","hooks/*")==true`, `path_glob_match("gatekeeper/src/scan.rs","gatekeeper/src/scan.rs")==true`, `path_glob_match("src/a/secret.txt","*secret*")==true`, `path_glob_match("README.md","hooks/*")==false`. Run `cargo test --manifest-path gatekeeper/Cargo.toml route::` → **RED** (module absent).
- **Change:** implement `pub(crate) fn path_glob_match(path: &str, glob: &str) -> bool` mirroring the documented semantics of `scan.rs:498-527` (trailing-`/` = directory prefix; `*` = wildcard segment, first anchored at start, last at end; no-`*` = exact). Add a doc comment cross-referencing `scan.rs:498-527` and noting the parity test (R3).
- **Test:** `cargo test --manifest-path gatekeeper/Cargo.toml route::path_glob_match_parity` → green.
- **Commit:** `feat(routing): dep-free path glob matcher in route.rs (parity-tested)`

### Task 3: route.rs — route_by_paths (RED→GREEN)
- **File:** `gatekeeper/src/route.rs`.
- **Test first:** `route_by_paths_matches_security`: build a `serde_json::json!` rules value with a `security-scanning` skill carrying `pathTriggers.globs=["hooks/*"]` and `enforcement="require"`; assert `route_by_paths(&rules, &["hooks/x.sh"])` returns `[("security-scanning","require")]` and `route_by_paths(&rules,&["README.md"])` returns `[]`. **RED**.
- **Change:** `pub(crate) fn route_by_paths(rules: &serde_json::Value, paths: &[&str]) -> Vec<(String,String)>` — mirror `route()` (main.rs:657-685) but read `pathTriggers.globs`; a skill matches if ANY of its globs matches ANY path; dedupe + `sort()`.
- **Test:** green.
- **Commit:** `feat(routing): route_by_paths over pathTriggers globs`

### Task 4: route subcommand in main.rs (RED→GREEN) — PROTECTED, override
- **Files:** `gatekeeper/src/main.rs`, `gatekeeper/tests/cli_route.rs` (new).
- **Test first (`cli_route.rs`):** mirror `cli_help_flags.rs` (`scratch_root()` with a `hooks/skill-rules.json` carrying a `security-scanning` pathTriggers). Cases: `route --paths hooks/x.sh` prints `- security-scanning [require]`, exit 0; `route --paths README.md` prints the "no skills" line, exit 0; `route --help` exit 0; `route --bogus` exit 2. Run `cargo test --test cli_route` → **RED** (no `route` subcommand).
- **Change:** in `main.rs`: add `mod route;`; add a `SubcommandSpec { name:"route", usage:"USAGE:\n  gatekeeper route --paths <p1> [<p2>...]\n  gatekeeper route --staged-paths", synopsis:"Route skills by file paths.", known_flags:&["--paths","--staged-paths"], handler:|a| cmd_route(a) }`; implement `fn cmd_route(args)`: honor `check_help_or_unknown`; collect paths from `--paths <rest>` or `git diff --cached --name-only` for `--staged-paths`; load `hooks/skill-rules.json` like `cmd_activate`; call `route::route_by_paths`; print the same grammar as `cmd_activate` (`Topology: …` / `Routed skills for these paths:` / `- name [enf]`).
- **Test:** `cargo test --test cli_route` green; `cargo test cli_doc_sync` green (new subcommand auto-documented).
- **Commit (override):** `feat(routing): gatekeeper route --paths/--staged-paths` with body line `protected-path override (main.rs) authorized under the 2026-06-14 autonomy grant`.

### Task 5: full suite + lints
- **Test:** `cargo test --manifest-path gatekeeper/Cargo.toml` → all green (≥553 + new); `cargo fmt --check`; `cargo clippy --all-targets -- -D warnings`; `shellcheck hooks/*.sh scripts/*.sh`.
- **Commit:** none (verification only) unless fmt/clippy require a fixup.

## After this plan
Slice 1 then goes through verify → review (fresh-context) → finish → PR. Slices 2 (PostToolUse advisory hook + settings.json wiring) and 3 (≥50-case `routing-eval.jsonl` + recall/precision CI thresholds) are deferred to a maintainer checkpoint and will be planned separately — R1 (eval-label subjectivity) is the reason for the checkpoint.
