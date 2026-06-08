# Plan: Memory + research-first hardening (Phase 5)

- **Date:** 2026-06-08
- **Feature slug:** memory-research-first
- **Design:** docs/specs/2026-06-08-memory-research-first.md
- **Research / ADR:** docs/research/2026-06-08-memory-research-first.md, docs/adr/0009-memory-research-first-hardening.md
- **Baseline:** branched from the Phase-4-merged `main` (`8ea06d8`). Confirm before starting:
  `cd gatekeeper && cargo test` → green; the gate helpers `gate_doc_exists`/`find_doc`/`feature_arg`
  (`gatekeeper/src/main.rs:233-285`), `is_protected`/`scan_check_path` (`gatekeeper/src/scan.rs:461-471`),
  and the `learn`/`adapt`/`scan`/`review` modules are present.

## Conventions for every task

- **No new dependencies.** Deps stay `regex` / `serde` / `serde_json` / `toml`; this feature does **not**
  touch `gatekeeper/Cargo.toml` / `Cargo.lock`. Frontmatter is parsed by hand (the repo already does this
  in `instinct.rs`); no YAML/date crate is added.
- **No wall clock.** `gatekeeper` reads no clock by design (`learn.rs` takes dates via `--date`). `memory
  write` follows suit: `created` comes from a required `--date <YYYY-MM-DD>` flag, so writes are
  deterministic and testable.
- **Git facts via subprocess.** `branch`/`head_sha` are read with `Command::new("git")` (same call style
  as `review.rs:189` / `scan.rs:480`). **Deliberate policy difference:** unlike `review.rs`, which errors
  when git facts are unavailable, `memory write` degrades to empty strings off a repo so a handoff can be
  written anywhere — a chosen behaviour, not a mirror.
- **Tests are the per-task gate.** Each task ends with `cd gatekeeper && cargo test <filter>` green
  (exit 0). `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check` are enforced once, in the
  final verify task.
- **Test style:** in-module `#[cfg(test)]` for pure functions; `std::process::Command` +
  `env!("CARGO_BIN_EXE_gatekeeper")` over a scratch framework root for the CLI tests (mirrors
  `tests/cli_adapt.rs`). No `assert_cmd` / `predicates`.
- **Hygiene is reused, not re-listed.** `memory write` calls one new `scan::scan_bytes_for_secrets`; it
  never carries its own copy of the secret rules. The scanner **refuses** (and redacts the hint) — it does
  not silently strip — so docs say "secret refusal," not "strip."
- **Self-protection (expected, not a blocker).** Editing `main.rs`, `scan.rs`, `security/rules.toml`,
  `AGENTS.md`, `scripts/install.sh`, and `.gitignore` may touch protected paths — land them as discrete,
  reviewable commits. `memory.rs`, `skills/`, `docs/`, and the `memory/` seeds are not protected.

## Files (key signatures)

```rust
// main.rs — cmd_check: research arm + design sequence-lock
"research" => gate_doc_exists("research", &feature_arg(args)),
"design" => {                                  // research note required BEFORE the spec check
    let f = feature_arg(args);
    match find_doc("research", &f) {
        None => { println!("FAIL design gate: research-first — no docs/research/*{f}*.md"); 1 }
        Some(_) => gate_doc_exists("specs", &f),
    }
}

// scan.rs — byte helper (the --content path is byte-based; NUL/non-UTF8 are tested), no change to cmd_scan
/// Ok(()) if clean; Err(redacted hint) on the first block-severity match.
pub fn scan_bytes_for_secrets(rules: &Rules, bytes: &[u8]) -> Result<(), String> { /* … */ }

// scan.rs — is_protected gains directory-prefix matching via Path::starts_with (component-wise:
// `memory/artifacts` matches `memory/artifacts/x.md` but NOT `memory/artifacts-evil/`)
fn is_protected(root: &Path, protected: &[String], path: &str) -> bool {
    let target = resolve_against_root(root, path);
    protected.iter().any(|p| {
        let pr = resolve_against_root(root, p);
        target == pr || target.starts_with(&pr)
    })
}

// memory.rs — new module
pub fn cmd_memory(args: &[String], root: &Path) -> i32;   // write|read|list ; --feature --date --status --verified-by
fn render(slug, date, branch, sha, status, verified_by, body) -> String;  // header + frontmatter + body
fn git_info(root: &Path) -> (String, String);             // (branch, head_sha) via `git`, empty off-repo
// --feature validated by instinct::validate_id; one kind (handoff); scan runs on render(...) output
```

## Tasks

