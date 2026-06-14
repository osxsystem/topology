//! TDD red-green *replay* engine — Phase 15 (plan task 2).
//!
//! These are RED (TDD) tests for an engine that does not exist yet.  The
//! replay engine, once built, will: find the merge-base `B` and the first
//! test-only commit `T`; `git worktree add --detach <tmp> B`; `git -C <tmp>
//! checkout T -- <test paths>`; run the configured `test_command` inside
//! `<tmp>`; require a NONZERO exit ("red at base").  A zero exit at base means
//! the test is *vacuous* (it passed without the production code) → gate FAIL.
//!
//! Replay only fires when the scratch project's `docs/config.toml` carries
//! `[tdd]\nmode = "replay"` and a top-level `test_command`.  In `history` mode
//! the legacy heuristic verdict is preserved and the would-be replay verdict is
//! shadow-logged to stderr.
//!
//! Today's `check tdd` only runs the legacy "test-commit precedes prod-commit"
//! heuristic and never inspects test content, so:
//!   - `replay_rejects_vacuous_test` expects nonzero but gets 0 today → red.
//!   - `replay_accepts_genuine_red_first` expects 0 and gets 0 today → may pass
//!     vacuously (no engine yet); it pins the green-side contract.
//!   - `history_mode_skips_replay` expects a `SHADOW … "check":"replay"` line
//!     that is not emitted today → red.
//!   - `replay_cleans_up_worktree` may pass vacuously today (no worktree is
//!     created yet); it pins the RAII-cleanup contract.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

// ── shared helpers (mirrors cli_hollow.rs:23-66; the repo duplicates these
//    per test file — `run` here additionally merges stderr so the SHADOW line,
//    which goes to stderr, is visible to assertions) ─────────────────────────

/// Run `gatekeeper <args>` from `cwd`.  Returns `(exit_code, stdout+stderr)`.
fn run(cwd: &Path, args: &[&str]) -> (i32, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_gatekeeper"))
        .current_dir(cwd)
        .args(args)
        .output()
        .unwrap();
    let mut combined = String::from_utf8_lossy(&out.stdout).into_owned();
    combined.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.code().unwrap_or(-1), combined)
}

/// Run a git command inside `root`; panics on failure.
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

// ── scratch cargo crate builder ────────────────────────────────────────────

