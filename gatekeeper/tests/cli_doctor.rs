//! Integration tests for `gatekeeper doctor`.
//!
//! All tests use scratch roots — no coupling to the real repo tree.
//! Mirrors the scratch-root / run helper style of cli_check.rs.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

const VALID_RULES_TOML: &str = "schema_version = 1\n";

const VALID_SKILL_MD: &str = "\
---\n\
name: test-skill\n\
description: A test skill for doctor probes.\n\
---\n\
\n\
Body text.\n";

/// Build a minimal healthy scratch root with no .git/ (so pre-commit is n/a).
/// Returns the root path.
fn scratch_root(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("topo_doctor_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);

    // Create the root directory first (required before writing any files into it).
    fs::create_dir_all(&root).unwrap();

    // AGENTS.md — required marker so is_marked_root() passes (Phase 11 F1 check).
    fs::write(
        root.join("AGENTS.md"),
        "# Topology Framework\n\nThis is a test root.\n",
    )
    .unwrap();

    // skills/ — the framework_root() anchor (+ AGENTS.md above = is_marked_root)
    fs::create_dir_all(root.join("skills").join("test-skill")).unwrap();
    fs::write(
        root.join("skills").join("test-skill").join("SKILL.md"),
        VALID_SKILL_MD,
    )
    .unwrap();

    // security/rules.toml
    fs::create_dir_all(root.join("security")).unwrap();
    fs::write(root.join("security").join("rules.toml"), VALID_RULES_TOML).unwrap();

    // instincts/ (empty dir — doctor must not fail on an empty instincts dir)
    fs::create_dir_all(root.join("instincts")).unwrap();

    // hooks/ — one executable .sh
    fs::create_dir_all(root.join("hooks")).unwrap();
    let hook = root.join("hooks").join("test-hook.sh");
    fs::write(&hook, "#!/usr/bin/env bash\necho ok\n").unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();

    root
}

fn run(cwd: &Path, args: &[&str]) -> (i32, String) {
    run_with_env(cwd, args, &[])
}

fn run_with_env(cwd: &Path, args: &[&str], env_vars: &[(&str, &str)]) -> (i32, String) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_gatekeeper"));
    cmd.current_dir(cwd).args(args);
    // After Phase 11, binary-adjacent resolution points at the actual topology repo,
    // so tests that create scratch roots must pin TOPOLOGY_ROOT to keep
    // framework_root() == scratch root (which controls which rules.toml / hooks/ are checked).
    // Canonicalize so /var/folders/... matches /private/var/folders/... on macOS.
    // Tests that need a different root override this via env_vars.
    let canonical_cwd = std::fs::canonicalize(cwd).unwrap_or_else(|_| cwd.to_path_buf());
    cmd.env("TOPOLOGY_ROOT", &canonical_cwd);
    for (k, v) in env_vars {
        cmd.env(k, v);
    }
    let out = cmd.output().unwrap();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
}

/// Write a `.claude/settings.json` into `root` with one PreToolUse hook `command` and an optional
/// `env.GATEKEEPER_BIN`. Used by the settings-path probe tests (issue #52).
fn write_settings(root: &Path, hook_command: &str, gatekeeper_bin: Option<&str>) {
    let claude = root.join(".claude");
    fs::create_dir_all(&claude).unwrap();
    let env_block = match gatekeeper_bin {
        Some(b) => format!("\"env\": {{ \"GATEKEEPER_BIN\": \"{b}\" }},"),
        None => String::new(),
    };
    let json = format!(
        "{{\n  {env_block}\n  \"hooks\": {{\n    \"PreToolUse\": [\n      \
         {{ \"hooks\": [ {{ \"type\": \"command\", \"command\": \"{hook_command}\", \
         \"timeout\": 30 }} ] }}\n    ]\n  }}\n}}"
    );
    fs::write(claude.join("settings.json"), json).unwrap();
}

// ── Healthy root → exit 0 ─────────────────────────────────────────────────

