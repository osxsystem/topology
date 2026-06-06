use std::fs;
use std::path::Path;
use std::process::Command;

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

#[test]
fn review_gate_runs_from_nested_subdir() {
    let root = std::env::temp_dir().join(format!("topo_cli_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("skills")).unwrap(); // marks the framework root
    git(&root, &["init", "-q", "-b", "main"]);
    git(&root, &["config", "user.email", "t@t.t"]);
    git(&root, &["config", "user.name", "t"]);
    fs::write(root.join("a.txt"), "one\n").unwrap();
    git(&root, &["add", "."]);
    git(&root, &["commit", "-q", "-m", "init"]);

    let out = Command::new("git")
        .arg("-C")
        .arg(&root)
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    let head = String::from_utf8(out.stdout).unwrap().trim().to_string();

    let dir = root.join("docs").join("reviews");
    fs::create_dir_all(&dir).unwrap();
    let body = format!(
        "VERDICT: pass\nHEAD: {head}\nBASE: {head}\n\n# Review\n\n## Blocking findings\nNone.\n\n## Criteria checked\n### Spec/plan\n- crit — met\n### Standards\n- rule — met\n"
    );
    fs::write(dir.join("2026-06-05-code-review-gate.md"), body).unwrap();

    let nested = root.join("src").join("deep").join("nested");
    fs::create_dir_all(&nested).unwrap();
    let status = Command::new(env!("CARGO_BIN_EXE_gatekeeper"))
        .current_dir(&nested)
        .args(["check", "review", "--feature", "code-review-gate"])
        .status()
        .unwrap();
    assert_eq!(
        status.code(),
        Some(0),
        "gate should pass from a nested subdir"
    );
    let _ = fs::remove_dir_all(&root);
}
