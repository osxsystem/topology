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
    fs::create_dir_all(root.join("skills")).unwrap();
    fs::write(root.join("AGENTS.md"), "").unwrap(); // marks the framework root
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

// ── AC-3: Review gate runs against the *project* repo (not the framework) ──

/// Run gatekeeper from `cwd` with TOPOLOGY_ROOT set to `fw`. Returns (exit_code, stdout).
fn run_review(cwd: &Path, fw: &Path, feature: &str) -> (i32, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_gatekeeper"))
        .current_dir(cwd)
        .env("TOPOLOGY_ROOT", fw)
        .args(["check", "review", "--feature", feature])
        .output()
        .unwrap();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
}

/// Build a scratch framework dir (skills/ + AGENTS.md, no .git).
fn scratch_fw(tag: &str) -> std::path::PathBuf {
    let fw = std::env::temp_dir().join(format!("topo_rv_fw_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&fw);
    fs::create_dir_all(fw.join("skills")).unwrap();
    fs::write(fw.join("AGENTS.md"), "").unwrap();
    fw
}

/// Build a scratch project repo: git init + one commit. Returns (root, head_sha).
fn scratch_proj(tag: &str) -> (std::path::PathBuf, String) {
    let proj = std::env::temp_dir().join(format!("topo_rv_proj_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&proj);
    fs::create_dir_all(&proj).unwrap();
    git(&proj, &["init", "-q", "-b", "main"]);
    git(&proj, &["config", "user.email", "t@t.t"]);
    git(&proj, &["config", "user.name", "t"]);
    fs::write(proj.join("a.txt"), "one\n").unwrap();
    git(&proj, &["add", "."]);
    git(&proj, &["commit", "-q", "-m", "init"]);
    let out = Command::new("git")
        .args(["-C", proj.to_str().unwrap(), "rev-parse", "HEAD"])
        .output()
        .unwrap();
    let head = String::from_utf8(out.stdout).unwrap().trim().to_string();
    (proj, head)
}

/// Write a review artifact at `<root>/<reviews_sub>/<date>-<feature>.md`.
fn write_artifact(root: &std::path::Path, reviews_sub: &str, head: &str, base: &str, feature: &str, pass: bool) {
    let dir = root.join(reviews_sub);
    fs::create_dir_all(&dir).unwrap();
    let (v, blk) = if pass { ("pass", "None.") } else { ("fail", "- a.txt:1 — wrong") };
    let body = format!(
        "VERDICT: {v}\nHEAD: {head}\nBASE: {base}\n\n# Review\n\n## Blocking findings\n{blk}\n\n## Criteria checked\n### Spec/plan\n- crit — met\n### Standards\n- rule — met\n"
    );
    fs::write(dir.join(format!("2026-06-10-{feature}.md")), body).unwrap();
}

#[test]
fn external_project_review_passes_with_artifact_in_claude_topology() {
    // AC-3: well-formed artifact at .claude/topology/reviews/<date>-x.md naming HEAD passes.
    let fw = scratch_fw("rv_pass");
    let (proj, head) = scratch_proj("rv_pass");

    write_artifact(&proj, ".claude/topology/reviews", &head, &head, "installer-v2", true);

    let (code, out) = run_review(&proj, &fw, "installer-v2");
    assert_eq!(code, 0, "should PASS; out:\n{out}");
    assert!(out.contains("PASS"), "output must say PASS; got:\n{out}");

    let _ = fs::remove_dir_all(&fw);
    let _ = fs::remove_dir_all(&proj);
}

#[test]
fn external_project_review_fails_dirty_non_reviews_file() {
    // AC-3: a dirty (non-reviews) file in the *project* repo fails the gate.
    let fw = scratch_fw("rv_dirty");
    let (proj, head) = scratch_proj("rv_dirty");

    write_artifact(&proj, ".claude/topology/reviews", &head, &head, "installer-v2", true);
    // Modify a tracked file in the project repo → dirty tree.
    fs::write(proj.join("a.txt"), "changed\n").unwrap();

    let (code, _out) = run_review(&proj, &fw, "installer-v2");
    assert_eq!(code, 1, "dirty project tree must fail the gate");

    let _ = fs::remove_dir_all(&fw);
    let _ = fs::remove_dir_all(&proj);
}
