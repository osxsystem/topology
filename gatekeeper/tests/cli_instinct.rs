use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// A minimal framework root: a `skills/` marker (so `framework_root()` resolves here) and an
/// `instincts/` dir with one high + one medium seed.
fn scratch_root(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("topo_inst_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("skills")).unwrap();
    fs::create_dir_all(root.join("instincts")).unwrap();
    fs::write(
        root.join("instincts").join("evidence-over-assertion.md"),
        "---\nid: evidence-over-assertion\npriority: high\nsource: doc:ROADMAP\n---\nDone means a re-runnable command and its output, never a feeling.\n",
    )
    .unwrap();
    fs::write(
        root.join("instincts").join("surgical-changes-only.md"),
        "---\nid: surgical-changes-only\npriority: medium\nsource: doc:EXTENDING\n---\nChange only what the task needs; no drive-by refactors.\n",
    )
    .unwrap();
    root
}

/// Run `gatekeeper <args>` from `cwd`, feeding `stdin`. Returns (exit code, stdout).
fn run(cwd: &Path, args: &[&str], stdin: &[u8]) -> (i32, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_gatekeeper"))
        .current_dir(cwd)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(stdin).unwrap();
    let out = child.wait_with_output().unwrap();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
}

#[test]
fn list_enumerates_sorted_with_priority() {
    let root = scratch_root("list");
    let (code, out) = run(&root, &["instinct", "list"], b"");
    assert_eq!(code, 0);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(
        lines,
        vec![
            "evidence-over-assertion\thigh",
            "surgical-changes-only\tmedium",
        ],
        "high sorts before medium"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn render_claude_emits_header_and_id_lines() {
    let root = scratch_root("render");
    let (code, out) = run(&root, &["instinct", "render", "--harness", "claude"], b"");
    assert_eq!(code, 0);
    assert!(out.starts_with("Always-on instincts — how to reason here"));
    assert!(out.contains("  - [evidence-over-assertion] Done means a re-runnable command"));
    assert!(out.contains("  - [surgical-changes-only] Change only what the task needs"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn render_unsupported_harness_exits_2() {
    let root = scratch_root("harness");
    let (code, out) = run(&root, &["instinct", "render", "--harness", "cursor"], b"");
    assert_eq!(code, 2, "non-claude harness is a usage error in Phase 2");
    assert!(out.is_empty());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn render_budget_drops_lowest_priority_whole() {
    let root = scratch_root("budget");
    // The high body is 11 words; budget 11 keeps it and drops the medium whole.
    let (code, out) = run(&root, &["instinct", "render", "--budget", "11"], b"");
    assert_eq!(code, 0);
    assert!(out.contains("evidence-over-assertion"));
    assert!(
        !out.contains("surgical-changes-only"),
        "medium dropped under tight budget"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn list_on_malformed_file_exits_2() {
    let root = scratch_root("badlist");
    fs::write(
        root.join("instincts").join("bad.md"),
        "---\nid: bad\napplies: always\n---\nscoped is no longer a field\n",
    )
    .unwrap();
    let (code, _) = run(&root, &["instinct", "list"], b"");
    assert_eq!(code, 2, "unknown frontmatter field fails loud in list");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn activate_injects_instincts_between_skills_and_gate_warning() {
    let root = scratch_root("activate");
    // No skill-rules.json in the scratch root → no routed skills, but instincts still inject.
    let (code, out) = run(&root, &["activate"], b"please refactor the parser\n");
    assert_eq!(code, 0);
    let header = out
        .find("Always-on instincts —")
        .expect("instincts header present");
    let gate = out
        .find("You may not write production code")
        .expect("gate warning present");
    assert!(
        header < gate,
        "instincts must appear before the gate-warning line"
    );
    assert!(out.contains("  - [evidence-over-assertion]"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn activate_with_no_instincts_dir_does_not_break_turn() {
    let root = std::env::temp_dir().join(format!("topo_inst_noinst_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("skills")).unwrap(); // marker, but NO instincts/ dir
    let (code, out) = run(&root, &["activate"], b"hello\n");
    assert_eq!(code, 0, "missing instincts/ dir must not break the turn");
    assert!(
        !out.contains("Always-on instincts —"),
        "no section when there are no instincts"
    );
    assert!(out.contains("You may not write production code"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn activate_skips_malformed_file_and_still_exits_0() {
    let root = scratch_root("activate_soft");
    fs::write(
        root.join("instincts").join("broken.md"),
        "no frontmatter fence at all\n",
    )
    .unwrap();
    let (code, out) = run(&root, &["activate"], b"hi\n");
    assert_eq!(
        code, 0,
        "a malformed instinct is skipped, not fatal, at activate time"
    );
    assert!(
        out.contains("  - [evidence-over-assertion]"),
        "the good instincts still render"
    );
    let _ = fs::remove_dir_all(&root);
}
