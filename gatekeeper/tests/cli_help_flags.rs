//! Integration tests for `--help` / `-h` and unknown-flag rejection (fix for issue #27).
//!
//! Every subcommand must:
//!   - print usage and exit 0 on `--help` or `-h` (without reading stdin).
//!   - print an error + usage to stderr and exit 2 on an unrecognized flag.
//!   - pass `check finish -- cmd --any-flag` through unmolested (flags after `--` are not scanned).
//!   - keep existing valid invocations working (regression guard).

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

// ── helpers ──────────────────────────────────────────────────────────────────

/// Minimal framework root: skills/ + AGENTS.md + security/rules.toml.
fn scratch_root(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("topo_help_flags_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("skills")).unwrap();
    fs::write(root.join("AGENTS.md"), "").unwrap();
    fs::create_dir_all(root.join("security")).unwrap();
    fs::write(
        root.join("security").join("rules.toml"),
        "schema_version = 1\n",
    )
    .unwrap();
    root
}

/// Run `gatekeeper <args>` from `cwd` with a closed stdin pipe.
/// Returns `(exit_code, stdout, stderr)`.
fn run(cwd: &Path, args: &[&str]) -> (i32, String, String) {
    run_stdin(cwd, args, b"")
}

/// Run `gatekeeper <args>` from `cwd`, writing `stdin_bytes` to stdin (then closing it).
/// Returns `(exit_code, stdout, stderr)`.
fn run_stdin(cwd: &Path, args: &[&str], stdin_bytes: &[u8]) -> (i32, String, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_gatekeeper"))
        .current_dir(cwd)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn gatekeeper");
    let mut child_stdin = child.stdin.take().unwrap();
    if let Err(e) = child_stdin.write_all(stdin_bytes) {
        if e.kind() != std::io::ErrorKind::BrokenPipe {
            panic!("failed to write child stdin: {e}");
        }
    }
    drop(child_stdin);
    let out = child.wait_with_output().unwrap();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

// ── activate --help ────────────────────────────────────────────────────────

/// `activate --help` must exit 0 and print usage without reading stdin.
#[test]
fn activate_help_exits_0_prints_usage_no_stdin() {
    let root = scratch_root("act_help");
    // Pass a non-writing stdin (empty) to prove it never blocks waiting for input.
    let (code, out, _stderr) = run(&root, &["activate", "--help"]);
    assert_eq!(code, 0, "activate --help must exit 0; stdout: {out}");
    assert!(
        out.contains("activate"),
        "activate --help output should mention 'activate'; got: {out}"
    );
    let _ = fs::remove_dir_all(&root);
}

/// `activate -h` must also exit 0 without reading stdin.
#[test]
fn activate_short_help_exits_0() {
    let root = scratch_root("act_shelp");
    let (code, out, _) = run(&root, &["activate", "-h"]);
    assert_eq!(code, 0, "activate -h must exit 0; stdout: {out}");
    assert!(
        out.contains("activate"),
        "output should mention 'activate'; got: {out}"
    );
    let _ = fs::remove_dir_all(&root);
}

/// `activate --explain` is an unrecognized flag → exit 2, names the flag.
#[test]
fn activate_unknown_flag_exits_2_names_flag() {
    let root = scratch_root("act_unk");
    let (code, _out, stderr) = run(&root, &["activate", "--explain"]);
    assert_eq!(code, 2, "activate --explain must exit 2; stderr: {stderr}");
    assert!(
        stderr.contains("unknown flag") && stderr.contains("--explain"),
        "stderr must name the unknown flag; got: {stderr}"
    );
    let _ = fs::remove_dir_all(&root);
}

// ── list --help ────────────────────────────────────────────────────────────

#[test]
fn list_help_exits_0() {
    let root = scratch_root("list_help");
    let (code, out, _) = run(&root, &["list", "--help"]);
    assert_eq!(code, 0, "list --help must exit 0; stdout: {out}");
    assert!(
        out.contains("list"),
        "output should mention 'list'; got: {out}"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn list_unknown_flag_exits_2() {
    let root = scratch_root("list_unk");
    let (code, _, stderr) = run(&root, &["list", "--bogus"]);
    assert_eq!(code, 2, "list --bogus must exit 2; stderr: {stderr}");
    assert!(
        stderr.contains("unknown flag") && stderr.contains("--bogus"),
        "stderr must name the flag; got: {stderr}"
    );
    let _ = fs::remove_dir_all(&root);
}

// ── check --help ───────────────────────────────────────────────────────────

#[test]
fn check_help_exits_0() {
    let root = scratch_root("chk_help");
    let (code, out, _) = run(&root, &["check", "--help"]);
    assert_eq!(code, 0, "check --help must exit 0; stdout: {out}");
    assert!(
        out.contains("check"),
        "output should mention 'check'; got: {out}"
    );
    let _ = fs::remove_dir_all(&root);
}

/// `check design --bogus` is an unrecognized flag on the design gate → exit 2.
#[test]
fn check_design_unknown_flag_exits_2() {
    let root = scratch_root("chk_des_unk");
    let (code, _, stderr) = run(&root, &["check", "design", "--bogus"]);
    assert_eq!(
        code, 2,
        "check design --bogus must exit 2; stderr: {stderr}"
    );
    assert!(
        stderr.contains("unknown flag") && stderr.contains("--bogus"),
        "stderr must name the flag; got: {stderr}"
    );
    let _ = fs::remove_dir_all(&root);
}

/// `check finish -- echo --weird-flag` must pass through (flags after `--` are not scanned).
#[test]
fn check_finish_double_dash_passthrough() {
    let root = scratch_root("chk_fin_dd");
    // `echo --ok` exits 0 so the finish gate passes.
    let (code, out, _stderr) = run(&root, &["check", "finish", "--", "echo", "--ok"]);
    assert_eq!(
        code, 0,
        "check finish -- echo --ok must pass (echo exits 0); stdout: {out}"
    );
    assert!(out.contains("PASS"), "output should say PASS; got: {out}");
    let _ = fs::remove_dir_all(&root);
}

/// `check finish --help` exits 0 (before executing any command).
#[test]
fn check_finish_help_exits_0() {
    let root = scratch_root("chk_fin_help");
    let (code, out, _) = run(&root, &["check", "finish", "--help"]);
    assert_eq!(code, 0, "check finish --help must exit 0; stdout: {out}");
    assert!(
        out.contains("finish"),
        "output should mention 'finish'; got: {out}"
    );
    let _ = fs::remove_dir_all(&root);
}

// ── scan --help ────────────────────────────────────────────────────────────

#[test]
fn scan_help_exits_0() {
    let root = scratch_root("scan_help");
    let (code, out, _) = run(&root, &["scan", "--help"]);
    assert_eq!(code, 0, "scan --help must exit 0; stdout: {out}");
    assert!(
        out.contains("scan"),
        "output should mention 'scan'; got: {out}"
    );
    let _ = fs::remove_dir_all(&root);
}

/// `scan --content` is a known valid flag — must not be rejected.
#[test]
fn scan_content_valid_flag_still_works() {
    let root = scratch_root("scan_content_valid");
    // `scan --content` reads stdin; feed it an empty string → clean → exit 0.
    let (code, _out, stderr) = run_stdin(&root, &["scan", "--content"], b"");
    assert_eq!(
        code, 0,
        "scan --content with empty stdin must exit 0; stderr: {stderr}"
    );
    let _ = fs::remove_dir_all(&root);
}

// ── instinct --help ────────────────────────────────────────────────────────

#[test]
fn instinct_help_exits_0() {
    let root = scratch_root("inst_help");
    let (code, out, _) = run(&root, &["instinct", "--help"]);
    assert_eq!(code, 0, "instinct --help must exit 0; stdout: {out}");
    assert!(
        out.contains("instinct"),
        "output should mention 'instinct'; got: {out}"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn instinct_list_help_exits_0() {
    let root = scratch_root("inst_list_help");
    fs::create_dir_all(root.join("instincts")).unwrap();
    let (code, out, _) = run(&root, &["instinct", "list", "--help"]);
    assert_eq!(code, 0, "instinct list --help must exit 0; stdout: {out}");
    assert!(
        out.contains("instinct"),
        "output should mention 'instinct'; got: {out}"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn instinct_list_unknown_flag_exits_2() {
    let root = scratch_root("inst_list_unk");
    fs::create_dir_all(root.join("instincts")).unwrap();
    let (code, _, stderr) = run(&root, &["instinct", "list", "--bogus"]);
    assert_eq!(
        code, 2,
        "instinct list --bogus must exit 2; stderr: {stderr}"
    );
    assert!(
        stderr.contains("unknown flag") && stderr.contains("--bogus"),
        "stderr must name the flag; got: {stderr}"
    );
    let _ = fs::remove_dir_all(&root);
}

// ── adapt --help ───────────────────────────────────────────────────────────

#[test]
fn adapt_help_exits_0() {
    let root = scratch_root("adapt_help");
    let (code, out, _) = run(&root, &["adapt", "--help"]);
    assert_eq!(code, 0, "adapt --help must exit 0; stdout: {out}");
    assert!(
        out.contains("adapt"),
        "output should mention 'adapt'; got: {out}"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn adapt_unknown_flag_exits_2() {
    let root = scratch_root("adapt_unk");
    let (code, _, stderr) = run(&root, &["adapt", "--unknown"]);
    assert_eq!(code, 2, "adapt --unknown must exit 2; stderr: {stderr}");
    assert!(
        stderr.contains("unknown flag") && stderr.contains("--unknown"),
        "stderr must name the flag; got: {stderr}"
    );
    let _ = fs::remove_dir_all(&root);
}

// ── learn --help ───────────────────────────────────────────────────────────

#[test]
fn learn_help_exits_0() {
    let root = scratch_root("learn_help");
    let (code, out, _) = run(&root, &["learn", "--help"]);
    assert_eq!(code, 0, "learn --help must exit 0; stdout: {out}");
    assert!(
        out.contains("learn"),
        "output should mention 'learn'; got: {out}"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn learn_list_unknown_flag_exits_2() {
    let root = scratch_root("learn_list_unk");
    let (code, _, stderr) = run(&root, &["learn", "list", "--bogus"]);
    assert_eq!(code, 2, "learn list --bogus must exit 2; stderr: {stderr}");
    assert!(
        stderr.contains("unknown flag") && stderr.contains("--bogus"),
        "stderr must name the flag; got: {stderr}"
    );
    let _ = fs::remove_dir_all(&root);
}

// ── memory --help ──────────────────────────────────────────────────────────

#[test]
fn memory_help_exits_0() {
    let root = scratch_root("mem_help");
    let (code, out, _) = run(&root, &["memory", "--help"]);
    assert_eq!(code, 0, "memory --help must exit 0; stdout: {out}");
    assert!(
        out.contains("memory"),
        "output should mention 'memory'; got: {out}"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn memory_list_unknown_flag_exits_2() {
    let root = scratch_root("mem_list_unk");
    let (code, _, stderr) = run(&root, &["memory", "list", "--bogus"]);
    assert_eq!(code, 2, "memory list --bogus must exit 2; stderr: {stderr}");
    assert!(
        stderr.contains("unknown flag") && stderr.contains("--bogus"),
        "stderr must name the flag; got: {stderr}"
    );
    let _ = fs::remove_dir_all(&root);
}

// ── doctor --help ──────────────────────────────────────────────────────────

#[test]
fn doctor_help_exits_0() {
    let root = scratch_root("doc_help");
    let (code, out, _) = run(&root, &["doctor", "--help"]);
    assert_eq!(code, 0, "doctor --help must exit 0; stdout: {out}");
    assert!(
        out.contains("doctor"),
        "output should mention 'doctor'; got: {out}"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn doctor_unknown_flag_exits_2() {
    let root = scratch_root("doc_unk");
    let (code, _, stderr) = run(&root, &["doctor", "--bogus"]);
    assert_eq!(code, 2, "doctor --bogus must exit 2; stderr: {stderr}");
    assert!(
        stderr.contains("unknown flag") && stderr.contains("--bogus"),
        "stderr must name the flag; got: {stderr}"
    );
    let _ = fs::remove_dir_all(&root);
}
