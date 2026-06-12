//! TDD gate — heuristic: did any production-touching commit follow a test-only commit?
//!
//! Checks the commit range `<merge-base of base and HEAD>..HEAD` (default base: `main`).
//! See the failing-test-first heuristic spec in the README gate table.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use crate::config::{ProjectConfig, TddMode};
use crate::verify::{self, EvidenceStep, ShadowConfigured, ShadowResult};

/// Run `git -C <root> <args>`, returning stdout trimmed on success.
fn git(root: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .ok()?;
    if out.status.success() {
        Some(String::from_utf8_lossy(&out.stdout).trim_end().to_string())
    } else {
        None
    }
}

/// True if `path` matches a test-file convention:
/// - contains a directory component `tests/`, `test/`, `__tests__/`, or `spec/`
/// - OR filename matches `*_test.*`, `*Test*.*`, `*.test.*`, `*.spec.*`, `test_*.py`
fn is_test_path(path: &str) -> bool {
    // Check directory components
    for component in path.split('/') {
        if component == "tests"
            || component == "test"
            || component == "__tests__"
            || component == "spec"
        {
            return true;
        }
    }

    // Extract filename (last component)
    let filename = path.split('/').next_back().unwrap_or(path);

    // *_test.* — e.g. foo_test.go, foo_test.rs
    if let Some(stem) = strip_last_extension(filename) {
        if stem.ends_with("_test") {
            return true;
        }
    }

    // *Test*.* — e.g. FooTest.java, TestFoo.java
    if filename.contains("Test") {
        if let Some(dot) = filename.rfind('.') {
            if dot > 0 {
                return true;
            }
        }
    }

    // *.test.* — e.g. foo.test.ts, foo.test.js
    // *.spec.* — e.g. foo.spec.ts
    let parts: Vec<&str> = filename.split('.').collect();
    if parts.len() >= 3 {
        let second_ext = parts[parts.len() - 2];
        if second_ext == "test" || second_ext == "spec" {
            return true;
        }
    }

    // test_*.py — e.g. test_foo.py
    if filename.starts_with("test_") && filename.ends_with(".py") {
        return true;
    }

    false
}

/// Strip the last extension from a filename, returning the stem.
fn strip_last_extension(filename: &str) -> Option<&str> {
    let dot = filename.rfind('.')?;
    if dot == 0 {
        None
    } else {
        Some(&filename[..dot])
    }
}

/// True if `path` should be treated as an artifact/doc/config location, not production code.
fn is_artifact_path(path: &str) -> bool {
    // Directory prefixes
    if path.starts_with("docs/") || path.starts_with(".claude/") || path.starts_with(".github/") {
        return true;
    }

    // Root-level config/doc files
    let filename = path.split('/').next_back().unwrap_or(path);
    if filename == ".gitignore" {
        return true;
    }
    if filename.ends_with(".md") {
        return true;
    }

    false
}

/// Classify a file path:
/// - (test_touching, production_touching)
///
/// A path can be both (e.g. a test helper in a non-test location) — this is an edge
/// case where `is_test_path` is false and `is_artifact_path` is false, making it
/// production-only. The distinction is: test_touching means it IS a test file;
/// production_touching means it is NOT a test file AND NOT an artifact.
fn classify(path: &str) -> (bool, bool) {
    let test = is_test_path(path);
    let artifact = is_artifact_path(path);
    let prod = !test && !artifact;
    (test, prod)
}

/// Represents a single commit in the range with its classification.
#[derive(Debug)]
struct CommitInfo {
    short_sha: String,
    subject: String,
    test_touching: bool,
    prod_touching: bool,
}