/// Create a *real* minimal cargo crate under `temp_dir()`, recognised as a
/// Topology framework root (`skills/` + `AGENTS.md`) and carrying a
/// `docs/config.toml` with the given `[tdd] mode` plus a top-level
/// `test_command = "cargo test"`.  The initial commit on `main` is the crate
/// skeleton; its sha is the merge-base.  Then a `feature` branch with a
/// test-only commit `T` (`tests/<test_file>` with `test_body`) followed by a
/// production commit `P` (`src/<prod_file>` with `prod_body`).
///
/// Returns `(root, base_sha)`.
fn build_replay_crate(
    tag: &str,
    mode: &str,
    test_file: &str,
    test_body: &str,
    prod_file: &str,
    prod_body: &str,
) -> (PathBuf, String) {
    let root = std::env::temp_dir().join(format!("topo_replay_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("skills")).unwrap();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(root.join("docs")).unwrap();
    fs::write(root.join("AGENTS.md"), "").unwrap();

    // Minimal cargo crate so `cargo test` runs inside the replay worktree.
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"replayfix\"\nversion = \"0.0.0\"\nedition = \"2021\"\n\n[lib]\npath = \"src/lib.rs\"\n",
    )
    .unwrap();
    fs::write(root.join("src").join("lib.rs"), "pub fn placeholder() {}\n").unwrap();

    // config.toml: replay (or history) mode + a runnable test command.
    fs::write(
        root.join("docs").join("config.toml"),
        format!("test_command = \"cargo test\"\n\n[tdd]\nmode = \"{mode}\"\n"),
    )
    .unwrap();

    // Initial commit on main = crate skeleton; its sha is the merge-base.
    git(&root, &["init", "-q", "-b", "main"]);
    git(&root, &["config", "user.email", "t@t.t"]);
    git(&root, &["config", "user.name", "t"]);
    git(&root, &["add", "."]);
    git(&root, &["commit", "-q", "-m", "init: crate skeleton"]);
    let base = head_sha(&root);

    // Feature branch.
    git(&root, &["checkout", "-q", "-b", "feature"]);

    // Commit T — test-only.
    fs::create_dir_all(root.join("tests")).unwrap();
    fs::write(root.join("tests").join(test_file), test_body).unwrap();
    git(&root, &["add", &format!("tests/{test_file}")]);
    git(&root, &["commit", "-q", "-m", "test: add behavior test"]);

    // Commit P — production.
    fs::write(root.join("src").join(prod_file), prod_body).unwrap();
    git(&root, &["add", &format!("src/{prod_file}")]);
    git(&root, &["commit", "-q", "-m", "feat: implement behavior"]);

    (root, base)
}

// ── 1. vacuous test (assert!(true)) is rejected by replay ──────────────────

#[test]
fn replay_rejects_vacuous_test() {
    // Commit T's test is `assert!(true)`: it compiles AND passes when checked
    // out onto the base (production code is irrelevant to it).  At base → exit
    // 0 → vacuous → the replay gate must reject it (nonzero exit).
    let (root, base) = build_replay_crate(
        "vac",
        "replay",
        "vac.rs",
        "#[test]\nfn vac() {\n    assert!(true);\n}\n",
        "feat.rs",
        "pub fn feat() {}\n",
    );

    let (code, out) = run(&root, &["check", "tdd", "--feature", "x", "--base", &base]);

    assert_ne!(
        code, 0,
        "VACUOUS PASS: assert!(true) test passed at the merge-base but the \
         replay gate accepted it; out: {out}"
    );
    let lower = out.to_lowercase();
    assert!(
        lower.contains("merge-base") || lower.contains("vacuous"),
        "expected the rejection to mention 'merge-base' or 'vacuous'; out: {out}"
    );

    let _ = fs::remove_dir_all(&root);
}

// ── 2. genuine red-first test is accepted by replay ────────────────────────

#[test]
fn replay_accepts_genuine_red_first() {
    // Commit T's test calls `replayfix::answer()`, which does NOT exist at
    // base → fails to compile at base → nonzero exit → genuinely red.  Commit P
    // adds `answer()`.  Replay must accept this (exit 0).
    let (root, base) = build_replay_crate(
        "real",
        "replay",
        "real.rs",
        "#[test]\nfn real() {\n    assert_eq!(replayfix::answer(), 42);\n}\n",
        "answer.rs",
        // NOTE: the production commit must wire `answer` into the crate's public
        // API.  We append a `mod`+`pub use` to lib.rs via the prod file plus a
        // module file; the simplest self-contained form is a single function in
        // lib.rs, but build_replay_crate only edits src/<prod_file>.  We define
        // the function in its own module and re-export it from lib at build time
        // below is impossible without touching lib.rs, so the prod file IS the
        // function and we rely on it being declared `pub` at crate root via a
        // `#[path]`-free `mod`.  Keep it dead-simple: put the fn in lib via the
        // prod body being a full module that lib re-exports.  To avoid editing
        // lib.rs, we instead make answer.rs the crate's lib by convention — but
        // tests import `replayfix::answer`, which requires it in the crate root.
        "pub fn answer() -> i32 { 42 }\n",
    );

    // The production fn must be reachable as `replayfix::answer`.  Wire it into
    // the crate root: append a `pub mod`/`pub use` to lib.rs on the feature tip
    // so commit P actually exposes `answer` (the test target at the merge-base
    // remains red because neither the module nor the fn exists at base).
    fs::write(
        root.join("src").join("lib.rs"),
        "pub fn placeholder() {}\n#[path = \"answer.rs\"]\nmod answer_mod;\npub use answer_mod::answer;\n",
    )
    .unwrap();
    git(&root, &["add", "src/lib.rs"]);
    git(
        &root,
        &["commit", "-q", "-m", "feat: expose answer at crate root"],
    );

    let (code, out) = run(&root, &["check", "tdd", "--feature", "x", "--base", &base]);

    assert_eq!(
        code, 0,
        "GENUINE RED REJECTED: a test that fails to compile at the merge-base \
         (answer() absent) should pass the replay gate; out: {out}"
    );

    let _ = fs::remove_dir_all(&root);
}

// ── 3. history mode preserves legacy verdict + shadow-logs replay ──────────

#[test]
fn history_mode_skips_replay() {
    // Same vacuous repo as test 1, but `mode = "history"`: the legacy heuristic
    // (test commit precedes prod commit) passes → exit 0.  The would-be replay
    // verdict is shadow-logged: a `SHADOW ` line on stderr whose JSON carries
    // "gate":"tdd" and "check":"replay".
    let (root, base) = build_replay_crate(
        "hist",
        "history",
        "vac.rs",
        "#[test]\nfn vac() {\n    assert!(true);\n}\n",
        "feat.rs",
        "pub fn feat() {}\n",
    );

    let (code, out) = run(&root, &["check", "tdd", "--feature", "x", "--base", &base]);

    assert_eq!(
        code, 0,
        "history mode must preserve the legacy heuristic verdict (exit 0); out: {out}"
    );

    let shadow_line = out
        .lines()
        .find(|l| l.starts_with("SHADOW "))
        .unwrap_or_else(|| {
            panic!("expected a `SHADOW ` line on stderr in history mode; out: {out}")
        });
    assert!(
        shadow_line.contains("\"gate\":\"tdd\""),
        "SHADOW line must carry \"gate\":\"tdd\"; line: {shadow_line}"
    );
    assert!(
        shadow_line.contains("\"check\":\"replay\""),
        "SHADOW line must carry \"check\":\"replay\"; line: {shadow_line}"
    );

    let _ = fs::remove_dir_all(&root);
}

// ── 4. RAII guard removes the replay worktree on every exit path ───────────

#[test]
fn replay_cleans_up_worktree() {
    // Run test 1's vacuous scenario, then assert the engine left NO worktree
    // behind for this feature (the ReplayWorktree Drop guard must remove it even
    // when the gate fails).
    //
    // The engine nests its worktrees at `temp_dir()/gatekeeper-replay/<feature>-<pid>`,
    // NOT at the top level as `gatekeeper-replay-*`. So we scan the nested
    // `gatekeeper-replay` parent for CHILD entries left behind whose name starts
    // with this test's feature slug. A unique slug (with this process id) keeps
    // the scan immune to sibling replays running in parallel test processes — we
    // only count children we could have created. PROOF that this is non-vacuous:
    // neutering `ReplayWorktree::drop` in src/tdd.rs makes this assertion fail
    // (the child `<feature>-<pid>` dir survives the run).
    let feature = format!("cleanup_{}", std::process::id());
    let (root, base) = build_replay_crate(
        "clean",
        "replay",
        "vac.rs",
        "#[test]\nfn vac() {\n    assert!(true);\n}\n",
        "feat.rs",
        "pub fn feat() {}\n",
    );

    let _ = run(
        &root,
        &["check", "tdd", "--feature", &feature, "--base", &base],
    );

    // The engine's worktree parent. Children are named `<feature>-<pid>`; we
    // assert none survive whose name starts with this test's unique feature slug.
    let parent = std::env::temp_dir().join("gatekeeper-replay");
    let orphans: Vec<PathBuf> = match fs::read_dir(&parent) {
        Ok(entries) => entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.starts_with(&feature))
                    .unwrap_or(false)
            })
            .collect(),
        // No parent dir at all ⟹ nothing left behind.
        Err(_) => Vec::new(),
    };

    assert!(
        orphans.is_empty(),
        "replay worktree(s) left behind under {}: {orphans:?}",
        parent.display()
    );

    let _ = fs::remove_dir_all(&root);
}

