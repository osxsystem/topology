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
///
/// TOPOLOGY_ROOT is pinned to the canonicalized `cwd` so that after Phase 11
/// binary-adjacent resolution does not silently point at the actual topology repo
/// instead of the scratch root. Canonicalization resolves macOS /var → /private/var.
fn run(cwd: &Path, args: &[&str]) -> (i32, String) {
    let canonical_cwd = fs::canonicalize(cwd).unwrap_or_else(|_| cwd.to_path_buf());
    let out = Command::new(env!("CARGO_BIN_EXE_gatekeeper"))
        .current_dir(cwd)
        .args(args)
        .env("TOPOLOGY_ROOT", &canonical_cwd)
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
    fs::create_dir_all(root.join("skills")).unwrap(); // no AGENTS.md — adapt must fail
    fs::create_dir_all(root.join("instincts")).unwrap();
    // Pin TOPOLOGY_ROOT to the scratch root: after the Phase 11 rewrite, binary-adjacent
    // resolves to the actual repo (which has AGENTS.md), so without the pin the test would
    // silently pass; we must force the root to the no-AGENTS.md scratch dir.
    let out = Command::new(env!("CARGO_BIN_EXE_gatekeeper"))
        .current_dir(&root)
        .args(["adapt", "--harness", "codex"])
        .env("TOPOLOGY_ROOT", &root)
        .output()
        .unwrap();
    let code = out.status.code().unwrap_or(-1);
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

// ── AC-4: adapt writes to the project root; hook paths point at the framework root ──

#[test]
fn adapt_writes_to_project_not_framework() {
    // Run adapt from a scratch project dir (git repo) with TOPOLOGY_ROOT → scratch framework.
    // Asserts: .claude/settings.json written in project, nothing written in framework,
    // hook paths in settings.json point at the framework dir.
    let fw = std::env::temp_dir().join(format!("topo_adapt_fw_{}", std::process::id()));
    let proj = std::env::temp_dir().join(format!("topo_adapt_proj_{}", std::process::id()));
    let _ = fs::remove_dir_all(&fw);
    let _ = fs::remove_dir_all(&proj);

    // Framework: skills/ + AGENTS.md + hooks/ (marks it as topology root).
    fs::create_dir_all(fw.join("skills").join("s")).unwrap();
    fs::create_dir_all(fw.join("instincts")).unwrap();
    fs::create_dir_all(fw.join("hooks")).unwrap();
    fs::write(fw.join("AGENTS.md"), "# Topology\n\nGates.\n").unwrap();
    fs::write(
        fw.join("skills").join("s").join("SKILL.md"),
        "---\nname: s\ndescription: A skill.\n---\nBody.\n",
    )
    .unwrap();

    // Project: a git repo (so project_root() resolves here).
    fs::create_dir_all(&proj).unwrap();
    std::process::Command::new("git")
        .args(["-C", proj.to_str().unwrap(), "init", "-q", "-b", "main"])
        .status()
        .unwrap();

    let out = Command::new(env!("CARGO_BIN_EXE_gatekeeper"))
        .current_dir(&proj)
        .env("TOPOLOGY_ROOT", &fw)
        .args(["adapt", "--harness", "claude"])
        .output()
        .unwrap();
    let code = out.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    assert_eq!(code, 0, "adapt should succeed; out:\n{stdout}");

    // .claude/settings.json must exist in the project.
    let settings_path = proj.join(".claude").join("settings.json");
    assert!(
        settings_path.exists(),
        ".claude/settings.json must be written in the project"
    );

    // Hook paths must reference the framework dir (not the project).
    let settings = fs::read_to_string(&settings_path).unwrap();
    assert!(
        settings.contains(fw.to_str().unwrap()),
        "hook paths must reference the framework root; settings:\n{settings}"
    );

    // Nothing must have been written under the framework root.
    let fw_claude = fw.join(".claude");
    assert!(
        !fw_claude.exists(),
        ".claude/ must not be created under the framework root"
    );

    // adapt --check passes immediately after.
    let check_out = Command::new(env!("CARGO_BIN_EXE_gatekeeper"))
        .current_dir(&proj)
        .env("TOPOLOGY_ROOT", &fw)
        .args(["adapt", "--harness", "claude", "--check"])
        .output()
        .unwrap();
    assert_eq!(
        check_out.status.code().unwrap_or(-1),
        0,
        "adapt --check must pass after a successful write"
    );

    let _ = fs::remove_dir_all(&fw);
    let _ = fs::remove_dir_all(&proj);
}

// ── Phase 9 integration tests (AC-1 through AC-8) — #[ignore] until task 4/5 implements them ──

/// Minimal CONTRACT template for scratch framework roots (all three placeholders present).
const MINI_CONTRACT_TEMPLATE: &str = "\
# Topology Agent

Artifacts: {{ARTIFACTS_ROOT}}

Cmd: {{GATEKEEPER_CMD}} check design --feature <slug>

**First session in a new project.** If the project documentation above this import line is a bare \
Topology stub, your first gate is: analyze the codebase, write project docs, then proceed.

{{BINARY_NOTE}}\
";

/// Build a scratch framework root with templates/ included, for Phase 9 project-install tests.
fn scratch_fw_with_template(tag: &str) -> PathBuf {
    let fw = std::env::temp_dir().join(format!("topo_p9_fw_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&fw);
    fs::create_dir_all(fw.join("skills").join("s")).unwrap();
    fs::create_dir_all(fw.join("instincts")).unwrap();
    fs::create_dir_all(fw.join("hooks")).unwrap();
    fs::create_dir_all(fw.join("templates")).unwrap();
    fs::write(fw.join("AGENTS.md"), "# Topology\n\nGates.\n").unwrap();
    fs::write(
        fw.join("skills").join("s").join("SKILL.md"),
        "---\nname: s\ndescription: A skill.\n---\nBody.\n",
    )
    .unwrap();
    fs::write(
        fw.join("instincts").join("gates-not-rules.md"),
        "---\nid: gates-not-rules\npriority: high\n---\nGates not rules.\n",
    )
    .unwrap();
    fs::write(
        fw.join("templates").join("CONTRACT.template.md"),
        MINI_CONTRACT_TEMPLATE,
    )
    .unwrap();
    fw
}

/// Build a scratch project root (git-initialised, no pre-existing Claude files).
fn scratch_proj(tag: &str) -> PathBuf {
    let proj = std::env::temp_dir().join(format!("topo_p9_proj_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&proj);
    fs::create_dir_all(&proj).unwrap();
    std::process::Command::new("git")
        .args(["-C", proj.to_str().unwrap(), "init", "-q", "-b", "main"])
        .status()
        .unwrap();
    proj
}

/// Run `gatekeeper <args>` in `proj` with TOPOLOGY_ROOT pinned to `fw`.
fn run_proj(fw: &Path, proj: &Path, args: &[&str]) -> (i32, String, String) {
    let canonical_fw = fs::canonicalize(fw).unwrap_or_else(|_| fw.to_path_buf());
    let out = Command::new(env!("CARGO_BIN_EXE_gatekeeper"))
        .current_dir(proj)
        .args(args)
        .env("TOPOLOGY_ROOT", &canonical_fw)
        .output()
        .unwrap();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// AC-1: pre-existing CLAUDE.md is not clobbered; only the import line is appended; second run is no-op.
#[test]
#[ignore = "task 5 implements project-install path"]
fn ac1_append_only_claude_md() {
    let fw = scratch_fw_with_template("ac1");
    let proj = scratch_proj("ac1");

    let prior = "# My Project\n\nSome user content.\n";
    fs::write(proj.join("CLAUDE.md"), prior).unwrap();

    let (code, stdout, stderr) = run_proj(&fw, &proj, &["adapt", "--harness", "claude"]);
    assert_eq!(code, 0, "adapt should succeed; stderr:\n{stderr}");

    let after = fs::read_to_string(proj.join("CLAUDE.md")).unwrap();
    assert!(
        after.starts_with("# My Project\n"),
        "prior bytes must be preserved; CLAUDE.md:\n{after}"
    );
    assert!(
        after.contains("Some user content."),
        "prior content must survive"
    );
    assert!(
        after.contains("@.topology/CONTRACT.md"),
        "import line must be appended; stdout:\n{stdout}"
    );

    // Second run: no-op.
    let (code2, _stdout2, stderr2) = run_proj(&fw, &proj, &["adapt", "--harness", "claude"]);
    assert_eq!(code2, 0, "second run must succeed; stderr:\n{stderr2}");
    let after2 = fs::read_to_string(proj.join("CLAUDE.md")).unwrap();
    assert_eq!(after, after2, "second run must be a no-op");

    // --check must exit 0 after the write.
    let (check_code, _, check_err) =
        run_proj(&fw, &proj, &["adapt", "--harness", "claude", "--check"]);
    assert_eq!(check_code, 0, "--check must exit 0 after write; stderr:\n{check_err}");

    let _ = fs::remove_dir_all(&fw);
    let _ = fs::remove_dir_all(&proj);
}

/// AC-2: CLAUDE.md created if missing, containing only the import line.
#[test]
#[ignore = "task 5 implements project-install path"]
fn ac2_create_if_missing_claude_md() {
    let fw = scratch_fw_with_template("ac2");
    let proj = scratch_proj("ac2");

    assert!(
        !proj.join("CLAUDE.md").exists(),
        "CLAUDE.md must not exist before adapt"
    );

    let (code, _stdout, stderr) = run_proj(&fw, &proj, &["adapt", "--harness", "claude"]);
    assert_eq!(code, 0, "adapt should succeed; stderr:\n{stderr}");

    let contents = fs::read_to_string(proj.join("CLAUDE.md")).unwrap();
    assert_eq!(
        contents, "@.topology/CONTRACT.md\n",
        "CLAUDE.md must contain only the import line; contents:\n{contents}"
    );

    let _ = fs::remove_dir_all(&fw);
    let _ = fs::remove_dir_all(&proj);
}

/// AC-3: codex AGENTS.md managed block round-trip.
/// - Edit inside block → re-run restores it.
/// - Edit outside block → preserved.
/// - --check flags an out-of-date block.
#[test]
#[ignore = "task 5 implements project-install path"]
fn ac3_codex_managed_block_round_trip() {
    let fw = scratch_fw_with_template("ac3");
    let proj = scratch_proj("ac3");

    let outside = "# My AGENTS\n\nUser-written prose that must survive.\n";
    fs::write(proj.join("AGENTS.md"), outside).unwrap();

    // First run: managed block appended.
    let (code, _stdout, stderr) = run_proj(&fw, &proj, &["adapt", "--harness", "codex"]);
    assert_eq!(code, 0, "first adapt must succeed; stderr:\n{stderr}");

    let after1 = fs::read_to_string(proj.join("AGENTS.md")).unwrap();
    assert!(
        after1.contains("<!-- BEGIN TOPOLOGY MANAGED BLOCK -->"),
        "managed block must be present after first adapt"
    );
    assert!(
        after1.contains("User-written prose that must survive."),
        "user content outside block must survive"
    );

    // Tamper inside the block: --check should report drift.
    let tampered = after1.replace(".topology/CONTRACT.md", "WRONG");
    fs::write(proj.join("AGENTS.md"), &tampered).unwrap();
    let (check_code, _out, _err) =
        run_proj(&fw, &proj, &["adapt", "--harness", "codex", "--check"]);
    assert_eq!(check_code, 1, "--check must report drift (exit 1) on tampered block");

    // Re-run: block restored, outside content preserved.
    let (code2, _stdout2, stderr2) = run_proj(&fw, &proj, &["adapt", "--harness", "codex"]);
    assert_eq!(code2, 0, "re-run after tamper must succeed; stderr:\n{stderr2}");
    let after2 = fs::read_to_string(proj.join("AGENTS.md")).unwrap();
    assert!(
        after2.contains("User-written prose that must survive."),
        "user content outside block must still survive after re-run"
    );
    assert!(
        !after2.contains("WRONG"),
        "tampered body must be replaced"
    );

    // Edit outside the block: user prose change preserved through adapt.
    let with_extra = after2.replace("User-written prose", "UPDATED user prose");
    fs::write(proj.join("AGENTS.md"), &with_extra).unwrap();
    let (code3, _, _) = run_proj(&fw, &proj, &["adapt", "--harness", "codex"]);
    assert_eq!(code3, 0, "run with outside-block change must succeed");
    let after3 = fs::read_to_string(proj.join("AGENTS.md")).unwrap();
    assert!(
        after3.contains("UPDATED user prose"),
        "user change outside block must be preserved through re-run"
    );

    let _ = fs::remove_dir_all(&fw);
    let _ = fs::remove_dir_all(&proj);
}

/// AC-4: pre-existing settings.json user key (`model`) survives after adapt.
#[test]
#[ignore = "task 4 implements settings merge"]
fn ac4_settings_no_clobber() {
    let fw = scratch_fw_with_template("ac4");
    let proj = scratch_proj("ac4");

    fs::create_dir_all(proj.join(".claude")).unwrap();
    fs::write(
        proj.join(".claude").join("settings.json"),
        "{\n  \"model\": \"claude-opus-4-5\"\n}\n",
    )
    .unwrap();

    let (code, _stdout, stderr) = run_proj(&fw, &proj, &["adapt", "--harness", "claude"]);
    assert_eq!(code, 0, "adapt must succeed; stderr:\n{stderr}");

    let settings_str = fs::read_to_string(proj.join(".claude").join("settings.json")).unwrap();
    let settings: serde_json::Value = serde_json::from_str(&settings_str).unwrap();
    assert_eq!(
        settings["model"], "claude-opus-4-5",
        "user model key must survive; settings:\n{settings_str}"
    );
    assert!(
        settings["hooks"].is_object(),
        "hooks must be present after adapt"
    );
    assert!(
        settings["env"]["GATEKEEPER_BIN"].is_string(),
        "env.GATEKEEPER_BIN must be present"
    );

    // --check must not report the user model key as drift.
    let (check_code, _, check_err) =
        run_proj(&fw, &proj, &["adapt", "--harness", "claude", "--check"]);
    assert_eq!(
        check_code, 0,
        "--check must not report user model key as drift; stderr:\n{check_err}"
    );

    let _ = fs::remove_dir_all(&fw);
    let _ = fs::remove_dir_all(&proj);
}

/// AC-5: env.GATEKEEPER_BIN equals `<framework>/bin/gatekeeper`.
#[test]
#[ignore = "task 4 implements settings merge"]
fn ac5_gatekeeper_bin_value() {
    let fw = scratch_fw_with_template("ac5");
    let proj = scratch_proj("ac5");

    let (code, _stdout, stderr) = run_proj(&fw, &proj, &["adapt", "--harness", "claude"]);
    assert_eq!(code, 0, "adapt must succeed; stderr:\n{stderr}");

    let settings_str = fs::read_to_string(proj.join(".claude").join("settings.json")).unwrap();
    let settings: serde_json::Value = serde_json::from_str(&settings_str).unwrap();
    let bin = settings["env"]["GATEKEEPER_BIN"]
        .as_str()
        .expect("GATEKEEPER_BIN must be a string");

    let canonical_fw = fs::canonicalize(&fw).unwrap_or(fw.clone());
    let expected = canonical_fw.join("bin").join("gatekeeper");
    assert_eq!(
        bin,
        expected.to_str().unwrap(),
        "GATEKEEPER_BIN must equal <framework>/bin/gatekeeper"
    );

    let _ = fs::remove_dir_all(&fw);
    let _ = fs::remove_dir_all(&proj);
}

/// AC-6: `.claude/topology/{research,specs,plans,verify,reviews}` dirs exist after one run.
#[test]
#[ignore = "task 5 implements scaffold"]
fn ac6_scaffold_dirs_exist() {
    let fw = scratch_fw_with_template("ac6");
    let proj = scratch_proj("ac6");

    let (code, _stdout, stderr) = run_proj(&fw, &proj, &["adapt", "--harness", "claude"]);
    assert_eq!(code, 0, "adapt must succeed; stderr:\n{stderr}");

    for subdir in &["research", "specs", "plans", "verify", "reviews"] {
        let p = proj.join(".claude").join("topology").join(subdir);
        assert!(p.exists(), ".claude/topology/{subdir} must exist after adapt");
    }

    let _ = fs::remove_dir_all(&fw);
    let _ = fs::remove_dir_all(&proj);
}

/// AC-7: rendered project CONTRACT.md contains the bare-stub first-session instruction.
#[test]
#[ignore = "task 5 implements contract write; task 6 adds the instruction to template"]
fn ac7_contract_contains_first_session_instruction() {
    let fw = scratch_fw_with_template("ac7");
    let proj = scratch_proj("ac7");

    let (code, _stdout, stderr) = run_proj(&fw, &proj, &["adapt", "--harness", "claude"]);
    assert_eq!(code, 0, "adapt must succeed; stderr:\n{stderr}");

    let contract = fs::read_to_string(proj.join(".topology").join("CONTRACT.md"))
        .expect(".topology/CONTRACT.md must exist after adapt");

    assert!(
        contract.contains("First session in a new project") || contract.contains("bare"),
        "CONTRACT.md must contain the first-session instruction; contract:\n{contract}"
    );

    let _ = fs::remove_dir_all(&fw);
    let _ = fs::remove_dir_all(&proj);
}

/// AC-8: malformed managed block (begin, no end) → exit 2 naming the file.
#[test]
#[ignore = "task 5 implements project-install path"]
fn ac8_malformed_managed_block_exits_2() {
    let fw = scratch_fw_with_template("ac8");
    let proj = scratch_proj("ac8");

    // Write a malformed AGENTS.md: begin marker with no end.
    fs::write(
        proj.join("AGENTS.md"),
        "# AGENTS\n\n<!-- BEGIN TOPOLOGY MANAGED BLOCK -->\nNo end marker here.\n",
    )
    .unwrap();

    let (code, _stdout, stderr) = run_proj(&fw, &proj, &["adapt", "--harness", "codex"]);
    assert_eq!(
        code, 2,
        "malformed managed block must exit 2; stderr:\n{stderr}"
    );
    assert!(
        !stderr.is_empty(),
        "stderr must name the problem; stderr:\n{stderr}"
    );

    let _ = fs::remove_dir_all(&fw);
    let _ = fs::remove_dir_all(&proj);
}
