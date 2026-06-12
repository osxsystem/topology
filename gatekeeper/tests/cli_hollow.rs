//! Adversarial "hollow artifact" scoreboard — FM2 track (spec §1).
//!
//! Seven fixtures; each asserts that the relevant gate REJECTS a semantically empty
//! artifact.  Today every gate ACCEPTS these artifacts, so every test body reaches
//! its `assert_ne!` and FAILS — that red state is the scoreboard.  The `#[ignore]`
//! attribute keeps the default test suite green while the failures stay visible under
//! `cargo test --test cli_hollow -- --ignored`.
//!
//! Un-ignoring a fixture is the definition of progress for each gate hardening:
//!   (a) un-ignored by task 7  — design substance floor (spec §4)
//!   (b) un-ignored by task 6  — verify evidence replay (spec §3)
//!   (c) stays ignored          — Phase 15 red-green replay
//!   (d) stays ignored          — Phase 17 review judge
//!   (e) un-ignored by task 8  — finish zero-test floor (spec §5)
//!   (f) stays ignored          — Phase 17 plan judge
//!   (g) un-ignored by task 8  — finish zero-test floor (spec §5)

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

// ── shared helpers ────────────────────────────────────────────────────────────

/// Minimal framework root recognised by `framework_root()`: a `skills/` dir plus
/// one of the ROOT_MARKERS.  We use `AGENTS.md` (matches `cli_review.rs`).
fn scratch_root(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("topo_hollow_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("skills")).unwrap();
    fs::write(root.join("AGENTS.md"), "").unwrap();
    root
}

/// Run `gatekeeper <args>` from `cwd`.  Returns `(exit_code, combined_stdout)`.
fn run(cwd: &Path, args: &[&str]) -> (i32, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_gatekeeper"))
        .current_dir(cwd)
        .args(args)
        .output()
        .unwrap();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
}

/// Run a git command inside `root`; panics on failure (mirrors `cli_review.rs`).
fn git(root: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .status()
        .unwrap()
        .success();
    assert!(ok, "git {args:?} failed");
}

/// Read the HEAD sha of `root` (full 40-char hex).
fn head_sha(root: &Path) -> String {
    let out = Command::new("git")
        .args(["-C", root.to_str().unwrap(), "rev-parse", "HEAD"])
        .output()
        .unwrap();
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

// ── (a) spec containing ONLY `Status: approved` ──────────────────────────────

#[test]
fn hollow_a_approved_only_spec() {
    // The design gate's future substance floor (config-gated: `[design]
    // substance_floor = true`) requires ≥2 `##` headings and at least one
    // non-empty body line besides the `Status:` line.  A spec that is literally
    // just the approval marker has zero headings and zero body — today the gate
    // checks only for the approval marker and passes (the config key does not
    // exist yet and is silently ignored); once §4 lands it must reject this.
    let root = scratch_root("a_spec");
    let research_dir = root.join("docs").join("research");
    let specs_dir = root.join("docs").join("specs");
    fs::create_dir_all(&research_dir).unwrap();
    fs::create_dir_all(&specs_dir).unwrap();

    // config.toml opting in to the substance floor (silently ignored today).
    fs::write(
        root.join("docs").join("config.toml"),
        "[design]\nsubstance_floor = true\n",
    )
    .unwrap();

    // Research note present (satisfies the sequence-lock).
    fs::write(
        research_dir.join("2026-06-11-hollow-a.md"),
        "# Research\n\nSome findings here.\n",
    )
    .unwrap();

    // Spec with ONLY the approval marker — no headings, no body.
    fs::write(
        specs_dir.join("2026-06-11-hollow-a.md"),
        "Status: approved\n",
    )
    .unwrap();

    let (code, out) = run(&root, &["check", "design", "--feature", "hollow-a"]);
    assert_ne!(
        code, 0,
        "HOLLOW PASS: spec with only 'Status: approved' was accepted by the design gate; out: {out}"
    );

    let _ = fs::remove_dir_all(&root);
}

// ── (b) empty verify file ─────────────────────────────────────────────────────

#[test]
fn hollow_b_empty_verify_file() {
    // The verify gate's future replay mode (spec §3) fails-closed on zero evidence
    // blocks: an artifact with no ``` evidence ``` fenced blocks fails.  Today the
    // gate only checks file existence and passes an empty file; once §3 lands with
    // `[verify] mode = "replay"` the gate must reject it.
    //
    // The config.toml carries `[verify]\nmode = "replay"` — the key does not yet
    // exist in `ProjectConfig`, so it is silently ignored and the gate still passes
    // today (demonstrating that the red state is the assertion, not a config error).
    let root = scratch_root("b_verify");
    let verify_dir = root.join("docs").join("verify");
    fs::create_dir_all(&verify_dir).unwrap();

    // Empty verify artifact — zero evidence blocks.
    fs::write(verify_dir.join("2026-06-11-hollow-b.md"), "").unwrap();

    // config.toml opting in to replay mode (silently ignored today).
    fs::write(
        root.join("docs").join("config.toml"),
        "[verify]\nmode = \"replay\"\n",
    )
    .unwrap();

    let (code, out) = run(&root, &["check", "verify", "--feature", "hollow-b"]);
    assert_ne!(
        code, 0,
        "HOLLOW PASS: empty verify file was accepted by the verify gate; out: {out}"
    );

    let _ = fs::remove_dir_all(&root);
}

// ── (c) TDD gate: assert!(true) red commit ────────────────────────────────────

#[test]
#[ignore = "red until Phase 15 red-green replay checks test quality"]
fn hollow_c_assert_true_red_commit() {
    // The TDD gate's current heuristic only checks that a test-file-only commit
    // precedes the first production commit; it does not inspect test content.  A
    // test commit whose body is `assert!(true)` satisfies the heuristic today and
    // the gate passes.  Phase 15 will add red-green replay that detects a test that
    // cannot fail (i.e. never actually "red").
    let root = scratch_root("c_tdd");

    // git repo: initial commit on `main` (becomes the merge-base).
    git(&root, &["init", "-q", "-b", "main"]);
    git(&root, &["config", "user.email", "t@t.t"]);
    git(&root, &["config", "user.name", "t"]);
    fs::write(root.join("README.md"), "# hollow-c\n").unwrap();
    git(&root, &["add", "."]);
    git(&root, &["commit", "-q", "-m", "init"]);

    // Record the merge-base sha for --base.
    let base = head_sha(&root);

    // Switch to feature branch.
    git(&root, &["checkout", "-q", "-b", "feature"]);

    // "Red" commit: test-only file, but the test body is `assert!(true)` (never fails).
    fs::create_dir_all(root.join("tests")).unwrap();
    fs::write(
        root.join("tests").join("hollow_c_test.rs"),
        "#[test]\nfn hollow_c_always_passes() {\n    assert!(true);\n}\n",
    )
    .unwrap();
    git(&root, &["add", "tests/hollow_c_test.rs"]);
    git(
        &root,
        &[
            "commit",
            "-q",
            "-m",
            "test: hollow red commit (assert!(true))",
        ],
    );

    // Production commit.
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src").join("hollow_c.rs"), "pub fn hollow() {}\n").unwrap();
    git(&root, &["add", "src/hollow_c.rs"]);
    git(
        &root,
        &["commit", "-q", "-m", "feat: hollow implementation"],
    );

    let (code, out) = run(
        &root,
        &["check", "tdd", "--feature", "hollow-c", "--base", &base],
    );
    assert_ne!(
        code, 0,
        "HOLLOW PASS: assert!(true) test-only commit was accepted as a valid red commit; out: {out}"
    );

    let _ = fs::remove_dir_all(&root);
}

// ── (d) review artifact with superficial body ─────────────────────────────────

#[test]
#[ignore = "red until Phase 17 review judge checks substantive criteria evidence"]
fn hollow_d_looks_fine_review() {
    // The review gate's current parser checks artifact structure (VERDICT, HEAD,
    // BASE, blocking section, criteria subsections) but not the substance of review
    // evidence.  A structurally-valid artifact whose criteria subsections contain
    // only "Looks fine." passes today.  Phase 17's judge will require evidence that
    // references specific code locations.
    let root = scratch_root("d_review");

    // git repo with one commit (HEAD == merge-base for a single-commit repo).
    git(&root, &["init", "-q", "-b", "main"]);
    git(&root, &["config", "user.email", "t@t.t"]);
    git(&root, &["config", "user.name", "t"]);
    fs::write(root.join("README.md"), "# hollow-d\n").unwrap();
    git(&root, &["add", "."]);
    git(&root, &["commit", "-q", "-m", "init"]);

    let head = head_sha(&root);

    // Structurally-valid pass artifact with hollow criteria evidence.
    let reviews_dir = root.join("docs").join("reviews");
    fs::create_dir_all(&reviews_dir).unwrap();
    let body = format!(
        "VERDICT: pass\nHEAD: {head}\nBASE: {head}\n\n\
         # Review\n\n\
         ## Blocking findings\n\
         None.\n\n\
         ## Criteria checked\n\
         ### Spec/plan\n\
         Looks fine.\n\
         ### Standards\n\
         Looks fine.\n"
    );
    fs::write(reviews_dir.join("2026-06-11-hollow-d.md"), body).unwrap();

    let (code, out) = run(&root, &["check", "review", "--feature", "hollow-d"]);
    assert_ne!(
        code, 0,
        "HOLLOW PASS: superficial 'Looks fine.' review was accepted by the review gate; out: {out}"
    );

    let _ = fs::remove_dir_all(&root);
}

// ── (e) finish gate: test_command = "true" ────────────────────────────────────

#[test]
fn hollow_e_test_command_true() {
    // `true` exits 0 but runs zero tests.  The finish gate's future zero-test floor
    // (spec §5) with `[finish] require_test_count = true` must reject a command that
    // produces no recognisable runner summary.  Today the gate checks only the exit
    // code and passes `true` unconditionally.
    let root = scratch_root("e_finish");
    let docs_dir = root.join("docs");
    fs::create_dir_all(&docs_dir).unwrap();

    // config.toml: test_command = "true" with require_test_count enabled.
    fs::write(
        docs_dir.join("config.toml"),
        "test_command = \"true\"\n\n[finish]\nrequire_test_count = true\n",
    )
    .unwrap();

    // Run with no `-- cmd` override so the gate reads test_command from config.
    let (code, out) = run(&root, &["check", "finish"]);
    assert_ne!(
        code, 0,
        "HOLLOW PASS: 'true' (zero tests) was accepted by the finish gate with require_test_count; out: {out}"
    );

    let _ = fs::remove_dir_all(&root);
}

// ── (f) plan dodging the denylist with synonyms ───────────────────────────────

#[test]
#[ignore = "red until Phase 17 plan judge detects semantic placeholders"]
fn hollow_f_synonym_placeholder_plan() {
    // The plan gate's denylist (PLACEHOLDERS in main.rs) checks for literal strings:
    //   "tbd", "implement later", "similar to task", "appropriate validation",
    //   "to be determined", "fill in later".
    //
    // A plan can dodge every entry by using synonyms that are semantically hollow
    // but literally absent from the denylist.  Today the gate passes such a plan.
    // Phase 17's judge will use semantic analysis to detect this class of evasion.
    let root = scratch_root("f_plan");
    let plans_dir = root.join("docs").join("plans");
    fs::create_dir_all(&plans_dir).unwrap();

    // Plan with synonym placeholders — none match the literal denylist.
    fs::write(
        plans_dir.join("2026-06-11-hollow-f.md"),
        "# Plan — hollow-f\n\n\
         ## Tasks\n\n\
         | # | Task | Notes |\n\
         |---|------|-------|\n\
         | 1 | Set up the module | Details forthcoming. |\n\
         | 2 | Wire the interface | We will figure this out as we go. |\n\
         | 3 | Add error handling | Specifics to be resolved later. |\n\
         | 4 | Write documentation | We'll sort this out once the API stabilises. |\n\
         | 5 | Deploy | Steps yet to be clarified. |\n",
    )
    .unwrap();

    let (code, out) = run(&root, &["check", "plan", "--feature", "hollow-f"]);
    assert_ne!(
        code, 0,
        "HOLLOW PASS: synonym-placeholder plan was accepted by the plan gate; out: {out}"
    );

    let _ = fs::remove_dir_all(&root);
}

// ── (g) finish gate: zero-test runner (exit 0, no runner summary) ─────────────

#[test]
fn hollow_g_zero_test_runner() {
    // Distinct from (e): the command emits a *recognised* cargo summary line whose
    // executed-test count is zero.  (e) covers the no-recognisable-summary class;
    // this covers recognised-summary-zero-count.  The future zero-test floor must
    // reject both: a runner that ran nothing is not verification.
    let root = scratch_root("g_finish");
    let docs_dir = root.join("docs");
    fs::create_dir_all(&docs_dir).unwrap();

    // A command that exits 0 and prints a genuine cargo-format summary with a
    // zero count (config test_command runs via `sh -c`, so echo works here).
    fs::write(
        docs_dir.join("config.toml"),
        "test_command = \"echo 'test result: ok. 0 passed; 0 failed; 0 ignored'\"\n\n[finish]\nrequire_test_count = true\n",
    )
    .unwrap();

    let (code, out) = run(&root, &["check", "finish"]);
    assert_ne!(
        code, 0,
        "HOLLOW PASS: recognised summary with zero tests was accepted by the finish gate with require_test_count; out: {out}"
    );

    let _ = fs::remove_dir_all(&root);
}