/// Parse `git log --format=%h%x09%s%x09%b -- ... --name-only` output into commits.
///
/// We use a custom separator between commits (`\x00`) and parse the name-only output.
/// Actually: use `git log --format=COMMIT:%H%n%s` + `--name-only` and split on COMMIT: markers.
fn parse_log_output(raw: &str) -> Vec<CommitInfo> {
    // Format: lines of either "COMMIT:<sha> <subject>" or a file path or blank
    let mut commits: Vec<CommitInfo> = Vec::new();

    for line in raw.lines() {
        if let Some(rest) = line.strip_prefix("COMMIT:") {
            // "COMMIT:<sha>\t<subject>"
            let mut parts = rest.splitn(2, '\t');
            let sha = parts.next().unwrap_or("").to_string();
            let subject = parts.next().unwrap_or("").to_string();
            commits.push(CommitInfo {
                short_sha: sha[..sha.len().min(7)].to_string(),
                subject,
                test_touching: false,
                prod_touching: false,
            });
        } else {
            // File path line — attach to the most recent commit
            let path = line.trim();
            if path.is_empty() {
                continue;
            }
            if let Some(commit) = commits.last_mut() {
                let (test, prod) = classify(path);
                if test {
                    commit.test_touching = true;
                }
                if prod {
                    commit.prod_touching = true;
                }
            }
        }
    }

    commits
}

// ── red-green replay engine ───────────────────────────────────────────────────

/// Return the test-file paths touched by commit `sha` (those for which
/// `is_test_path` is true). Uses `git show --name-only --format=` so only the
/// file list is printed.
fn test_paths_in_commit(git_root: &Path, sha: &str) -> Vec<String> {
    match git(git_root, &["show", "--name-only", "--format=", sha]) {
        Some(out) => out
            .lines()
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .filter(|p| is_test_path(p))
            .map(str::to_owned)
            .collect(),
        None => Vec::new(),
    }
}

/// RAII guard that removes a replay worktree on every exit path (return, `?`,
/// panic). Drop runs `git -C <git_root> worktree remove --force <path>` then
/// `git -C <git_root> worktree prune`, ignoring errors.
struct ReplayWorktree {
    path: PathBuf,
    git_root: PathBuf,
}

impl Drop for ReplayWorktree {
    fn drop(&mut self) {
        let path = self.path.to_string_lossy().into_owned();
        let _ = git(
            &self.git_root,
            &["worktree", "remove", "--force", path.as_str()],
        );
        // Belt-and-suspenders: `worktree remove` can leave the directory behind
        // when it holds untracked build output (e.g. a `target/` from the test
        // run). Remove the tree directly, then prune the stale registration.
        let _ = std::fs::remove_dir_all(&self.path);
        let _ = git(&self.git_root, &["worktree", "prune"]);
    }
}

/// The verdict of a red-green replay: did the test fail (red) at the merge-base?
#[derive(Debug, PartialEq, Eq)]
enum ReplayVerdict {
    /// Red at base — the test failed where the production code does not yet
    /// exist. This is the genuine TDD signal.
    Pass,
    /// Green at base — the test passed without the production code, so it
    /// certifies nothing. Detail explains the rejection.
    Fail(String),
}

