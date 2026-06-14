# Plan: C6 — doctor check for the Co-Authored-By × approval_provenance trailer collision

- **Date:** 2026-06-14
- **Feature slug:** c6-trailer-collision
- **Design:** `docs/specs/2026-06-14-c6-trailer-collision.md`
- **Branch:** `fix/c6-trailer-collision` (off `main` at `d3cb721`)

All changes land in **unprotected** files: `gatekeeper/src/doctor.rs` (probe + pure fn + tests). No edits
to `main.rs`/`scan.rs`/`rules.toml`/hooks/`Cargo.*`. Edits use the Edit/Write tool (the path-mutation
rule blocks only *Bash* mutations into `gatekeeper/src/`, not the tool or `cargo`/`git add`).

## Task 1 — TDD red: unit tests for the pure decision fn

In `gatekeeper/src/doctor.rs` `#[cfg(test)] mod tests`, add tests driving a not-yet-existing pure fn
`approval_trailer_collision(trailers: &str, patterns: &[String]) -> Option<(String, String)>`:

1. `claude_co_author_matches_default_pattern` — trailer block with
   `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>` + `default_agent_trailer_patterns()`
   → `Some` whose `.1` is `(?i)claude` and `.0` contains `Claude`.
2. `clean_human_trailers_no_match` — `Signed-off-by: Jane <j@x>` + `Reviewed-by: Bob <b@x>` → `None`.
3. `only_co_authored_by_key_examined` — `Reviewed-by: claude-bot <c@x>` (value contains "claude" but
   key is not `Co-Authored-By`) → `None`.
4. `case_insensitive_key_and_non_default_pattern` — lowercase `co-authored-by: GitHub Copilot <…>` with
   patterns `["(?i)copilot"]` → `Some`.
5. `invalid_pattern_skipped_valid_still_matches` — patterns `["(unclosed", "(?i)claude"]` with a Claude
   co-author line → `Some` (no panic); the bad pattern is skipped.
6. `empty_input_no_match` — `""` → `None`.

Run `cargo test --manifest-path gatekeeper/Cargo.toml --bin gatekeeper approval_trailer` and **watch it
fail to compile / fail** (fn does not exist). Record the red.

## Task 2 — TDD green: implement the pure fn

Add `fn approval_trailer_collision(trailers: &str, patterns: &[String]) -> Option<(String, String)>` to
`doctor.rs` (module scope, near the other pure helpers). For each line of `trailers`: lowercase it; skip
unless it `starts_with("co-authored-by:")`; take `line["co-authored-by:".len()..].trim()` of the
ORIGINAL line as the value; for each `pattern`, `match regex::Regex::new(pattern) { Ok(re) if
re.is_match(value) => return Some((value.to_string(), pattern.clone())), _ => continue }`. Return `None`
after all lines. Mirrors `main.rs:1731-1759` mechanics exactly. Re-run the Task 1 tests → **green**.

## Task 3 — implement the probe wrapper

Add `fn probe_approval_trailer_collision(project_root: &Path, artifacts_root: &Path)` (returns `()` —
advisory) to `doctor.rs`:

- `let cfg = config::ProjectConfig::load(artifacts_root);`
- If `cfg.design_approval != config::DesignApproval::HumanCommit`: print
  `approval trailer collision: n/a (approval=status-line; provenance check is shadow)`; return.
- Shell out once:
  `Command::new("git").arg("-C").arg(project_root).args(["log","-n","20","--no-merges","--format=%(trailers)"])`.
  On non-success status or spawn error or empty stdout → print
  `approval trailer collision: n/a (git history unavailable)`; return.
- `match approval_trailer_collision(&stdout, &cfg.design_agent_trailer_patterns)`:
  - `Some((value, pattern))` → print the WARN line (see §WARN text).
  - `None` → print `approval trailer collision: ok (no agent trailer on recent authored commits)`.

Add `use crate::config;` to the import block (`doctor.rs:8-17`). Reuse the existing inline-`git` style
(`doctor.rs:516`); add `use std::process::Command;` only if not already present.

### WARN text (single line, plain `println!`, bare ASCII tag, no emoji)

```
approval trailer collision: WARN: recent commits carry an agent Co-Authored-By trailer ("<value>") matching agent_trailer_patterns pattern "<pattern>"; under [design] approval="human-commit" a human approval commit carrying this trailer will FAIL the design gate (read as agent self-approval). Drop the always-add-Co-Authored-By rule from your harness/commit template for approval commits, or relax [design] agent_trailer_patterns.
```

## Task 4 — register the probe in cmd_doctor

In `cmd_doctor` (`doctor.rs:79`), after the existing advisory probe calls (after `doctor.rs:383`), add:
`probe_approval_trailer_collision(&crate::project_root(), &crate::artifacts_root());`. It must NOT touch
`failures` (advisory only).

## Task 5 — finish-gate prep: fmt, clippy, full suite

From repo root, with `TOPOLOGY_ROOT` UNSET (per the dogfood gotcha):
- `cargo fmt --manifest-path gatekeeper/Cargo.toml` then `cargo fmt --manifest-path gatekeeper/Cargo.toml -- --check`
- `cargo clippy --manifest-path gatekeeper/Cargo.toml --all-targets -- -D warnings`
- `cargo test --manifest-path gatekeeper/Cargo.toml` (full suite green; note the new test count delta)

## Task 6 — verify gate (reproduce-then-resolve)

Record evidence at `docs/verify/2026-06-14-c6-trailer-collision.md`:
- **Reproduce the collision shadow:** show that all recent commits carry the Claude trailer
  (`git log -n 5 --format=%(trailers:key=Co-Authored-By)`), i.e. the input the gate keys on.
- **Resolve / demonstrate the check:** build the binary; run `gatekeeper doctor` in a temp repo (or via
  a throwaway `config.toml` with `[design] approval = "human-commit"`) seeded with a Claude-trailered
  commit → assert the **WARN** fires. Run with default config → assert the **n/a (status-line)** line.
  Run with `human-commit` + a clean human-only commit → assert **ok**.

## Task 7 — code-review gate

Fresh-context critic review of the clean `HEAD` (bound to merge-base `d3cb721`), both rubric dimensions
(correctness + simplicity). Record at `docs/reviews/2026-06-14-c6-trailer-collision.md`. Address blocking
findings via further TDD before finish.

## Task 8 — finish + commit

`gatekeeper check finish -- cargo test --manifest-path gatekeeper/Cargo.toml`. Commit `doctor.rs` (no
`--no-verify` needed — unprotected). Do not push without maintainer confirmation.

## Rollback

Single-file, additive change. Revert by deleting the probe, its registration line, the pure fn, the
`use crate::config;` import, and the tests. No data migration, no protected-file state.
