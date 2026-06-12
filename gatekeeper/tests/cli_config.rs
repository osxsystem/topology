//! CLI integration tests for per-project config.toml (issue #29).
//!
//! Covers:
//! - `check finish`: runs config.test_command when no `-- cmd` given.
//! - `check finish`: explicit `-- cmd` overrides config.test_command.
//! - `check finish`: neither flag nor config → error mentioning config.toml.
//! - `check review`: uses config.base_branch on a master-only repo.
//! - `check review`: --base flag overrides config.base_branch.
//! - `gatekeeper adapt --harness claude`: generates config.toml on project install.
//! - `gatekeeper adapt --harness claude`: does NOT overwrite existing config.toml.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Minimal scratch root: `skills/` so `framework_root()` resolves here.
fn scratch_root(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("topo_cfg_cli_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("skills")).unwrap();
    root
}

/// Run gatekeeper from `cwd`. Returns (exit_code, stdout, stderr).
fn run3(cwd: &Path, args: &[&str]) -> (i32, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_gatekeeper"))
        .current_dir(cwd)
        .args(args)
        .output()
        .unwrap();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// Run gatekeeper with TOPOLOGY_ROOT overridden.
fn run3_with_root(cwd: &Path, fw: &Path, args: &[&str]) -> (i32, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_gatekeeper"))
        .current_dir(cwd)
        .env("TOPOLOGY_ROOT", fw)
        .args(args)
        .output()
        .unwrap();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

// ── finish gate + config.test_command ─────────────────────────────────────────

#[test]
fn finish_runs_config_test_command_when_no_cli_cmd() {
    // config.test_command = "true" (a shell builtin that exits 0).
    // No `-- cmd` on the CLI → config command is run → gate passes.
    let root = scratch_root("fin_cfg_pass");
    // Create .claude/topology/config.toml (self-is-framework layout uses docs/, but
    // we use TOPOLOGY_ROOT override to separate the two so artifacts_root → .claude/topology/).
    let fw = scratch_root("fin_cfg_pass_fw");
    fs::write(fw.join("AGENTS.md"), "").unwrap();
    let proj = std::env::temp_dir().join(format!("topo_cfg_proj_fin_pass_{}", std::process::id()));
    let _ = fs::remove_dir_all(&proj);
    fs::create_dir_all(&proj).unwrap();
    Command::new("git")
        .args(["-C", proj.to_str().unwrap(), "init", "-q", "-b", "main"])
        .status()
        .unwrap();

    let arts = proj.join(".claude").join("topology");
    fs::create_dir_all(&arts).unwrap();
    // "true" is a shell builtin that always exits 0.
    fs::write(arts.join("config.toml"), "test_command = \"true\"\n").unwrap();

    let (code, out, _) = run3_with_root(&proj, &fw, &["check", "finish"]);
    assert_eq!(
        code, 0,
        "should PASS when config.test_command exits 0; out: {out}"
    );
    assert!(out.contains("PASS"), "output should say PASS; got: {out}");

    let _ = fs::remove_dir_all(&proj);
    let _ = fs::remove_dir_all(&fw);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn finish_config_test_command_fail_exits_one() {
    // config.test_command = "false" (exits 1) → gate fails.
    let fw = scratch_root("fin_cfg_fail_fw");
    fs::write(fw.join("AGENTS.md"), "").unwrap();
    let proj = std::env::temp_dir().join(format!("topo_cfg_proj_fin_fail_{}", std::process::id()));
    let _ = fs::remove_dir_all(&proj);
    fs::create_dir_all(&proj).unwrap();
    Command::new("git")
        .args(["-C", proj.to_str().unwrap(), "init", "-q", "-b", "main"])
        .status()
        .unwrap();

    let arts = proj.join(".claude").join("topology");
    fs::create_dir_all(&arts).unwrap();
    fs::write(arts.join("config.toml"), "test_command = \"false\"\n").unwrap();

    let (code, out, _) = run3_with_root(&proj, &fw, &["check", "finish"]);
    assert_eq!(
        code, 1,
        "should FAIL when config.test_command exits non-zero; out: {out}"
    );
    assert!(out.contains("FAIL"), "output should say FAIL; got: {out}");

    let _ = fs::remove_dir_all(&proj);
    let _ = fs::remove_dir_all(&fw);
}

#[test]
fn finish_explicit_cli_cmd_overrides_config_test_command() {
    // config.test_command = "false" (would fail), but `-- true` on CLI wins → pass.
    let fw = scratch_root("fin_override_fw");
    fs::write(fw.join("AGENTS.md"), "").unwrap();
    let proj =
        std::env::temp_dir().join(format!("topo_cfg_proj_fin_override_{}", std::process::id()));
    let _ = fs::remove_dir_all(&proj);
    fs::create_dir_all(&proj).unwrap();
    Command::new("git")
        .args(["-C", proj.to_str().unwrap(), "init", "-q", "-b", "main"])
        .status()
        .unwrap();

    let arts = proj.join(".claude").join("topology");
    fs::create_dir_all(&arts).unwrap();
    fs::write(arts.join("config.toml"), "test_command = \"false\"\n").unwrap();

    // Explicit `-- true` should win over config's "false".
    let (code, out, _) = run3_with_root(&proj, &fw, &["check", "finish", "--", "true"]);
    assert_eq!(
        code, 0,
        "explicit -- cmd must override config.test_command; out: {out}"
    );
    assert!(out.contains("PASS"), "output should say PASS; got: {out}");

    let _ = fs::remove_dir_all(&proj);
    let _ = fs::remove_dir_all(&fw);
}

#[test]
fn finish_no_cmd_no_config_exits_2_with_config_hint() {
    // No `-- cmd`, no config.toml → exit 2, error mentions config.toml.
    let root = scratch_root("fin_nocmd");
    let (code, _out, err) = run3(&root, &["check", "finish"]);
    assert_eq!(code, 2, "should exit 2 (usage error); err: {err}");
    assert!(
        err.contains("config.toml"),
        "error message must mention config.toml; got: {err}"
    );
    let _ = fs::remove_dir_all(&root);
}

// ── review gate + config.base_branch ─────────────────────────────────────────

/// Build a minimal framework dir (skills/ + AGENTS.md + templates/, no .git).
fn scratch_fw(tag: &str) -> PathBuf {
    let fw = std::env::temp_dir().join(format!("topo_cfg_fw_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&fw);
    fs::create_dir_all(fw.join("skills")).unwrap();
    fs::create_dir_all(fw.join("templates")).unwrap();
    fs::write(fw.join("AGENTS.md"), "").unwrap();
    fs::write(
        fw.join("templates").join("CONTRACT.template.md"),
        "# Contract\nRoot: {{ARTIFACTS_ROOT}}\nCmd: {{GATEKEEPER_CMD}}\n{{BINARY_NOTE}}",
    )
    .unwrap();
    fw
}

/// Build a scratch project repo on `branch`. Returns (root, head_sha).
fn scratch_proj_on_branch(tag: &str, branch: &str) -> (PathBuf, String) {
    let proj = std::env::temp_dir().join(format!("topo_cfg_proj_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&proj);
    fs::create_dir_all(&proj).unwrap();
    Command::new("git")
        .args(["-C", proj.to_str().unwrap(), "init", "-q", "-b", branch])
        .status()
        .unwrap();
    Command::new("git")
        .args([
            "-C",
            proj.to_str().unwrap(),
            "config",
            "user.email",
            "t@t.t",
        ])
        .status()
        .unwrap();
    Command::new("git")
        .args(["-C", proj.to_str().unwrap(), "config", "user.name", "t"])
        .status()
        .unwrap();
    fs::write(proj.join("a.txt"), "one\n").unwrap();
    Command::new("git")
        .args(["-C", proj.to_str().unwrap(), "add", "."])
        .status()
        .unwrap();
    Command::new("git")
        .args(["-C", proj.to_str().unwrap(), "commit", "-q", "-m", "init"])
        .status()
        .unwrap();
    let out = Command::new("git")
        .args(["-C", proj.to_str().unwrap(), "rev-parse", "HEAD"])
        .output()
        .unwrap();
    let head = String::from_utf8(out.stdout).unwrap().trim().to_string();
    (proj, head)
}

/// Write a review artifact at `<root>/<reviews_sub>/<date>-<feature>.md`.
fn write_review_artifact(root: &Path, reviews_sub: &str, head: &str, base: &str, feature: &str) {
    let dir = root.join(reviews_sub);
    fs::create_dir_all(&dir).unwrap();
    let body = format!(
        "VERDICT: pass\nHEAD: {head}\nBASE: {base}\n\n# Review\n\n\
         ## Blocking findings\nNone.\n\n## Criteria checked\n\
         ### Spec/plan\n- crit — met\n### Standards\n- rule — met\n"
    );
    fs::write(dir.join(format!("2026-06-11-{feature}.md")), body).unwrap();
}

/// Commit all changes in a repo (helper for CLI tests that need a clean tree).
fn git_add_commit(proj: &Path, msg: &str) {
    Command::new("git")
        .args(["-C", proj.to_str().unwrap(), "add", "."])
        .status()
        .unwrap();
    Command::new("git")
        .args(["-C", proj.to_str().unwrap(), "commit", "-q", "-m", msg])
        .status()
        .unwrap();
}

#[test]
fn review_uses_config_base_branch_on_master_repo() {
    // A master-only repo with config.base_branch = "master" → gate passes without --base flag.
    let fw = scratch_fw("rv_cfg_base");
    let (proj, _) = scratch_proj_on_branch("rv_cfg_base", "master");

    // Place config.base_branch = "master" in the artifacts root, then commit it so the
    // worktree is clean (the review gate requires a clean tree outside reviews/).
    let arts = proj.join(".claude").join("topology");
    fs::create_dir_all(&arts).unwrap();
    fs::write(arts.join("config.toml"), "base_branch = \"master\"\n").unwrap();
    git_add_commit(&proj, "add config.toml");

    // Get HEAD after the config commit.
    let head_out = Command::new("git")
        .args(["-C", proj.to_str().unwrap(), "rev-parse", "HEAD"])
        .output()
        .unwrap();
    let head = String::from_utf8(head_out.stdout)
        .unwrap()
        .trim()
        .to_string();

    // Write review artifact under .claude/topology/reviews/ (untracked, which is fine).
    write_review_artifact(&proj, ".claude/topology/reviews", &head, &head, "feat");

    let (code, out, _) = run3_with_root(&proj, &fw, &["check", "review", "--feature", "feat"]);
    assert_eq!(
        code, 0,
        "gate should PASS with config.base_branch=master; out:\n{out}"
    );
    assert!(out.contains("PASS"), "output must say PASS; got:\n{out}");

    let _ = fs::remove_dir_all(&proj);
    let _ = fs::remove_dir_all(&fw);
}

#[test]
fn review_base_flag_overrides_config_base_branch() {
    // config.base_branch = "nonexistent", but --base main wins → gate passes.
    let fw = scratch_fw("rv_flag_wins");
    let (proj, _) = scratch_proj_on_branch("rv_flag_wins", "main");

    let arts = proj.join(".claude").join("topology");
    fs::create_dir_all(&arts).unwrap();
    // Deliberately wrong config: --base should override it.
    fs::write(arts.join("config.toml"), "base_branch = \"nonexistent\"\n").unwrap();
    git_add_commit(&proj, "add config.toml");

    // Get HEAD after the config commit.
    let head_out = Command::new("git")
        .args(["-C", proj.to_str().unwrap(), "rev-parse", "HEAD"])
        .output()
        .unwrap();
    let head = String::from_utf8(head_out.stdout)
        .unwrap()
        .trim()
        .to_string();

    write_review_artifact(&proj, ".claude/topology/reviews", &head, &head, "feat");

    let (code, out, _) = run3_with_root(
        &proj,
        &fw,
        &["check", "review", "--feature", "feat", "--base", "main"],
    );
    assert_eq!(
        code, 0,
        "--base flag must override config.base_branch; out:\n{out}"
    );
    assert!(out.contains("PASS"), "output must say PASS; got:\n{out}");

    let _ = fs::remove_dir_all(&proj);
    let _ = fs::remove_dir_all(&fw);
}

// ── adapt generates config.toml ───────────────────────────────────────────────

#[test]
fn adapt_claude_generates_config_toml_on_project_install() {
    // When read_root != write_root, adapt --harness claude should also write config.toml
    // under <project>/.claude/topology/config.toml.
    let fw = scratch_fw("adapt_gen_cfg");
    // write_root needs AGENTS.md for build_claude to succeed.
    fs::write(fw.join("AGENTS.md"), "# contract\n").unwrap();
    // Create minimal hooks so build_claude doesn't fail.
    fs::create_dir_all(fw.join("hooks")).unwrap();
    fs::write(fw.join("hooks/skill-activation.sh"), "#!/bin/sh\n").unwrap();
    fs::write(fw.join("hooks/security-scan.sh"), "#!/bin/sh\n").unwrap();

    let proj = std::env::temp_dir().join(format!("topo_cfg_proj_adapt_{}", std::process::id()));
    let _ = fs::remove_dir_all(&proj);
    fs::create_dir_all(&proj).unwrap();
    Command::new("git")
        .args(["-C", proj.to_str().unwrap(), "init", "-q", "-b", "main"])
        .status()
        .unwrap();

    let (code, out, err) = run3_with_root(&proj, &fw, &["adapt", "--harness", "claude"]);
    assert_eq!(code, 0, "adapt should succeed; err: {err}; out: {out}");

    let config_path = proj.join(".claude").join("topology").join("config.toml");
    assert!(
        config_path.exists(),
        "config.toml must be generated at {}",
        config_path.display()
    );
    let contents = fs::read_to_string(&config_path).unwrap();
    assert!(
        contents.contains("base_branch"),
        "config.toml must contain base_branch; got: {contents}"
    );
    assert!(
        contents.contains("test_command"),
        "config.toml must mention test_command (even if commented); got: {contents}"
    );

    let _ = fs::remove_dir_all(&proj);
    let _ = fs::remove_dir_all(&fw);
}

#[test]
fn adapt_does_not_overwrite_existing_config_toml() {
    // An existing config.toml must not be clobbered by adapt.
    let fw = scratch_fw("adapt_no_overwrite");
    fs::write(fw.join("AGENTS.md"), "# contract\n").unwrap();
    fs::create_dir_all(fw.join("hooks")).unwrap();
    fs::write(fw.join("hooks/skill-activation.sh"), "#!/bin/sh\n").unwrap();
    fs::write(fw.join("hooks/security-scan.sh"), "#!/bin/sh\n").unwrap();

    let proj = std::env::temp_dir().join(format!("topo_cfg_proj_no_ow_{}", std::process::id()));
    let _ = fs::remove_dir_all(&proj);
    fs::create_dir_all(&proj).unwrap();
    Command::new("git")
        .args(["-C", proj.to_str().unwrap(), "init", "-q", "-b", "main"])
        .status()
        .unwrap();

    // Pre-write a config.toml with a custom sentinel.
    let arts = proj.join(".claude").join("topology");
    fs::create_dir_all(&arts).unwrap();
    let original = "base_branch = \"sentinel-value\"\n";
    fs::write(arts.join("config.toml"), original).unwrap();

    let (code, _, err) = run3_with_root(&proj, &fw, &["adapt", "--harness", "claude"]);
    assert_eq!(code, 0, "adapt should succeed; err: {err}");

    let after = fs::read_to_string(arts.join("config.toml")).unwrap();
    assert_eq!(
        after, original,
        "existing config.toml must not be overwritten"
    );

    let _ = fs::remove_dir_all(&proj);
    let _ = fs::remove_dir_all(&fw);
}