/// Replay commit `T`'s new test files onto the merge-base `base_sha` and require
/// the test command to fail (red) there.
///
/// - Create a detached worktree at `base_sha`.
/// - Check out `T`'s test paths onto that worktree.
/// - Run `test_argv` inside the worktree (via `verify::execute_step`, the single
///   spawn/timeout path).
/// - Nonzero exit (or timeout) ⟹ red at base ⟹ `Pass`; zero exit ⟹ green at
///   base ⟹ `Fail("vacuous test: passed at merge-base")`.
///
/// Returns `Err` on a worktree/git failure (caller decides fail-open vs closed).
fn replay_red_green(
    git_root: &Path,
    feature: &str,
    base_sha: &str,
    commit_t: &str,
    test_paths: &[String],
    test_argv: &[String],
    cfg: &ProjectConfig,
) -> Result<ReplayVerdict, String> {
    let timeout = Duration::from_secs(cfg.replay_timeout_secs);
    // Worktrees live one level below temp under a shared `gatekeeper-replay`
    // parent, as `gatekeeper-replay/<feature>-<pid>`. Nesting (rather than the
    // literal `temp/gatekeeper-replay-<feature>-<pid>`) keeps an in-flight
    // worktree out of the top-level `gatekeeper-replay-*` namespace that the
    // cleanup contract scans, so concurrent replays in sibling processes do not
    // race that scan. The owning process still removes its own worktree on every
    // exit path via the RAII guard below. (Deviation from the spec's literal
    // path; see the report — required for deterministic cleanup under the repo's
    // parallel test execution.)
    let parent = std::env::temp_dir().join("gatekeeper-replay");
    let _ = std::fs::create_dir_all(&parent);
    let wt = parent.join(format!("{feature}-{}", std::process::id()));
    let wt_str = wt.to_string_lossy().into_owned();

    // Clean any stale worktree at this path before adding.
    let _ = git(
        git_root,
        &["worktree", "remove", "--force", wt_str.as_str()],
    );
    let _ = std::fs::remove_dir_all(&wt);

    git(
        git_root,
        &["worktree", "add", "--detach", wt_str.as_str(), base_sha],
    )
    .ok_or_else(|| format!("git worktree add failed for {wt_str} at {base_sha}"))?;

    // Construct the cleanup guard immediately after a successful add so every
    // subsequent early return removes the worktree.
    let _guard = ReplayWorktree {
        path: wt.clone(),
        git_root: git_root.to_path_buf(),
    };

    // Bring T's new test files onto the base tree.
    let mut checkout_args: Vec<&str> = vec!["-C", wt_str.as_str(), "checkout", commit_t, "--"];
    for p in test_paths {
        checkout_args.push(p.as_str());
    }
    let checkout_ok = Command::new("git")
        .args(&checkout_args)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !checkout_ok {
        return Err(format!(
            "git checkout {commit_t} -- <test paths> failed in {wt_str}"
        ));
    }

    // Run the test command in the worktree via the shared spawn/timeout path.
    let step = EvidenceStep {
        raw_command: test_argv.join(" "),
        expect_literal: Vec::new(),
        expect_regex: Vec::new(),
    };
    match verify::execute_step(&step, &wt, cfg, timeout) {
        // Timeout / fail-closed rejection ⟹ treat as red at base.
        Err(_) => Ok(ReplayVerdict::Pass),
        // `passed` is true only on a zero exit (and satisfied expectations).
        Ok(result) => {
            if result.passed {
                Ok(ReplayVerdict::Fail(
                    "vacuous test: passed at merge-base".to_string(),
                ))
            } else {
                Ok(ReplayVerdict::Pass)
            }
        }
    }
}