// ── 5. replay mode with no test_command anywhere → fail-closed (exit 2) ─────

#[test]
fn replay_mode_without_test_command_exits_2() {
    // Same shape as `replay_rejects_vacuous_test` (test-only commit T, then a
    // production commit), but the scratch `docs/config.toml` selects replay mode
    // while declaring NO test command at all — neither a top-level `test_command`
    // nor a `[tdd] replay_test_command`.  Replay has nothing to run, so the gate
    // must fail closed (exit 2) rather than silently passing or crashing.
    let (root, base) = build_replay_crate(
        "notest",
        "replay",
        "vac.rs",
        "#[test]\nfn vac() {\n    assert!(true);\n}\n",
        "feat.rs",
        "pub fn feat() {}\n",
    );

    // Overwrite the helper's config (which always writes `test_command`) with one
    // that selects replay mode but declares no command of any kind.
    fs::write(
        root.join("docs").join("config.toml"),
        "[tdd]\nmode = \"replay\"\n",
    )
    .unwrap();

    let (code, out) = run(&root, &["check", "tdd", "--feature", "x", "--base", &base]);

    assert_eq!(
        code, 2,
        "FAIL-OPEN: replay mode with no test_command must fail closed with exit 2; \
         got exit {code}; out: {out}"
    );
    assert!(
        out.to_lowercase()
            .contains("replay mode requires a test_command"),
        "expected the exit-2 error to say 'replay mode requires a test_command'; out: {out}"
    );

    let _ = fs::remove_dir_all(&root);
}

