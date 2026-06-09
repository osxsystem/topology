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

    // skills/ — the framework_root() anchor
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
    for (k, v) in env_vars {
        cmd.env(k, v);
    }
    let out = cmd.output().unwrap();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
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