### Task 1: `research` gate + `design` sequence-lock
- **File(s):** `gatekeeper/src/main.rs`; `gatekeeper/tests/cli_check.rs` (new)
- **Change (a):** in `cmd_check` add the `"research" => gate_doc_exists("research", &feature_arg(args))`
  arm **and** change the `"design"` arm to require a research note first (the `find_doc("research", …)`
  guard in the Files block) — an independent arm would not actually block design, which is the real gap
  the review caught. Add the `check research` line to `print_help()` and a `//!` line.
- **Change (b):** `cli_check.rs` — a `scratch_root(tag)` helper + `run(cwd, args)` (mirrors
  `cli_adapt.rs`). Cases: with no `docs/research/*S*.md`, `check research --feature S` exits `1` **and**
  `check design --feature S` exits `1` *even after a spec is written* (proves the lock); after writing
  `docs/research/2026-06-08-S.md`, `check research` exits `0` and `check design` falls through to its spec
  check; missing `--feature` exits `2`.
- **Test:** `cd gatekeeper && cargo test --test cli_check` → green; `cargo test` → full suite green
  (the existing design/verify behaviour for already-researched features is unchanged).
- **Commit:** `feat(gatekeeper): research gate + design sequence-lock (research-first)`

### Task 2: `memory.rs` protocol + `scan_bytes_for_secrets` + `main.rs` wiring
- **File(s):** `gatekeeper/src/scan.rs`, `gatekeeper/src/memory.rs` (new), `gatekeeper/src/main.rs`
- **Change (a) — `scan.rs`:** factor the byte-matching loop out of `scan_content_cmd`/`scan_with` into
  `pub fn scan_bytes_for_secrets(rules, bytes: &[u8]) -> Result<(), String>` (Err carries the redacted
  rule hint, never the matched value). It stays on the **byte** path (the `--content` tests cover NUL /
  non-UTF8, `scan.rs:1151-1157`); `scan_content_cmd` calls it, so its observable behaviour — reporting all
  warn/block findings on stdin bytes — is unchanged.
- **Change (b) — `memory.rs`:**
  - `cmd_memory` parses the subcommand + `--feature` (required, write/read) + `--date` (required for
    write) + `--status` (default `in-progress`) + `--verified-by`. The machine-state fields come from these
    flags, never from the body. **Validation:** `--feature` through `instinct::validate_id` (rejects `../`,
    spaces, etc.); a malformed `--date` or an invalid `--feature` ⇒ `2`, writing nothing. Unknown
    subcommand ⇒ `2`.
  - `render(...)` emits a generated-by header, the YAML frontmatter (`feature`/`created`/`branch`/
    `head_sha`/`status`/`verified_by` — the spec contract), then the body.
  - `git_info(root)` runs `git rev-parse --abbrev-ref HEAD` / `rev-parse HEAD`, empties off-repo.
  - `write`: read body from stdin; reject a body that opens its own second `---` frontmatter block;
    build the artifact with `render(...)`; **scan the rendered bytes** with `scan_bytes_for_secrets` (so a
    secret-shaped `branch`/feature is caught, not just the body) — Err ⇒ redacted hint + `1`, write
    nothing; if `status == "done"`, require an existing `docs/verify/*<feature>*.md`
    (reuse `find_doc("verify", feature)`) else `1`; create `memory/artifacts/`; write
    `memory/artifacts/<slug>.handoff.md`; print the path.
  - `read`: print `memory/artifacts/<slug>.handoff.md` to stdout, `0`; absent ⇒ message + `1`.
  - `list`: plain directory read of `memory/artifacts/`; print `slug · created · status`; read-only.
- **Change (c) — `main.rs`:** `mod memory;`; `Some("memory") => memory::cmd_memory(&args[1..], &framework_root()),`;
  the three `memory` lines in `print_help()` and the `//!` header.
- **Unit tests (`#[cfg(test)]`):** `scan_bytes_for_secrets` Ok on clean bytes, Err on a known pattern, and
  unchanged on NUL bytes; `render` round-trips; an invalid `--feature`/`--date` and a double-frontmatter
  body each return non-zero with no file; a secret reachable only via a stamped field is refused;
  `status: done` without an existing verify note is refused, with one it succeeds; clean `write`→`read` is
  byte-equal; `list` reports two artifacts.
- **Test:** `cd gatekeeper && cargo test memory` and `cargo test scan` and `cargo test` → green.
- **Commit:** `feat(gatekeeper): memory protocol — write/read/list handoff artifacts`

### Task 3: `cli_memory.rs` integration tests (move CLI cases out of the module)
- **File(s):** `gatekeeper/tests/cli_memory.rs` (new); `gatekeeper/src/memory.rs` (trim)
- **Note:** Task 2 put CLI-level cases *in-module* using a `std::env::current_exe()` walk-up because
  `env!("CARGO_BIN_EXE_gatekeeper")` isn't defined for binary unit tests. Task 3 does it the repo's way:
  move those CLI cases into `tests/cli_memory.rs` (a real integration test, where the macro *is* defined),
  and **delete the in-module CLI tests**, leaving only the pure-function unit tests (`render` round-trip,
  date validation, frontmatter parse, `scan_bytes_for_secrets`) in `memory.rs`. No fragile `current_exe()`.