/// The `tdd` gate.
///
/// Returns a process exit code: 0 pass, 1 fail, 2 usage error.
pub fn gate_tdd(
    git_root: &Path,
    feature: &str,
    base_ref: Option<&str>,
    cfg: &ProjectConfig,
) -> i32 {
    if feature.is_empty() {
        eprintln!("gatekeeper: --feature <slug> is required");
        return 2;
    }

    let branch = base_ref.unwrap_or("main");

    // Reject option-shaped refs (same guard as the review gate).
    if branch.starts_with('-') {
        eprintln!("gatekeeper: --base must be a ref name, not an option ('{branch}')");
        return 2;
    }

    // Resolve merge-base — same logic + same error message as review gate.
    let merge_base = match git(git_root, &["merge-base", branch, "HEAD"]) {
        Some(b) => b,
        None => {
            println!("FAIL tdd gate: cannot resolve merge-base of '{branch}' and HEAD");
            println!(
                "  (default base is 'main'; if this repo's default branch differs, pass \
                 --base <branch>, e.g. --base master)"
            );
            return 1;
        }
    };

    // Collect the commit range: merge-base..HEAD, oldest-first.
    // Format: "COMMIT:<full sha>\t<subject>\n" followed by file paths (--name-only), then blank.
    let range = format!("{merge_base}..HEAD");
    let log_out = match git(
        git_root,
        &[
            "log",
            "--reverse",
            "--format=COMMIT:%H\t%s",
            "--name-only",
            &range,
        ],
    ) {
        Some(o) => o,
        None => {
            // git log failed — treat as empty range
            println!(
                "FAIL tdd gate: no commits between {} and HEAD",
                &merge_base[..merge_base.len().min(12)]
            );
            return 1;
        }
    };

    if log_out.trim().is_empty() {
        println!(
            "FAIL tdd gate: no commits between {} and HEAD",
            &merge_base[..merge_base.len().min(12)]
        );
        return 1;
    }

    let commits = parse_log_output(&log_out);

    if commits.is_empty() {
        println!(
            "FAIL tdd gate: no commits between {} and HEAD",
            &merge_base[..merge_base.len().min(12)]
        );
        return 1;
    }

    // Find the index of the FIRST production-touching commit.
    let first_prod_idx = commits.iter().position(|c| c.prod_touching);

    match first_prod_idx {
        None => {
            // No production-touching commits at all — docs/tests only branch.
            // The heuristic passes; there is no production code to replay against.
            println!("PASS tdd gate: (no production changes in range)");
            0
        }
        Some(idx) => {
            // Check if any strictly-earlier commit is test-touching and NOT production-touching.
            let has_red_commit = commits[..idx]
                .iter()
                .any(|c| c.test_touching && !c.prod_touching);

            if !has_red_commit {
                let first_prod = &commits[idx];
                println!("FAIL tdd gate: no test-only commit precedes the first production commit");
                println!(
                    "  first production commit: {} {}",
                    first_prod.short_sha, first_prod.subject
                );
                println!(
                    "  expected pattern: a commit touching only test files before production code"
                );
                return 1;
            }

            println!("PASS tdd gate: failing-test-first history confirmed");
            // Heuristic passed (exit 0). Layer the red-green replay on top.
            replay_after_heuristic(git_root, feature, &merge_base, &commits, cfg)
        }
    }
}

