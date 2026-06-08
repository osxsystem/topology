//! Integration tests for `gatekeeper adapt`: run the compiled binary over a scratch framework root
//! and assert the generated files + exit codes. JSON/TOML validity is proven against the real
//! `codex`/JSON tools in docs/verify/2026-06-08-cross-harness-adapters.md; here we assert structure.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// A minimal framework root: a `skills/` marker (so `framework_root()` resolves here), one skill, one
/// instinct, and the `AGENTS.md` contract.
fn scratch_root(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("topo_adapt_cli_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("skills").join("brainstorm-design")).unwrap();
    fs::create_dir_all(root.join("instincts")).unwrap();
    fs::write(
        root.join("AGENTS.md"),
        "# Topology Agent\n\nGate sequence: design then plan then tdd.\n",
    )
    .unwrap();
    fs::write(
        root.join("skills")
            .join("brainstorm-design")
            .join("SKILL.md"),
        "---\nname: brainstorm-design\ndescription: Turn an idea into a design. Use when starting a feature.\n---\n# Brainstorm\n\nNo code before a design doc.\n",
    )
    .unwrap();
    fs::write(
        root.join("instincts").join("gates-not-rules.md"),
        "---\nid: gates-not-rules\npriority: high\n---\nPhrase a commitment as trigger then check then act.\n",
    )
    .unwrap();
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

#[test]
fn codex_writes_project_safe_config() {
    let root = scratch_root("codex");
    let (code, out) = run(&root, &["adapt", "--harness", "codex"]);
    assert_eq!(code, 0);
    assert!(out.contains("wrote .codex/config.toml"));
    let cfg = fs::read_to_string(root.join(".codex/config.toml")).unwrap();
    assert!(cfg.contains("project_doc_max_bytes = 1048576"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn cursor_instincts_always_skill_agent_requested() {
    let root = scratch_root("cursor");
    let (code, _) = run(&root, &["adapt", "--harness", "cursor"]);
    assert_eq!(code, 0);
    let inst = fs::read_to_string(root.join(".cursor/rules/instincts.mdc")).unwrap();
    assert!(inst.contains("alwaysApply: true"));
    assert!(inst.contains("gates-not-rules"));
    let skill = fs::read_to_string(root.join(".cursor/rules/skill-brainstorm-design.mdc")).unwrap();
    assert!(skill.contains("alwaysApply: false"));
    assert!(skill.contains("description:"));
    assert!(
        !skill.contains("globs:"),
        "Agent-Requested rule omits globs"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn opencode_writes_schema_and_verbatim_skill() {
    let root = scratch_root("opencode");
    let (code, _) = run(&root, &["adapt", "--harness", "opencode"]);
    assert_eq!(code, 0);
    let cfg = fs::read_to_string(root.join("opencode.json")).unwrap();
    assert!(cfg.contains("https://opencode.ai/config.json"));
    assert!(cfg.contains("AGENTS.md"));
    let src = fs::read_to_string(root.join("skills/brainstorm-design/SKILL.md")).unwrap();
    let copied =
        fs::read_to_string(root.join(".opencode/skills/brainstorm-design/SKILL.md")).unwrap();
    assert_eq!(copied, src, "skill copied verbatim");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn claude_writes_hook_settings() {
    let root = scratch_root("claude");
    let (code, _) = run(&root, &["adapt", "--harness", "claude"]);
    assert_eq!(code, 0);
    let s = fs::read_to_string(root.join(".claude/settings.json")).unwrap();
    assert!(s.contains("UserPromptSubmit"));
    assert!(s.contains("PreToolUse"));
    assert!(s.contains("security-scan.sh"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn unknown_harness_exits_2() {
    let root = scratch_root("unknown");
    let (code, _) = run(&root, &["adapt", "--harness", "frobnicate"]);
    assert_eq!(code, 2);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn missing_harness_flag_exits_2() {
    let root = scratch_root("noflag");
    let (code, _) = run(&root, &["adapt"]);
    assert_eq!(code, 2);
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn missing_agents_md_exits_2() {
    let root = std::env::temp_dir().join(format!("topo_adapt_cli_noagents_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("skills")).unwrap(); // framework_root marker, but no AGENTS.md
    fs::create_dir_all(root.join("instincts")).unwrap();
    let (code, _) = run(&root, &["adapt", "--harness", "codex"]);
    assert_eq!(code, 2, "missing AGENTS.md is a hard error");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn check_mode_is_idempotent_then_detects_drift() {
    let root = scratch_root("check");
    assert_eq!(
        run(&root, &["adapt", "--harness", "opencode"]).0,
        0,
        "write"
    );
    assert_eq!(
        run(&root, &["adapt", "--harness", "opencode", "--check"]).0,
        0,
        "re-check is clean (idempotent)"
    );
    fs::write(root.join("opencode.json"), "{}\n").unwrap();
    assert_eq!(
        run(&root, &["adapt", "--harness", "opencode", "--check"]).0,
        1,
        "a mutated output is reported as drift"
    );
    let _ = fs::remove_dir_all(&root);
}