- **Change:** `scratch_root(tag)` (with a `security/rules.toml`) + `run_stdin(cwd, args, body)`, mirroring
  `tests/cli_adapt.rs` and using `env!("CARGO_BIN_EXE_gatekeeper")`. Cases:
  `memory write --feature S --date 2026-06-08` (body on stdin) writes `memory/artifacts/S.handoff.md` with
  stamped fields; `memory read --feature S` is byte-equal and exits `0`; `read` on an unknown slug exits
  `1`; a non-allowlisted secret in the body **and** one reachable via a stamped field each exit `1` with the
  target absent; `--status done` exits `1` with no `docs/verify/*S*.md` and `0` once it exists; an invalid
  `--feature` (`../escape`), a malformed `--date`, and a double-`---` body each exit non-zero with nothing
  written; `memory list` shows the entries; an unknown subcommand exits `2`.
- **Test:** `cd gatekeeper && cargo test --test cli_memory` → green; `cargo test` → full suite green.
- **Commit:** `test(gatekeeper): memory CLI integration tests (cli_memory)`

### Task 4: directory-prefix protection + Bash tamper rule for `memory/artifacts/`
- **File(s):** `gatekeeper/src/scan.rs`, `security/rules.toml`
- **Change (a):** extend `is_protected` to also match when the resolved target is **beneath** a protected
  entry, via `Path::starts_with` on resolved paths (component-wise — so `memory/artifacts` does **not**
  match `memory/artifacts-evil`); exact-match entries keep working. Add `memory/artifacts` to
  `[integrity] protected_paths` in `security/rules.toml`.
- **Change (b):** add a heuristic **tamper-command rule** to `security/rules.toml` that flags a Bash
  command redirecting into `memory/artifacts/` (e.g. `> memory/artifacts` / `>> .../memory/artifacts/`). This
  raises — does not close — the Bash residual (the hook does not parse shell); recorded as such, not sold
  as airtight.
- **Test:** unit tests in `scan.rs` — protected: `memory/artifacts/x.md`, an absolute in-repo path to it, a
  `..` alias resolving into it, and a trailing-slash form; **not** protected: `memory/TEMPLATE.handoff.md`
  and `memory/artifacts-evil/x.md` (the collision case); an existing exact-match protected path still is.
  CLI: `scan --check-path memory/artifacts/a.handoff.md` → `1`; `--check-path memory/artifacts-evil/a.md` and
  `--check-path memory/TEMPLATE.handoff.md` → `0`. (Symlink/case bypasses are **not** claimed — `is_protected`
  is lexical; noted as a residual in the verify note, not tested as a guarantee.) `cargo test scan` and
  `cargo test` → green.
- **Commit:** `feat(gatekeeper): protect memory/artifacts/ (dir-prefix) + bash tamper rule`

### Task 5: `memory/` source-of-truth directory + `.gitignore`
- **File(s):** `memory/README.md` (new), `memory/TEMPLATE.handoff.md` (new), `.gitignore`
- **Change:** `README.md` documents the protocol — the handoff artifact, the frontmatter contract,
  `artifacts/` is gitignored + write-protected, updates go only through `gatekeeper memory write`,
  and a pointer to the `resume` skill. `TEMPLATE.handoff.md` is the format example: valid frontmatter +
  the Goal / State / Next steps / Key files / Decisions sections from the spec. `.gitignore` gains
  `/memory/artifacts/`.
- **Test:** `gatekeeper memory list` in a clean tree exits `0` with no entries; `TEMPLATE.handoff.md`
  parses (round-trips through the `memory.rs` frontmatter parser in a test); `git status --porcelain
  memory/artifacts/` shows nothing tracked after a write.
- **Commit:** `feat(memory): protocol README + handoff template; ignore artifacts/`

### Task 6: `skills/research-first/SKILL.md`
- **File(s):** `skills/research-first/SKILL.md` (new)
- **Change:** house-format frontmatter (`name: research-first`, `description: "… Use when …"`). Body: the
  method (decompose → gather → cite → verify) and the rule to **delegate heavy exploration to a subagent**
  whose returned summary becomes a `docs/research/<date>-<slug>.md` note — which is exactly what
  `check research` gates on. Reach (per the review): Claude reads `skills/` natively; `adapt` copies it to
  Cursor + OpenCode (`adapt.rs:221-256`); Codex reaches it only through `AGENTS.md` — **not** "every
  harness via adapt."
- **Test:** the skill loads (frontmatter parses); `cargo test` unaffected; after the skill's note is
  written, `gatekeeper check research --feature <slug>` exits `0` (the Task-1 path).
