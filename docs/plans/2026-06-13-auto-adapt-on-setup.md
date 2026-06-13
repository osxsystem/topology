# Plan: idempotent setup-time `gatekeeper adapt` (auto-wire fresh clones/worktrees)

- **Date:** 2026-06-13
- **Feature slug:** auto-adapt-on-setup
- **Design:** docs/specs/2026-06-13-auto-adapt-on-setup.md
- **Baseline:** tests green at commit `0677053` (gatekeeper: 551 passed, 0 failed, 5 ignored)

## Environment note (commits)

Same stray-`.topology/` pre-commit misfire as #52 (now filed as #60). Commit every step with
`TOPOLOGY_ROOT="$PWD" git commit -m "…"` (keeps the scan active, points it at the real root). Run
`cargo test` with `TOPOLOGY_ROOT` **unset** (exporting it pollutes the design-hardening scratch tests).

## Files

- `gatekeeper/tests/cli_adapt.rs` — add one characterization test pinning the self-governed claude
  apply-rerun-no-op that the trigger relies on (test-after of existing behavior, per the design).
- `justfile` — enhance the `setup` recipe: after the pre-commit install, build the release binary then
  run `gatekeeper adapt --harness claude` (Bash glue, no logic).
- `docs/DEVELOPMENT.md` — document the bootstrap; link ADR-0019; state why the build is load-bearing.
- `CHANGELOG.md` — Unreleased entry.

## Tasks

### Task 1: Characterization test — claude apply re-run is a no-op (self-governed)

- **File(s):** `gatekeeper/tests/cli_adapt.rs`
- **Change:** add this test (after `dogfood_settings_are_portable`, ~line 672). It uses the
  single-root self-governed `scratch_root` + `run` harness (like `dogfood_settings_are_portable:648`),
  **not** `run_proj` (governed) — so it does not duplicate `ac4_settings_no_clobber:510`. The no-op is
  asserted explicitly via the `wrote` line (apply, `adapt.rs:954`) and `--check` exit/`DRIFT`
  (`adapt.rs:925-929`).
  ```rust
  /// Characterization (test-after of existing behavior, per the #58 design): in the self-governed
  /// (single-root) case a *claude* `adapt` apply writes settings.json once, then is a true no-op on
  /// re-run — the property the `just setup` auto-wire trigger depends on. Self-governed harness
  /// (scratch_root + run), so it does not overlap the governed ac4_settings_no_clobber.
  #[test]
  fn dogfood_settings_claude_apply_rerun_is_noop() {
      let root = scratch_root("rerun_noop");

      // First apply writes the file.
      let (code1, out1) = run(&root, &["adapt", "--harness", "claude"]);
      assert_eq!(code1, 0, "first apply must succeed; out:\n{out1}");
      assert!(
          out1.contains("wrote .claude/settings.json"),
          "first apply must write settings.json; out:\n{out1}"
      );

      // Second apply is a true no-op: settings already correct → nothing written.
      let (code2, out2) = run(&root, &["adapt", "--harness", "claude"]);
      assert_eq!(code2, 0, "second apply must succeed; out:\n{out2}");
      assert!(
          !out2.contains("wrote .claude/settings.json"),
          "second apply must NOT rewrite settings.json (write-on-drift no-op); out:\n{out2}"
      );

      // --check confirms no drift after the write.
      let (code3, out3) = run(&root, &["adapt", "--harness", "claude", "--check"]);
      assert_eq!(code3, 0, "--check after write must report no drift (exit 0); out:\n{out3}");
      assert!(
          !out3.contains("DRIFT .claude/settings.json"),
          "--check must not report settings.json drift after a clean write; out:\n{out3}"
      );

      let _ = fs::remove_dir_all(&root);
  }
  ```
- **TDD honesty:** this is **characterization**, not red→green — `adapt` already implements the no-op
  (`adapt.rs:930`), so the test passes immediately. It pins behavior the trigger depends on against
  regression; stated openly rather than staged as fake red.
- **Test:** `cargo test --manifest-path gatekeeper/Cargo.toml --test cli_adapt dogfood_settings_claude_apply_rerun_is_noop`
  → expect `test result: ok. 1 passed`.
- **Commit:** `TOPOLOGY_ROOT="$PWD" git commit -m "test(adapt): pin self-governed claude apply re-run no-op (#58)"`

### Task 2: Enhance the `just setup` recipe (build, then adapt)

- **File(s):** `justfile`
- **Change:**
  1. Replace the header comment block above `setup:` (currently justfile:10-12) with:
     ```
     # Bootstrap a fresh framework clone or worktree:
     #   1. install hooks/pre-commit.sh as the git pre-commit hook (copy, not symlink; survives
     #      in-place edits); stops with an error if a non-topology pre-commit hook already exists.
     #   2. build the release binary, then `gatekeeper adapt --harness claude` to (re)generate the
     #      portable .claude/settings.json. The RELEASE build is load-bearing: portable settings drop
     #      GATEKEEPER_BIN, so the hooks resolve gatekeeper/target/release/gatekeeper (see
     #      docs/DEVELOPMENT.md + ADR-0019). adapt writes settings.json only on drift → re-run no-op.
     ```
  2. Append these three lines to the end of the `setup:` recipe body (after the pre-commit `fi`,
     currently justfile:28). Each is its own recipe line → its own shell → aborts on first failure, so
     a build failure is fatal and leaves the hook installed but skips `adapt` (M1):
     ```
         @echo "setup: building gatekeeper (release) and wiring .claude/settings.json…"
         cargo build --release --manifest-path gatekeeper/Cargo.toml
         ./gatekeeper/target/release/gatekeeper adapt --harness claude
     ```