#[test]
fn doctor_healthy_root_exits_0() {
    let root = scratch_root("healthy");
    let (code, out) = run(&root, &["doctor"]);
    assert_eq!(code, 0, "healthy root should exit 0; out:\n{out}");
    // Must print binary path and version.
    assert!(
        out.contains("binary:"),
        "output should contain 'binary:'; got:\n{out}"
    );
    assert!(
        out.contains("gatekeeper ") && out.contains("(rules schema v"),
        "output should contain version line; got:\n{out}"
    );
    // Must name the resolution mechanism.
    assert!(
        out.contains("resolution split:"),
        "output should contain 'resolution split:'; got:\n{out}"
    );
    // Summary line.
    assert!(
        out.contains("doctor: all probes ok"),
        "output should end with summary ok; got:\n{out}"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn doctor_does_not_write_any_files() {
    let root = scratch_root("readonly");
    // Collect directory listing before.
    let before: Vec<_> = walkdir(&root);
    let (_, _) = run(&root, &["doctor"]);
    let after: Vec<_> = walkdir(&root);
    assert_eq!(before, after, "doctor must not create or delete files");
    let _ = fs::remove_dir_all(&root);
}

// ── AC-7: roots and PATH version skew ────────────────────────────────────

#[test]
fn doctor_prints_all_three_root_lines() {
    let root = scratch_root("roots");
    let (code, out) = run(&root, &["doctor"]);
    assert_eq!(code, 0, "healthy root should exit 0; out:\n{out}");
    assert!(
        out.contains("framework root:"),
        "output must contain 'framework root:'; got:\n{out}"
    );
    // Phase 11: doctor must print "resolved by:" line after "framework root:".
    assert!(
        out.contains("resolved by:"),
        "output must contain 'resolved by:'; got:\n{out}"
    );
    assert!(
        out.contains("project root:"),
        "output must contain 'project root:'; got:\n{out}"
    );
    assert!(
        out.contains("artifacts root:"),
        "output must contain 'artifacts root:'; got:\n{out}"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn doctor_path_skew_is_informational_not_a_failure() {
    // Build a scratch dir with a fake 'gatekeeper' binary that prints an old version.
    let root = scratch_root("skew");
    let fake_bin_dir = std::env::temp_dir().join(format!("topo_fake_bin_{}", std::process::id()));
    let _ = fs::remove_dir_all(&fake_bin_dir);
    fs::create_dir_all(&fake_bin_dir).unwrap();

    // The fake binary just prints a static old-version string.
    let fake_bin = fake_bin_dir.join("gatekeeper");
    fs::write(
        &fake_bin,
        "#!/usr/bin/env bash\necho 'gatekeeper 0.0.1 (rules schema v0)'\n",
    )
    .unwrap();
    fs::set_permissions(&fake_bin, fs::Permissions::from_mode(0o755)).unwrap();

    // Prepend the fake bin dir to PATH.
    let original_path = std::env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{original_path}", fake_bin_dir.display());

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_gatekeeper"));
    cmd.current_dir(&root)
        .args(["doctor"])
        .env("PATH", &new_path);
    let out = cmd.output().unwrap();
    let code = out.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();

    // Exit code must be 0 — version skew is informational.
    assert_eq!(
        code, 0,
        "version skew must not fail (exit 0); out:\n{stdout}"
    );

    // Output must name the skew.
    assert!(
        stdout.contains("version skew"),
        "output must mention 'version skew'; got:\n{stdout}"
    );

    // The skew note must name both versions.
    assert!(
        stdout.contains("0.0.1"),
        "output must name the stale version; got:\n{stdout}"
    );

    let _ = fs::remove_dir_all(&root);
    let _ = fs::remove_dir_all(&fake_bin_dir);
}

// ── GATEKEEPER_BIN fault → exit 1 ────────────────────────────────────────

#[test]
fn doctor_gatekeeper_bin_nonexistent_exits_1() {
    let root = scratch_root("gkbin_fault");
    let (code, out) = run_with_env(
        &root,
        &["doctor"],
        &[("GATEKEEPER_BIN", "/nonexistent/gatekeeper")],
    );
    assert_eq!(
        code, 1,
        "GATEKEEPER_BIN=/nonexistent should exit 1; out:\n{out}"
    );
    assert!(
        out.contains("FAIL"),
        "output should name the FAIL; got:\n{out}"
    );
    assert!(
        out.contains("GATEKEEPER_BIN"),
        "output should mention GATEKEEPER_BIN; got:\n{out}"
    );
    let _ = fs::remove_dir_all(&root);
}

// ── rules.toml schema mismatch → exit 1 ──────────────────────────────────

#[test]
fn doctor_schema_version_mismatch_exits_1() {
    let root = scratch_root("schema_mismatch");
    // Overwrite rules.toml with a wrong schema version.
    fs::write(
        root.join("security").join("rules.toml"),
        "schema_version = 9\n",
    )
    .unwrap();
    let (code, out) = run(&root, &["doctor"]);
    assert_eq!(code, 1, "schema mismatch should exit 1; out:\n{out}");
    assert!(
        out.contains("FAIL"),
        "output should contain FAIL for rules.toml; got:\n{out}"
    );
    assert!(
        out.contains("security/rules.toml"),
        "output should name the offending probe; got:\n{out}"
    );
    let _ = fs::remove_dir_all(&root);
}

// ── non-executable hook → exit 1 ─────────────────────────────────────────

#[test]
fn doctor_non_executable_hook_exits_1() {
    let root = scratch_root("hook_noexec");
    // Remove execute bit from the hook.
    let hook = root.join("hooks").join("test-hook.sh");
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o644)).unwrap();
    let (code, out) = run(&root, &["doctor"]);
    assert_eq!(code, 1, "non-executable hook should exit 1; out:\n{out}");
    assert!(
        out.contains("FAIL"),
        "output should contain FAIL for hooks; got:\n{out}"
    );
    assert!(
        out.contains("test-hook.sh"),
        "output should name the offending hook; got:\n{out}"
    );
    let _ = fs::remove_dir_all(&root);
}

// ── pre-commit hook probe (git repo present) ─────────────────────────────

/// Build a scratch root that is also a git repository (for pre-commit probe tests).
fn scratch_root_with_git(tag: &str) -> PathBuf {
    let root = scratch_root(tag);
    // Initialise a git repo so `git rev-parse --git-path hooks` works.
    std::process::Command::new("git")
        .args(["-C", root.to_str().unwrap(), "init"])
        .output()
        .expect("git init failed");
    // Configure git identity so commits work (needed if we ever make a commit in tests).
    std::process::Command::new("git")
        .args([
            "-C",
            root.to_str().unwrap(),
            "config",
            "user.email",
            "test@example.com",
        ])
        .output()
        .expect("git config user.email failed");
    std::process::Command::new("git")
        .args(["-C", root.to_str().unwrap(), "config", "user.name", "Test"])
        .output()
        .expect("git config user.name failed");
    root
}

/// Install a hook into the scratch root's .git/hooks/pre-commit.
fn install_pre_commit_hook(root: &Path, content: &str) {
    let hooks_dir = root.join(".git").join("hooks");
    fs::create_dir_all(&hooks_dir).unwrap();
    let pc = hooks_dir.join("pre-commit");
    fs::write(&pc, content).unwrap();
    fs::set_permissions(&pc, fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn doctor_pre_commit_present_exits_0() {
    let root = scratch_root_with_git("pc_present");
    install_pre_commit_hook(
        &root,
        "#!/usr/bin/env bash\n# Topology pre-commit hook\nexec gatekeeper scan --staged\n",
    );
    let (code, out) = run(&root, &["doctor"]);
    assert_eq!(code, 0, "hook present should exit 0; out:\n{out}");
    assert!(
        out.contains(".git/hooks/pre-commit: ok"),
        "output should confirm hook ok; got:\n{out}"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn doctor_pre_commit_missing_in_git_repo_exits_1() {
    let root = scratch_root_with_git("pc_missing");
    // No pre-commit hook installed.
    let (code, out) = run(&root, &["doctor"]);
    assert_eq!(
        code, 1,
        "missing hook in a git repo should exit 1; out:\n{out}"
    );
    assert!(
        out.contains("FAIL"),
        "output should contain FAIL for missing pre-commit; got:\n{out}"
    );
    assert!(
        out.contains(".git/hooks/pre-commit"),
        "output should name the probe; got:\n{out}"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn doctor_pre_commit_missing_in_framework_repo_names_just_setup() {
    // When framework root == project root (no VERSION file → dev checkout),
    // the FAIL message must mention `just setup`.
    let root = scratch_root_with_git("pc_framework");
    // No pre-commit hook installed; no VERSION file → dev checkout.
    let (code, out) = run(&root, &["doctor"]);
    assert_eq!(
        code, 1,
        "missing hook in framework dev checkout should exit 1; out:\n{out}"
    );
    assert!(
        out.contains("just setup"),
        "FAIL message must mention 'just setup' for framework dev checkout; got:\n{out}"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn doctor_pre_commit_missing_in_governed_project_names_install_sh() {
    // When framework root != project root (VERSION file present → payload install),
    // the FAIL message must NOT mention `just setup` — it should name scripts/install.sh.
    let root = scratch_root_with_git("pc_governed");
    // Add a VERSION file to make doctor treat this as a payload install (framework ≠ project).
    let my_version = env!("CARGO_PKG_VERSION");
    fs::write(
        root.join("VERSION"),
        format!("version = \"{my_version}\"\nrules_schema = 1\n"),
    )
    .unwrap();
    // No pre-commit hook installed.
    let (code, out) = run(&root, &["doctor"]);
    assert_eq!(
        code, 1,
        "missing hook in governed project should exit 1; out:\n{out}"
    );
    assert!(
        out.contains("FAIL"),
        "output should contain FAIL; got:\n{out}"
    );
    assert!(
        !out.contains("just setup"),
        "governed project FAIL must not mention 'just setup'; got:\n{out}"
    );
    assert!(
        out.contains("scripts/install.sh"),
        "governed project FAIL must mention 'scripts/install.sh'; got:\n{out}"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn doctor_no_git_repo_pre_commit_is_na() {
    // The existing scratch_root() has no .git at all → n/a.
    let root = scratch_root("pc_norepo");
    let (code, out) = run(&root, &["doctor"]);
    assert_eq!(code, 0, "no git repo should exit 0; out:\n{out}");
    assert!(
        out.contains(".git/hooks/pre-commit: n/a"),
        "output should say n/a when no git repo; got:\n{out}"
    );
    let _ = fs::remove_dir_all(&root);
}

// ── Task 5: orphaned replay-worktree probe (informational) ────────────────

#[test]
fn doctor_warns_on_orphaned_replay_worktree() {
    // The replay engine nests worktrees under temp_dir()/gatekeeper-replay/<feature>-<pid>.
    // An orphan is any child left under that parent. Pre-create one with a UNIQUE name so
    // concurrent test runs (whose live worktrees may also sit under this parent) cannot make
    // the assertion flaky: we assert presence (count >= 1), not an exact count.
    let root = scratch_root("orphan_replay");
    let parent = std::env::temp_dir().join("gatekeeper-replay");
    let unique = format!("orphan-test-{}-{}", std::process::id(), line!());
    let orphan = parent.join(&unique);
    fs::create_dir_all(&orphan).unwrap();

    let (code, out) = run(&root, &["doctor"]);

    // The orphan probe is INFORMATIONAL: an otherwise-healthy root must still exit 0
    // even though an orphan exists under the replay parent.
    assert_eq!(
        code, 0,
        "orphaned replay worktree is informational, must not change exit code; out:\n{out}"
    );

    // Output must contain an informational line about replay worktrees that names a
    // nonzero count — not "0" and not "ok" — because at least one orphan is present.
    assert!(
        out.contains("replay worktrees:"),
        "output must contain a 'replay worktrees:' informational line; got:\n{out}"
    );
    let replay_line = out
        .lines()
        .find(|l| l.contains("replay worktrees:"))
        .unwrap_or("");
    assert!(
        !replay_line.contains(" 0 ") && !replay_line.contains("ok"),
        "with an orphan present the replay-worktrees line must name a nonzero count \
         (not '0', not 'ok'); got line: {replay_line:?}\nfull out:\n{out}"
    );

    // Cleanup the orphan we created (leave any sibling live worktrees alone).
    let _ = fs::remove_dir_all(&orphan);
    let _ = fs::remove_dir_all(&root);
}

// ── Task 5: [tdd] config section is recognized ────────────────────────────

#[test]
fn doctor_recognizes_tdd_config_section() {
    let root = scratch_root("tdd_config");
    // artifacts_root() == <root>/docs when project root == framework root (dev checkout).
    let docs = root.join("docs");
    fs::create_dir_all(&docs).unwrap();
    fs::write(
        docs.join("config.toml"),
        "[tdd]\nmode = \"replay\"\nreplay_test_command = \"cargo test\"\n",
    )
    .unwrap();

    let (_code, out) = run(&root, &["doctor"]);

    // Doctor must NOT flag `tdd` as an unrecognized top-level key.
    let flags_tdd_top = out
        .lines()
        .any(|l| (l.contains("unrecognized") || l.contains("unknown")) && l.contains("tdd"));
    assert!(
        !flags_tdd_top,
        "doctor must not flag 'tdd' as an unrecognized top-level key; got:\n{out}"
    );

    // Doctor must NOT flag the [tdd] sub-keys `mode` / `replay_test_command` as unknown.
    let flags_tdd_subkeys = out.lines().any(|l| {
        (l.contains("unrecognized") || l.contains("unknown"))
            && (l.contains("mode") || l.contains("replay_test_command"))
    });
    assert!(
        !flags_tdd_subkeys,
        "doctor must not flag [tdd] sub-keys 'mode'/'replay_test_command' as unknown; got:\n{out}"
    );

    let _ = fs::remove_dir_all(&root);
}

// ── settings.json stale-path probe (advisory, issue #52) ──────────────────

#[test]
fn doctor_warns_on_stale_settings_hook_path() {
    let root = scratch_root("settings_stale_hook");
    write_settings(&root, "/nonexistent/topology/hooks/security-scan.sh", None);
    let (code, out) = run(&root, &["doctor"]);
    assert_eq!(
        code, 0,
        "stale settings path is advisory and must not change the exit code; out:\n{out}"
    );
    assert!(
        out.contains(
            "hook command path does not exist: /nonexistent/topology/hooks/security-scan.sh"
        ),
        "doctor must WARN naming the stale hook path; got:\n{out}"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn doctor_no_warn_on_resolvable_portable_hook_path() {
    let root = scratch_root("settings_portable_ok");
    // scratch_root() created hooks/test-hook.sh; reference it via the portable literal.
    write_settings(&root, "${CLAUDE_PROJECT_DIR}/hooks/test-hook.sh", None);
    let (code, out) = run(&root, &["doctor"]);
    assert_eq!(code, 0, "out:\n{out}");
    assert!(
        out.contains("settings.json paths: ok"),
        "a resolvable portable hook path must report ok; got:\n{out}"
    );
    assert!(
        !out.contains("WARN: hook command path"),
        "a resolvable portable hook path must not WARN; got:\n{out}"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn doctor_warns_on_stale_gatekeeper_bin() {
    let root = scratch_root("settings_stale_bin");
    write_settings(
        &root,
        "${CLAUDE_PROJECT_DIR}/hooks/test-hook.sh",
        Some("/nonexistent/gatekeeper/target/release/gatekeeper"),
    );
    let (code, out) = run(&root, &["doctor"]);
    assert_eq!(code, 0, "out:\n{out}");
    assert!(
        out.contains(
            "GATEKEEPER_BIN path does not exist: /nonexistent/gatekeeper/target/release/gatekeeper"
        ),
        "doctor must WARN naming the stale GATEKEEPER_BIN path; got:\n{out}"
    );
    let _ = fs::remove_dir_all(&root);
}

// ── Helpers ───────────────────────────────────────────────────────────────

/// Collect all file paths under `root` for before/after comparison.
fn walkdir(root: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    collect_paths(root, &mut paths);
    paths.sort();
    paths
}

fn collect_paths(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        out.push(p.clone());
        if p.is_dir() {
            collect_paths(&p, out);
        }
    }
}