// ── 6. malformed/invalid config value → ParseFailed surfaced as exit 2 ──────

#[test]
fn tdd_parse_failed_exits_2() {
    // Build any minimal scratch crate, then OVERWRITE its `docs/config.toml` with
    // a malformed value for a known key (`mode = "bogus"` — an invalid `[tdd]`
    // mode → ParseFailed).  Today's handler uses the non-strict `ProjectConfig::load`,
    // which swallows ParseFailed and silently defaults, so the gate proceeds and
    // exits 0/1.  The handler must instead surface a config ParseFailed as a
    // usage/config error (exit 2).
    let (root, base) = build_replay_crate(
        "parsefail",
        "replay",
        "vac.rs",
        "#[test]\nfn vac() {\n    assert!(true);\n}\n",
        "feat.rs",
        "pub fn feat() {}\n",
    );

    fs::write(
        root.join("docs").join("config.toml"),
        "[tdd]\nmode = \"bogus\"\n",
    )
    .unwrap();

    let (code, out) = run(&root, &["check", "tdd", "--feature", "x", "--base", &base]);

    assert_eq!(
        code, 2,
        "SILENT DEFAULT: a malformed `[tdd] mode` value must surface as a config \
         ParseFailed (exit 2), not be swallowed; got exit {code}; out: {out}"
    );

    let _ = fs::remove_dir_all(&root);
}

// ── 7. replay mode with an UNRUNNABLE test_command → fail-closed (exit 2) ─────

#[test]
fn replay_unrunnable_command_fails_closed() {
    // A vacuous repo (assert!(true) test commit then a production commit) in
    // replay mode, with a configured `test_command` that CANNOT RUN — here a binary
    // that does not exist on PATH. A command that never reaches a real exit cannot
    // prove the test was red at the merge-base, so the gate must FAIL CLOSED rather
    // than certify a pass off a command it never executed. This is the FM2 guard.
    //
    // MECHANISM NOTE (slice #3): the replay-allowlist portability fix auto-includes
    // the configured `test_command` in the effective allowlist, so a non-default
    // command is no longer *rejected* by the allowlist. The genuine "never executed"
    // path is therefore a SPAWN FAILURE: `execute_step` returns
    // `Ok(StepResult { detail: "failed to spawn…" })`, which `replay_red_green` maps
    // to `Indeterminate` (tdd.rs:317-321) → fail-closed. This test guards the FM2
    // property via that spawn-failure path — independent of the allowlist mechanism,
    // which is exactly the soundness property that must survive the portability fix.
    let (root, base) = build_replay_crate(
        "unrunnable",
        "replay",
        "vac.rs",
        "#[test]\nfn vac() {\n    assert!(true);\n}\n",
        "feat.rs",
        "pub fn feat() {}\n",
    );

    // Replay mode with a command whose binary does not exist (auto-allowlisted by
    // the slice-3 fix, but unspawnable).
    fs::write(
        root.join("docs").join("config.toml"),
        "test_command = \"topology-no-such-test-runner-xyzzy run\"\n\n[tdd]\nmode = \"replay\"\n",
    )
    .unwrap();

    let (code, out) = run(&root, &["check", "tdd", "--feature", "x", "--base", &base]);

    assert_eq!(
        code, 2,
        "FAIL-OPEN: a replay command that cannot run (spawn failure) cannot prove \
         red and must fail closed with exit 2; got exit {code}; out: {out}"
    );
    // Tolerant fail-closed message check: SOME indication the command could not run
    // — NOT a pass.
    let lower = out.to_lowercase();
    assert!(
        lower.contains("spawn") || lower.contains("cannot") || lower.contains("indeterminate"),
        "expected a fail-closed message indicating the command could not run; out: {out}"
    );

    let _ = fs::remove_dir_all(&root);
}

// ── 8. history mode + unrunnable command → SHADOW logs skip, not pass ────────

