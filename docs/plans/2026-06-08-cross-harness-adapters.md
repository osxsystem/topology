# Plan: Cross-harness adapters (Phase 4)

- **Date:** 2026-06-08
- **Feature slug:** cross-harness-adapters
- **Design:** docs/specs/2026-06-08-cross-harness-adapters.md
- **Research / ADR:** docs/research/2026-06-08-cross-harness-adapters.md, docs/adr/0008-cross-harness-adapter-mappings.md
- **Baseline:** branched from `origin/main` (`5aa172a`, Phase 2 merged). Confirm before starting:
  `cd gatekeeper && cargo test` → green; the instinct loader (`gatekeeper/src/instinct.rs`) and the six
  `instincts/*.md` seeds are present.

## Conventions for every task

- **No new dependencies.** This feature adds **zero** crates and does **not** touch
  `gatekeeper/Cargo.toml` / `Cargo.lock`. `serde_json` (already a dep) serializes JSON; TOML / `.mdc` /
  Markdown are emitted directly. Integration tests assert on output **text** (no `serde_json` in the
  test crate), so `[dev-dependencies]` is untouched too.
- **Tests are the per-task gate.** Each task ends with `cd gatekeeper && cargo test <filter>` green
  (exit 0). `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` are enforced once, in the
  final verify task.
- **Test style:** in-module `#[cfg(test)]` for the pure builders; `std::process::Command` +
  `env!("CARGO_BIN_EXE_gatekeeper")` over a scratch framework root for the CLI tests (mirrors
  `tests/cli_instinct.rs`). No `assert_cmd`/`predicates`.
- **Self-protection (expected, not a blocker).** Editing `gatekeeper/src/main.rs` and `scripts/install.sh`
  touches `protected_paths`. Land them as discrete, reviewable commits. `adapt.rs`, `instinct.rs`, the
  `adapters/` dir, and the harness output dirs are **not** protected.
- **Outputs are build artifacts.** Generation is pure (`root → Vec<GenFile>`); all I/O is one
  `apply_or_check`. `--check` re-renders and diffs against disk (idempotency), writing nothing.

## Files

- `gatekeeper/src/adapt.rs` — **new.** `GenFile` + `apply_or_check`; `read_skill`/`load_skills`; the
  four builders (`build_codex`/`build_cursor`/`build_opencode`/`build_claude`); `cmd_adapt` arg parsing;
  `#[cfg(test)]` unit modules.
- `gatekeeper/src/instinct.rs` — **modify.** Add `pub fn instincts_for_adapt(root) -> Result<Vec<(String,
  String)>, String>` reusing `load_instincts` (strict) + `body_oneline`. One unit test.
- `gatekeeper/src/main.rs` — **modify.** `mod adapt;` (alphabetical: `adapt`, `instinct`, `review`,
  `scan`); the `"adapt"` dispatch arm; one `print_help()` line; one `//!` line.
- `gatekeeper/tests/cli_adapt.rs` — **new.** Scratch-root CLI integration tests for all four harnesses,
  `--check` idempotency, and the error paths.
- `adapters/README.md` — **new.** The per-harness mapping reference (what each builder emits + why),
  pointing at `adapt.rs` as the generator.
- `scripts/install.sh` — **modify.** Append an opt-in note listing the four `gatekeeper adapt --harness`
  commands and what each writes (install keeps *printing* the Claude hook block; it does not auto-write).
- `docs/ROADMAP.md` — **modify.** Mark Phase 4 delivered (mermaid node, section header, status table).
- `docs/verify/2026-06-08-cross-harness-adapters.md` — **new** (final task). Evidence: suite counts,
  clippy/fmt clean, `codex --strict-config` on the generated config, JSON validity, idempotency.

## Tasks

### Task 1: Docs — research, spec, plan, ADR-0008
- **File(s):** the four docs above.
- **Change:** Author them (this plan is one of them).
- **Test:** `gatekeeper check design --feature cross-harness-adapters` → PASS;
  `gatekeeper check plan --feature cross-harness-adapters` → PASS (placeholder-free).
- **Commit:** `docs: Phase 4 cross-harness-adapters — research, spec, plan, ADR-0008`

### Task 2: `adapt.rs` core + `instinct` accessor + CLI wiring
- **File(s):** `gatekeeper/src/adapt.rs` (new), `gatekeeper/src/instinct.rs`, `gatekeeper/src/main.rs`
- **Change (a) — `instinct.rs`:** add
  ```rust
  /// Adapter accessor: (id, one-line body) for every always-on instinct, sorted (priority, then id).
  /// Strict load — a malformed `instincts/` dir is an Err the caller surfaces as exit 2.
  pub fn instincts_for_adapt(root: &Path) -> Result<Vec<(String, String)>, String> {
      let mut warnings = Vec::new();
      let list = load_instincts(&root.join("instincts"), true, &mut warnings)?;
      Ok(list.into_iter().map(|i| (i.id, i.body_oneline())).collect())
  }
  ```