- **Commit:** `feat(skills): research-first — explore-before-design, subagent-delegated`

### Task 7: `skills/resume/SKILL.md`
- **File(s):** `skills/resume/SKILL.md` (new)
- **Change:** frontmatter + body encoding the resume routine: `gatekeeper memory read --feature <slug>`
  → read `git log` → run a smoke/build check → only then act; one slice per session; never self-assert
  `done` (set `verified_by` first).
- **Test:** the skill loads; links resolve by inspection; `cargo test` unaffected.
- **Commit:** `feat(skills): resume — read handoff, verify state, then act`

### Task 8: sequence docs (all three) + Compact Instructions
- **File(s):** `AGENTS.md`, `METHODOLOGY.md`, `docs/HOW-IT-WORKS.md`
- **Change (a) — propagate research into the enforced sequence in *all three* places it is documented**
  (they currently disagree with the post-Phase-5 binary): `AGENTS.md` §"The gate sequence" prepends
  research; `METHODOLOGY.md` drops the `[planned]` marker on the research gate (:122), updates the
  "design and plan pass (and, once shipped, research)" hedge (:130) to reflect it now binds, and bumps the
  process-skill count (8 → 10: `research-first` + `resume`); `HOW-IT-WORKS.md` adds research to its gate
  table/diagram. The `design`→`research` lock is noted as binding features from Phase 5 onward (ADR-0009).
- **Change (b):** add a `## Compact Instructions` section to `AGENTS.md` telling the harness to preserve
  handoff-relevant state (current slice, next step, open decisions) during auto-compaction.
- **Test:** `grep -n "research" AGENTS.md METHODOLOGY.md docs/HOW-IT-WORKS.md` shows it enforced (no
  `[planned]`) in all three; `grep -n "Compact Instructions" AGENTS.md` finds the header; `grep -n "10 process"
  METHODOLOGY.md` confirms the count. Adapters are build artifacts (not committed) — no regeneration required.
- **Commit:** `docs: propagate research gate across sequence docs; add compact instructions`

### Task 9: RTK proxy docs
- **File(s):** `docs/learn/rtk-proxy.md` (new); a one-line pointer from `README.md`
- **Change:** document RTK as the default shell proxy — what it does, the `UserPromptSubmit`/command-rewrite
  hook wiring, and an opt-in install note. Documentation only; no `gatekeeper` surface.
- **Test:** `lychee docs/learn/rtk-proxy.md` (repo `lychee.toml`) and `typos` are clean; the README pointer
  resolves.
- **Commit:** `docs(rtk): document RTK as the default shell proxy`

### Task 10: ROADMAP — Phase 5 delivered
- **File(s):** `docs/ROADMAP.md`
- **Change:** Mermaid `P5` node → `✅`; the Phase 5 section header → delivered with the verify-evidence
  pointer; the status-table row → `✅ delivered`.
- **Test:** `grep -n "Phase 5" docs/ROADMAP.md` shows the delivered markers; `cargo test` unaffected.
- **Commit:** `docs(roadmap): Phase 5 memory + research-first delivered`

### Task 11: Verify — quality gates + the evidence note
- **File(s):** `docs/verify/2026-06-08-memory-research-first.md` (new)
- **Change (a):** `cargo fmt --check` → 0; `cargo clippy --all-targets -- -D warnings` → 0; `cargo test`
  → green (record counts).
- **Change (b):** prove each spec acceptance criterion (1–11) with a re-runnable command: `check research`
  `1`→`0` and `check design` blocked `1`→falls-through across writing a note (the sequence-lock);
  `memory write`→`read` byte-equal (`cmp`); a secret in the body **and** one via a stamped field each exit
  `1` with the file absent (rendered-artifact scan); `status: done` refused until `docs/verify/*S*.md`
  exists; invalid `--feature`/`--date` refused; `memory list` accurate;
  `scan --check-path memory/artifacts/x.handoff.md` → `1`, `memory/artifacts-evil/x.md` and
  `memory/TEMPLATE.handoff.md` → `0` (collision + seed); `git diff --stat` shows `Cargo.toml`/`Cargo.lock`
  unchanged.
- **Change (c):** write the note recording the commands, their output, and each criterion's verdict —
  **including** the explicitly-unclosed residuals (criterion 11): a Bash redirection into `memory/artifacts/`
  is not blocked by the tool hook (only flagged by the tamper rule), and `is_protected` does not follow
  symlinks or fold case. Record these as accepted limitations, not silent gaps.
- **Test:** the note's commands are the test; all exit `0` / green as recorded.
- **Commit:** `test(memory): verify note — gates green, round-trip + protection validated`

<!-- No placeholder tokens: the plan gate rejects them. -->