#[test]
fn history_unrunnable_command_logs_skip_not_pass() {
    // Same vacuous repo, but `mode = "history"` (non-enforcing) with a
    // `test_command` whose binary does not exist. The legacy heuristic passes →
    // exit 0. The replay could NOT run (spawn failure), so the burn-in SHADOW log
    // must record `result:"skip"` — NOT `result:"pass"`. Logging a `pass` for a
    // command that never executed poisons the false-block-rate burn-in with a
    // phantom red. This is the FM2 burn-in-integrity guard.
    //
    // MECHANISM NOTE (slice #3): the configured command is now auto-allowlisted, so
    // the "never executed" path is the spawn failure → `Indeterminate` → `Skip`
    // (tdd.rs:317-321, 538-549). This test guards the property via that path.
    let (root, base) = build_replay_crate(
        "histunrunnable",
        "history",
        "vac.rs",
        "#[test]\nfn vac() {\n    assert!(true);\n}\n",
        "feat.rs",
        "pub fn feat() {}\n",
    );

    fs::write(
        root.join("docs").join("config.toml"),
        "test_command = \"topology-no-such-test-runner-xyzzy run\"\n\n[tdd]\nmode = \"history\"\n",
    )
    .unwrap();

    let (code, out) = run(&root, &["check", "tdd", "--feature", "x", "--base", &base]);

    assert_eq!(
        code, 0,
        "history mode is non-enforcing and must exit 0; got exit {code}; out: {out}"
    );

    let shadow_line = out
        .lines()
        .find(|l| l.starts_with("SHADOW ") && l.contains("\"check\":\"replay\""))
        .unwrap_or_else(|| {
            panic!("expected a `SHADOW ...\"check\":\"replay\"...` line; out: {out}")
        });
    assert!(
        shadow_line.contains("\"gate\":\"tdd\""),
        "SHADOW line must carry \"gate\":\"tdd\"; line: {shadow_line}"
    );
    assert!(
        !shadow_line.contains("\"result\":\"pass\""),
        "BURN-IN POISONED: a command that never ran must log \
         result:\"skip\", not result:\"pass\"; line: {shadow_line}"
    );
    assert!(
        shadow_line.contains("\"result\":\"skip\""),
        "expected the SHADOW line to log result:\"skip\" for a command that \
         could not run; line: {shadow_line}"
    );

    let _ = fs::remove_dir_all(&root);
}

// ── 9. slice #3: an auto-included non-default command RUNS (locks the wiring) ─

#[test]
fn replay_autoincluded_command_runs_and_establishes_red() {
    // slice #3 (replay-allowlist portability): a configured `test_command` that is
    // NOT in the default allowlist (here `false`, which exists on PATH) is
    // auto-included in the effective allowlist, so `execute_step` RUNS it instead of
    // rejecting it. `false` exits nonzero, which the replay reads as red at base ⇒
    // `ReplayVerdict::Pass` ⇒ the gate exits 0 and the SHADOW logs `result:"pass"`.
    //
    // NOTE: `false` is a degenerate always-nonzero stand-in chosen only because it is
    // guaranteed runnable AND non-default — it proves the command is RUN (not
    // rejected), not that a real test was exercised; the genuine-red contract is
    // pinned separately by `replay_rejects_vacuous_test` / `replay_accepts_genuine_red_first`.
    //
    // PRE-FIX this command was rejected (not in allowed_command_prefixes) ⇒ `Err` ⇒
    // `Indeterminate` ⇒ exit 2. So this test FAILS without the fix and locks the
    // wiring of `effective_allowed_prefixes()` into `execute_step` (verify.rs:471).
    let (root, base) = build_replay_crate(
        "autoincl",
        "replay",
        "vac.rs",
        "#[test]\nfn vac() {\n    assert!(true);\n}\n",
        "feat.rs",
        "pub fn feat() {}\n",
    );

    // A real, runnable, non-default command that exits nonzero (read as red at base).
    fs::write(
        root.join("docs").join("config.toml"),
        "test_command = \"false\"\n\n[tdd]\nmode = \"replay\"\n",
    )
    .unwrap();

    let (code, out) = run(&root, &["check", "tdd", "--feature", "x", "--base", &base]);

    assert_eq!(
        code, 0,
        "an auto-included non-default command must RUN and establish red ⇒ exit 0 \
         (pre-fix it was rejected ⇒ exit 2); got exit {code}; out: {out}"
    );
    let shadow_line = out
        .lines()
        .find(|l| l.starts_with("SHADOW ") && l.contains("\"check\":\"replay\""))
        .unwrap_or_else(|| {
            panic!("expected a `SHADOW ...\"check\":\"replay\"...` line; out: {out}")
        });
    assert!(
        shadow_line.contains("\"result\":\"pass\""),
        "expected replay to PASS (red established by the auto-included command that \
         actually ran); line: {shadow_line}"
    );

    let _ = fs::remove_dir_all(&root);
}
