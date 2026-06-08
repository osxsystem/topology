//! Integration tests for `gatekeeper check research` and the `design` sequence-lock.
//!
//! Mirrors the scratch-root / run helper style of `cli_adapt.rs`; no `assert_cmd`/`predicates`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Minimal framework root: a `skills/` marker so `framework_root()` resolves here.
fn scratch_root(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("topo_check_cli_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("skills")).unwrap();
    root
}

/// Run `gatekeeper <args>` from `cwd`. Returns (exit code, stdout).
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

// --- research gate ---

#[test]
fn research_gate_fails_when_no_research_note() {
    let root = scratch_root("res_miss");
    fs::create_dir_all(root.join("docs").join("research")).unwrap();

    let (code, out) = run(&root, &["check", "research", "--feature", "myslug"]);
    assert_eq!(code, 1, "should fail: no research note present");
    assert!(
        out.contains("FAIL"),
        "output should mention FAIL; got: {out}"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn research_gate_passes_when_research_note_exists() {
    let root = scratch_root("res_pass");
    let research_dir = root.join("docs").join("research");
    fs::create_dir_all(&research_dir).unwrap();
    fs::write(
        research_dir.join("2026-06-08-myslug.md"),
        "# Research\n\nSome findings.\n",
    )
    .unwrap();

    let (code, out) = run(&root, &["check", "research", "--feature", "myslug"]);
    assert_eq!(code, 0, "should pass with research note; out: {out}");
    assert!(
        out.contains("PASS"),
        "output should mention PASS; got: {out}"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn research_gate_missing_feature_exits_2() {
    let root = scratch_root("res_nofeat");
    fs::create_dir_all(root.join("docs").join("research")).unwrap();

    let (code, _) = run(&root, &["check", "research"]);
    assert_eq!(code, 2, "missing --feature should exit 2");
    let _ = fs::remove_dir_all(&root);
}

// --- design sequence-lock ---

#[test]
fn design_gate_fails_without_research_note_even_with_spec() {
    let root = scratch_root("des_lock");
    // Write a spec but NO research note.
    let specs_dir = root.join("docs").join("specs");
    fs::create_dir_all(&specs_dir).unwrap();
    fs::write(
        specs_dir.join("2026-06-08-myslug.md"),
        "# Spec\n\nReady to go.\n",
    )
    .unwrap();
    fs::create_dir_all(root.join("docs").join("research")).unwrap();

    let (code, out) = run(&root, &["check", "design", "--feature", "myslug"]);
    assert_eq!(
        code, 1,
        "design gate must fail when research note is absent (sequence-lock); out: {out}"
    );
    assert!(
        out.contains("research-first"),
        "output should mention research-first; got: {out}"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn design_gate_passes_after_both_research_and_spec_exist() {
    let root = scratch_root("des_pass");
    let research_dir = root.join("docs").join("research");
    let specs_dir = root.join("docs").join("specs");
    fs::create_dir_all(&research_dir).unwrap();
    fs::create_dir_all(&specs_dir).unwrap();

    fs::write(
        research_dir.join("2026-06-08-myslug.md"),
        "# Research\n\nFindings.\n",
    )
    .unwrap();
    fs::write(specs_dir.join("2026-06-08-myslug.md"), "# Spec\n\nReady.\n").unwrap();

    let (code, out) = run(&root, &["check", "design", "--feature", "myslug"]);
    assert_eq!(
        code, 0,
        "design gate should pass when both research and spec exist; out: {out}"
    );
    assert!(
        out.contains("PASS"),
        "output should mention PASS; got: {out}"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn design_gate_fails_without_spec_even_with_research() {
    let root = scratch_root("des_nospec");
    let research_dir = root.join("docs").join("research");
    fs::create_dir_all(&research_dir).unwrap();
    fs::create_dir_all(root.join("docs").join("specs")).unwrap();

    fs::write(
        research_dir.join("2026-06-08-myslug.md"),
        "# Research\n\nFindings.\n",
    )
    .unwrap();
    // No spec written.

    let (code, out) = run(&root, &["check", "design", "--feature", "myslug"]);
    assert_eq!(
        code, 1,
        "design gate should fail when spec is absent; out: {out}"
    );
    assert!(
        out.contains("FAIL"),
        "output should mention FAIL; got: {out}"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn design_gate_missing_feature_exits_2() {
    // A missing --feature is a usage error (exit 2, like the research gate), NOT a
    // research-first failure (exit 1): the empty slug must not fall into the lock branch.
    let root = scratch_root("des_nofeat");
    fs::create_dir_all(root.join("docs").join("research")).unwrap();

    let (code, _) = run(&root, &["check", "design"]);
    assert_eq!(
        code, 2,
        "missing --feature on the design gate should exit 2"
    );
    let _ = fs::remove_dir_all(&root);
}