- **Change (b) — `adapt.rs`:** the module per the spec. Key types/functions:
  - `struct GenFile { rel_path: PathBuf, contents: String }`.
  - `fn apply_or_check(files: &[GenFile], root: &Path, check: bool) -> i32` — write mode creates parent
    dirs and writes; `--check` reads each path and compares bytes, returns `1` on any miss/drift else `0`.
  - `struct Skill { name, description, body, raw }`; `fn read_skill(dir) -> Option<Skill>` (frontmatter
    `name`/`description`, body after the closing `---`, `raw` = whole file); `fn load_skills(root) ->
    Vec<Skill>` (sorted by `name`).
  - `fn require_agents_md(root) -> Result<String, String>`.
  - `fn build_codex/_cursor/_opencode/_claude(root) -> Result<Vec<GenFile>, String>` exactly as the spec
    specifies (Codex `project_doc_max_bytes = 1048576` + header; Cursor `agents-contract.mdc` +
    `instincts.mdc` always + `skill-<name>.mdc` agent-requested; OpenCode `opencode.json` +
    `.opencode/instincts.md` + copied skills; Claude `.claude/settings.json` hooks).
  - `fn mdc(description, always, body) -> String` and `fn yaml_inline(s) -> String` (double-quote +
    escape `\`/`"` when the value contains `:`, `"`, `#`, or a newline; else raw) for safe `.mdc`
    frontmatter.
  - `pub fn cmd_adapt(args, root) -> i32` — parse `--harness <h>` (required) and `--check`; dispatch;
    map builder `Err` to `eprintln!` + exit `2`.
- **Change (c) — `main.rs`:** `mod adapt;`; `Some("adapt") => adapt::cmd_adapt(&args[1..], &framework_root()),`;
  add to `print_help()`:
  `gatekeeper adapt --harness <codex|cursor|opencode|claude> [--check]\n`; add to the `//!` block:
  `//!   gatekeeper adapt --harness <h> [--check]   Generate <h>'s native config from the source.`
- **Unit tests (`#[cfg(test)] mod` in `adapt.rs`):** a `fixture()` writing a scratch root (`AGENTS.md`,
  two `skills/`, two `instincts/`). Assert: `build_codex` emits `project_doc_max_bytes` and **none** of
  `profile`/`model_provider`/`notify`; `build_cursor` emits `alwaysApply: true` for `instincts.mdc` and
  `alwaysApply: false` + a `description` + **no** `globs:` for a skill rule; `build_opencode`'s
  `opencode.json` contains the schema URL and `"AGENTS.md"`, and a skill file equals the source `raw`;
  `build_claude`'s settings contains `security-scan.sh` and `UserPromptSubmit`; `apply_or_check` writes
  then re-`--check`s clean, and reports drift after a mutation; `require_agents_md` errs when absent.
- **Test:** `cd gatekeeper && cargo test adapt` → green; `cargo test` → all prior suites still green.
- **Commit:** `feat(gatekeeper): cross-harness adapt — codex/cursor/opencode/claude generators`

### Task 3: `cli_adapt.rs` integration tests
- **File(s):** `gatekeeper/tests/cli_adapt.rs` (new)
- **Change:** `scratch_root(tag)` (skills/ + instincts/ + AGENTS.md + hooks/skill-rules.json) and a
  `run(cwd, args)` helper. Cases: each harness writes its files (assert paths + key substrings);
  `--harness frobnicate` → exit 2; bare `adapt` (no `--harness`) → exit 2; a root without `AGENTS.md`
  → exit 2; write-then-`--check` → exit 0, and `--check` after truncating one output → exit 1.
- **Test:** `cd gatekeeper && cargo test --test cli_adapt` → green; `cargo test` → full suite green.
- **Commit:** `test(gatekeeper): adapt CLI integration tests (cli_adapt)`

### Task 4: `adapters/README.md`
- **File(s):** `adapters/README.md` (new)
- **Change:** Overview (adapters are generators; outputs are build artifacts per ADR-0003/0008) + a
  per-harness section (files written, rule/setting mapping, the source it derives from) + the command
  and `--check`. Points to `gatekeeper/src/adapt.rs`.
- **Test:** `gatekeeper list` still works; links resolve by inspection.
- **Commit:** `docs(adapters): per-harness mapping reference`

### Task 5: `scripts/install.sh` — opt-in adapt note
- **File(s):** `scripts/install.sh`
- **Change:** After the hook-config block, append an `echo` section listing the four
  `gatekeeper adapt --harness …` commands and what each writes. No behavior change to the existing
  build/symlink/hook steps; the Claude hook JSON is still printed (not auto-written).
- **Test:** `bash -n scripts/install.sh` → exit 0; `shellcheck scripts/install.sh` → clean.
- **Commit:** `chore(install): document per-harness adapt generation`

### Task 6: ROADMAP — Phase 4 delivered
- **File(s):** `docs/ROADMAP.md`
- **Change:** Mermaid `P4` node → `✅`; the Phase 4 section header → delivered with the verify-evidence
  pointer; the status-table row → `✅ delivered`.
- **Test:** `grep -n "Phase 4" docs/ROADMAP.md` shows the delivered markers; `cargo test` unaffected.
- **Commit:** `docs(roadmap): Phase 4 cross-harness adapters delivered`

### Task 7: Verify — quality gates + the evidence note
- **File(s):** `docs/verify/2026-06-08-cross-harness-adapters.md` (new)
- **Change (a):** `cargo fmt --check` → 0; `cargo clippy --all-targets -- -D warnings` → 0; `cargo test`
  → green (record counts).
- **Change (b):** Generate into a scratch repo and prove each acceptance criterion with a re-runnable
  command: `codex exec --strict-config --skip-git-repo-check` loads the generated `.codex/config.toml`;
  `python3 -c 'json.load(...)'` parses `opencode.json` and `.claude/settings.json`; a copied
  `.opencode/skills/<name>/SKILL.md` is byte-equal to source (`cmp`); `adapt --check` exits 0 right after
  a write and 1 after a mutation.
- **Change (c):** Write the note recording the commands, their output, and each criterion's verdict.
- **Test:** the note's commands are the test; all exit 0 / green as recorded.
- **Commit:** `test(adapters): verify note — suite green, codex/json validated, idempotent`

<!-- No placeholder tokens: the plan gate rejects them. -->