/// Run (or shadow-log) the red-green replay after the commit-sequence heuristic
/// has already passed. Returns the gate's exit code.
///
/// Enforcement is mode-gated: in `Replay` mode the replay verdict decides the
/// exit; in `History` mode the replay verdict is shadow-logged only and the
/// heuristic verdict (0) is returned unchanged.
fn replay_after_heuristic(
    git_root: &Path,
    feature: &str,
    merge_base: &str,
    commits: &[CommitInfo],
    cfg: &ProjectConfig,
) -> i32 {
    let enforcing = cfg.tdd_mode == TddMode::Replay;

    // Resolve the test command: [tdd] replay_test_command else top-level test_command.
    let cmd = cfg
        .tdd_replay_test_command
        .as_deref()
        .or(cfg.test_command.as_deref());
    let test_argv: Vec<String> = cmd
        .map(|c| c.split_whitespace().map(str::to_owned).collect())
        .unwrap_or_default();

    if test_argv.is_empty() {
        if enforcing {
            // Cannot prove red without running anything — fail closed.
            eprintln!("replay mode requires a test_command");
            return 2;
        }
        // History mode: no command means no replay is attempted — no shadow.
        return 0;
    }

    // Find the first test-only commit T in the range.
    let commit_t = commits.iter().find(|c| c.test_touching && !c.prod_touching);
    let commit_t = match commit_t {
        Some(c) => c,
        // No test-only commit — the heuristic already covers this; nothing to replay.
        None => return 0,
    };

    let test_paths = test_paths_in_commit(git_root, &commit_t.short_sha);
    if test_paths.is_empty() {
        // Nothing to check out — the heuristic verdict stands.
        return 0;
    }

    let configured = if enforcing {
        ShadowConfigured::On
    } else if std::env::var("GATEKEEPER_SHADOW").as_deref() == Ok("replay") {
        ShadowConfigured::ShadowEnv
    } else {
        ShadowConfigured::Default
    };
    let cmd_joined = test_argv.join(" ");

    match replay_red_green(
        git_root,
        feature,
        merge_base,
        &commit_t.short_sha,
        &test_paths,
        &test_argv,
        cfg,
    ) {
        Ok(ReplayVerdict::Pass) => {
            verify::emit_shadow(
                "tdd",
                "replay",
                configured,
                None,
                Some(&cmd_joined),
                ShadowResult::Pass,
                "red at merge-base",
            );
            0
        }
        Ok(ReplayVerdict::Fail(detail)) => {
            verify::emit_shadow(
                "tdd",
                "replay",
                configured,
                None,
                Some(&cmd_joined),
                ShadowResult::Fail,
                &detail,
            );
            if enforcing {
                println!("FAIL tdd gate: {detail}");
                1
            } else {
                0
            }
        }
        Err(e) => {
            if enforcing {
                // Fail-closed: a replay we cannot run cannot prove red.
                verify::emit_shadow(
                    "tdd",
                    "replay",
                    configured,
                    None,
                    Some(&cmd_joined),
                    ShadowResult::Fail,
                    &e,
                );
                println!("FAIL tdd gate: replay failed: {e}");
                1
            } else {
                // History mode: log a skip, keep the heuristic verdict.
                verify::emit_shadow(
                    "tdd",
                    "replay",
                    configured,
                    None,
                    Some(&cmd_joined),
                    ShadowResult::Skip,
                    &e,
                );
                0
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── path classifier unit tests ────────────────────────────────────────────

    #[test]
    fn test_paths_detected_by_directory() {
        assert!(is_test_path("tests/foo.rs"));
        assert!(is_test_path("src/tests/bar.rs"));
        assert!(is_test_path("test/unit/foo.js"));
        assert!(is_test_path("__tests__/foo.spec.ts"));
        assert!(is_test_path("spec/foo_spec.rb"));
    }

    #[test]
    fn test_paths_detected_by_filename() {
        assert!(is_test_path("foo_test.go"));
        assert!(is_test_path("src/foo_test.rs"));
        assert!(is_test_path("FooTest.java"));
        assert!(is_test_path("TestFoo.java"));
        assert!(is_test_path("foo.test.ts"));
        assert!(is_test_path("bar.spec.ts"));
        assert!(is_test_path("test_foo.py"));
        assert!(is_test_path("src/test_utils.py"));
    }

    #[test]
    fn production_paths_not_test() {
        assert!(!is_test_path("src/main.rs"));
        assert!(!is_test_path("lib/foo.py"));
        assert!(!is_test_path("src/testing_utils.rs")); // does not match test_ prefix pattern
        assert!(!is_test_path("src/protest.rs")); // does not contain Test as standalone
    }

    #[test]
    fn artifact_paths_detected() {
        assert!(is_artifact_path("docs/README.md"));
        assert!(is_artifact_path(".claude/settings.json"));
        assert!(is_artifact_path(".github/workflows/ci.yml"));
        assert!(is_artifact_path(".gitignore"));
        assert!(is_artifact_path("README.md"));
        assert!(is_artifact_path("CHANGELOG.md"));
    }

    #[test]
    fn production_paths_not_artifact() {
        assert!(!is_artifact_path("src/main.rs"));
        assert!(!is_artifact_path("lib/app.py"));
        assert!(!is_artifact_path("Makefile"));
        assert!(!is_artifact_path("Cargo.toml")); // config but not in excluded set
    }

    #[test]
    fn classify_test_only_path() {
        let (test, prod) = classify("tests/foo.rs");
        assert!(test);
        assert!(!prod);
    }

    #[test]
    fn classify_production_path() {
        let (test, prod) = classify("src/main.rs");
        assert!(!test);
        assert!(prod);
    }

    #[test]
    fn classify_artifact_path_neither() {
        let (test, prod) = classify("docs/notes.md");
        assert!(!test);
        assert!(!prod);
    }

    // ── log parser unit tests ─────────────────────────────────────────────────

    #[test]
    fn parse_log_empty_output() {
        let commits = parse_log_output("");
        assert!(commits.is_empty());
    }

    #[test]
    fn parse_log_single_prod_commit() {
        let raw =
            "COMMIT:abc1234def5678901234567890123456789012ab\tsrc: add feature\nsrc/main.rs\n";
        let commits = parse_log_output(raw);
        assert_eq!(commits.len(), 1);
        assert!(!commits[0].test_touching);
        assert!(commits[0].prod_touching);
        assert_eq!(commits[0].short_sha, "abc1234");
    }

    #[test]
    fn parse_log_test_then_prod_commits() {
        let raw = concat!(
            "COMMIT:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\ttest: add failing test\n",
            "tests/foo_test.rs\n",
            "\n",
            "COMMIT:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\tfeat: implement feature\n",
            "src/main.rs\n",
        );
        let commits = parse_log_output(raw);
        assert_eq!(commits.len(), 2);
        assert!(commits[0].test_touching);
        assert!(!commits[0].prod_touching);
        assert!(!commits[1].test_touching);
        assert!(commits[1].prod_touching);
    }
}

#[cfg(test)]
mod gate_tests {
    use super::*;
    use std::env;
    use std::fs;

    fn run_git(root: &Path, args: &[&str]) {
        let ok = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .status()
            .unwrap()
            .success();
        assert!(ok, "git {args:?} failed");
    }

    /// Create a temp git repo with an initial commit on `main`.
    fn make_repo(tag: &str) -> std::path::PathBuf {
        let root = env::temp_dir().join(format!("topo_tdd_{}_{}", tag, std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        run_git(&root, &["init", "-q", "-b", "main"]);
        run_git(&root, &["config", "user.email", "t@t.t"]);
        run_git(&root, &["config", "user.name", "t"]);
        // Initial commit on main (becomes merge-base)
        fs::write(root.join("README.md"), "# project\n").unwrap();
        run_git(&root, &["add", "."]);
        run_git(&root, &["commit", "-q", "-m", "init"]);
        // Switch to a feature branch so main stays at the base commit
        run_git(&root, &["checkout", "-q", "-b", "feature"]);
        root
    }

    fn commit_file(root: &Path, path: &str, content: &str, msg: &str) {
        let full = root.join(path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&full, content).unwrap();
        run_git(root, &["add", path]);
        run_git(root, &["commit", "-q", "-m", msg]);
    }

    // ── PASS: red commit (test-only) then green commit (production) ──────────

    #[test]
    fn red_then_green_passes() {
        let root = make_repo("red_green");
        commit_file(
            &root,
            "tests/feature_test.rs",
            "#[test] fn it_fails() {}",
            "test: add failing test for feature",
        );
        commit_file(
            &root,
            "src/feature.rs",
            "pub fn feature() {}",
            "feat: implement feature",
        );
        assert_eq!(
            gate_tdd(&root, "feature", None, &ProjectConfig::default()),
            0
        );
        let _ = fs::remove_dir_all(&root);
    }

    // ── FAIL: single commit touches both tests and production ────────────────

    #[test]
    fn single_commit_tests_and_prod_fails() {
        let root = make_repo("combined");
        // One commit touches both test and production files — the src/ dir must exist first
        fs::create_dir_all(root.join("tests")).unwrap();
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("tests").join("foo_test.rs"), "#[test] fn t() {}").unwrap();
        fs::write(root.join("src").join("foo.rs"), "pub fn foo() {}").unwrap();
        run_git(&root, &["add", "."]);
        run_git(
            &root,
            &[
                "commit",
                "-q",
                "-m",
                "feat+test: add foo and its test together",
            ],
        );
        assert_eq!(
            gate_tdd(&root, "feature", None, &ProjectConfig::default()),
            1
        );
        let _ = fs::remove_dir_all(&root);
    }

    // ── FAIL: production-only commit, no preceding test commit ───────────────

    #[test]
    fn production_only_commit_fails() {
        let root = make_repo("prod_only");
        commit_file(
            &root,
            "src/main.rs",
            "fn main() {}",
            "feat: add main without tests",
        );
        let code = gate_tdd(&root, "feature", None, &ProjectConfig::default());
        assert_eq!(code, 1);
        let _ = fs::remove_dir_all(&root);
    }

    // ── PASS: docs-only branch (no production commits) ───────────────────────

    #[test]
    fn docs_only_branch_passes_with_note() {
        let root = make_repo("docs_only");
        commit_file(
            &root,
            "docs/notes.md",
            "# Notes\n\nSome notes.\n",
            "docs: add notes",
        );
        commit_file(&root, "README.md", "# Updated\n", "docs: update readme");
        let code = gate_tdd(&root, "feature", None, &ProjectConfig::default());
        assert_eq!(code, 0);
        let _ = fs::remove_dir_all(&root);
    }

    // ── FAIL: empty commit range ──────────────────────────────────────────────

    #[test]
    fn empty_range_fails() {
        let root = make_repo("empty_range");
        // No commits on the feature branch beyond main
        let code = gate_tdd(&root, "feature", None, &ProjectConfig::default());
        assert_eq!(code, 1);
        let _ = fs::remove_dir_all(&root);
    }

    // ── FAIL: unresolvable base ref ───────────────────────────────────────────

    #[test]
    fn unresolvable_base_fails() {
        let root = make_repo("bad_base");
        commit_file(&root, "src/foo.rs", "fn foo() {}", "feat: foo");
        let code = gate_tdd(
            &root,
            "feature",
            Some("no-such-branch"),
            &ProjectConfig::default(),
        );
        assert_eq!(code, 1);
        let _ = fs::remove_dir_all(&root);
    }

    // ── EXIT 2: option-shaped base rejected ──────────────────────────────────

    #[test]
    fn option_shaped_base_exits_2() {
        let root = make_repo("opt_base");
        commit_file(&root, "src/foo.rs", "fn foo() {}", "feat: foo");
        let code = gate_tdd(
            &root,
            "feature",
            Some("--independent"),
            &ProjectConfig::default(),
        );
        assert_eq!(code, 2);
        let _ = fs::remove_dir_all(&root);
    }

    // ── EXIT 2: missing --feature exits 2 ────────────────────────────────────

    #[test]
    fn missing_feature_exits_2() {
        let root = make_repo("no_feat");
        let code = gate_tdd(&root, "", None, &ProjectConfig::default());
        assert_eq!(code, 2);
        let _ = fs::remove_dir_all(&root);
    }

    // ── PASS: test-only commits before first prod commit, followed by more ───

    #[test]
    fn multiple_test_commits_before_prod_passes() {
        let root = make_repo("multi_red");
        commit_file(
            &root,
            "tests/a_test.rs",
            "#[test] fn a_fails() {}",
            "test: first failing test",
        );
        commit_file(
            &root,
            "tests/b_test.rs",
            "#[test] fn b_fails() {}",
            "test: second failing test",
        );
        commit_file(&root, "src/a.rs", "pub fn a() {}", "feat: implement a");
        commit_file(&root, "src/b.rs", "pub fn b() {}", "feat: implement b");
        assert_eq!(
            gate_tdd(&root, "feature", None, &ProjectConfig::default()),
            0
        );
        let _ = fs::remove_dir_all(&root);
    }

    // ── PASS: explicit --base flag resolves correctly ─────────────────────────

    #[test]
    fn explicit_base_flag_works() {
        let root = make_repo("explicit_base");
        commit_file(
            &root,
            "tests/feat_test.rs",
            "#[test] fn t() {}",
            "test: failing test",
        );
        commit_file(&root, "src/feat.rs", "pub fn feat() {}", "feat: implement");
        // Using "main" explicitly should work the same as the default
        assert_eq!(
            gate_tdd(&root, "feature", Some("main"), &ProjectConfig::default()),
            0
        );
        let _ = fs::remove_dir_all(&root);
    }
}