- **Test:** `just --dry-run setup 2>&1` → expect the expansion to contain
  `cargo build --release --manifest-path gatekeeper/Cargo.toml` and
  `gatekeeper/target/release/gatekeeper adapt --harness claude` (confirms the recipe is well-formed and
  wires both steps; the real end-to-end run is the verify gate). The full functional proof — running
  `just setup` in a throwaway worktree and observing a portable `.claude/settings.json` — is recorded
  at the **verify gate**, not here (the recipe is glue with no unit to test).
- **Commit:** `TOPOLOGY_ROOT="$PWD" git commit -m "feat(setup): just setup builds + runs adapt to wire settings.json (#58)"`

### Task 3: Document the bootstrap (DEVELOPMENT.md + CHANGELOG)

- **File(s):** `docs/DEVELOPMENT.md`, `CHANGELOG.md`
- **Change:**
  1. In `docs/DEVELOPMENT.md`, insert this section between the intro (ends line 5) and
     `## Stack conventions for this repo` (line 7):
     ```markdown
     ## Bootstrapping a fresh clone or worktree

     Run `just setup` once in any fresh clone or worktree. It (1) installs the topology git
     pre-commit hook, then (2) builds the release binary and runs `gatekeeper adapt --harness claude`
     to regenerate the **portable** `.claude/settings.json`.

     `.claude/settings.json` is **generated, never committed** (see
     [ADR-0019](adr/0019-generated-only-settings-json.md)) — `just setup` is how a fresh tree gets it,
     complementing the `gatekeeper doctor` stale-path warning (#52). Re-running `just setup` rewrites
     settings.json only on drift, so it is safe to run repeatedly.

     The release build in `just setup` is **load-bearing, not incidental**: portable settings
     deliberately omit `GATEKEEPER_BIN`, so the hooks resolve the binary via `security-scan.sh`'s
     fallback to `gatekeeper/target/release/gatekeeper`. Switching the bootstrap to a debug build, or
     skipping it when `gatekeeper` is merely on `PATH`, would silently leave a dev clone's security
     floor unwired.
     ```
  2. In `CHANGELOG.md`, insert a `### Changed` block immediately after the `## Unreleased` line
     (line 5) and before the existing `### Fixed`:
     ```markdown
     ### Changed

     - `just setup` now builds the release binary and runs `gatekeeper adapt --harness claude` after
       installing the pre-commit hook, so a fresh clone or worktree self-wires its portable
       `.claude/settings.json` with no manual step (#58). Complements the `gatekeeper doctor`
       stale-path warning (#52); the generated-only decision is recorded in ADR-0019. Re-running is a
       no-op on settings.json (write-on-drift). The release build is load-bearing — portable settings
       omit `GATEKEEPER_BIN`, so the hooks resolve `gatekeeper/target/release/gatekeeper`.
     ```
- **Test:** `just --justfile justfile docs` is unrelated; instead verify links/format:
  `grep -q "Bootstrapping a fresh clone or worktree" docs/DEVELOPMENT.md && grep -q "0019-generated-only-settings-json.md" docs/DEVELOPMENT.md && grep -q "just setup. now builds" CHANGELOG.md`
  → all three present (exit 0). (CI's `just docs` link-check is the authoritative gate; this is a
  presence check.)
- **Commit:** `TOPOLOGY_ROOT="$PWD" git commit -m "docs(setup): document just setup bootstrap + load-bearing build (#58)"`

### Task 4: Full-suite regression + fmt/clippy gate

- **File(s):** none (verification only).
- **Test:**
  ```bash
  cargo fmt --manifest-path gatekeeper/Cargo.toml --check
  cargo clippy --manifest-path gatekeeper/Cargo.toml --all-targets -- -D warnings
  cargo test --manifest-path gatekeeper/Cargo.toml --quiet     # TOPOLOGY_ROOT unset
  ```
  Expect: fmt silent (exit 0); clippy no warnings; `cargo test` adds 1 test to the 551 baseline →
  `552 passed, 0 failed`. (Tasks 2–3 add no Rust tests.)
- **Commit:** none.

## Done criteria (maps to spec acceptance criteria)

- AC1 (fresh tree auto-wires portable settings, no manual adapt): verify gate — run `just setup` in a
  throwaway worktree, assert portable `.claude/settings.json`.
- AC2 (re-run no-op on settings.json): Task 1 test + verify gate.
- AC3 (self-governed claude apply-rerun-noop characterization, single-root harness, explicit no-op):
  Task 1.
- AC4 (trigger points decided/wired; post-checkout declined): Task 2 + Task 3 (DEVELOPMENT.md) + spec.
- AC5 (DEVELOPMENT.md links ADR-0019 + states build coupling): Task 3.1.
- AC6 (Unreleased CHANGELOG entry): Task 3.2.
