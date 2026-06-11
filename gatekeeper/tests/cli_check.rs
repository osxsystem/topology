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

// --- version flag ---

#[test]
fn version_flag_long_exits_0_and_prints_version() {
    let root = scratch_root("ver_long");
    let (code, out) = run(&root, &["--version"]);
    assert_eq!(code, 0, "--version should exit 0; out: {out}");
    assert!(
        out.starts_with("gatekeeper "),
        "--version output should start with 'gatekeeper '; got: {out}"
    );
    assert!(
        out.contains("(rules schema v"),
        "--version output should contain '(rules schema v'; got: {out}"
    );
    assert!(
        out.contains(env!("CARGO_PKG_VERSION")),
        "--version output should contain crate version {}; got: {out}",
        env!("CARGO_PKG_VERSION")
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn version_flag_short_exits_0_and_prints_version() {
    let root = scratch_root("ver_short");
    let (code, out) = run(&root, &["-V"]);
    assert_eq!(code, 0, "-V should exit 0; out: {out}");
    assert!(
        out.starts_with("gatekeeper "),
        "-V output should start with 'gatekeeper '; got: {out}"
    );
    assert!(
        out.contains("(rules schema v"),
        "-V output should contain '(rules schema v'; got: {out}"
    );
    assert!(
        out.contains(env!("CARGO_PKG_VERSION")),
        "-V output should contain crate version {}; got: {out}",
        env!("CARGO_PKG_VERSION")
    );
    let _ = fs::remove_dir_all(&root);
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
    fs::write(
        specs_dir.join("2026-06-08-myslug.md"),
        "# Spec\n\n- **Status:** approved\n\nReady.\n",
    )
    .unwrap();

    let (code, out) = run(&root, &["check", "design", "--feature", "myslug"]);
    assert_eq!(
        code, 0,
        "design gate should pass when both research and spec exist (with approval marker); out: {out}"
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

// --- design gate: approval marker ---
//
// Issue #25: the gate must require an explicit `Status: approved` marker in the spec file.
// Tests cover:
//   - spec missing → existing FAIL (sequence check still takes precedence when research absent)
//   - spec present without any Status marker → new "not approved" FAIL
//   - spec present with `- **Status:** draft` (marker present but not approved) → FAIL
//   - spec with plain `Status: approved` → PASS
//   - spec with `**Status:** approved` → PASS
//   - spec with `- **Status:** approved` (house style) → PASS
//   - spec with `Status:   Approved` (flexible whitespace, mixed case) → PASS
//   - research-first sequence lock still takes precedence (spec w/ marker but no research → FAIL)

/// Helper: build a root with a research note AND a spec file with the given content.
fn root_with_spec(tag: &str, spec_content: &str) -> PathBuf {
    let root = scratch_root(tag);
    let research_dir = root.join("docs").join("research");
    let specs_dir = root.join("docs").join("specs");
    fs::create_dir_all(&research_dir).unwrap();
    fs::create_dir_all(&specs_dir).unwrap();
    fs::write(
        research_dir.join("2026-06-10-myslug.md"),
        "# Research\n\nFindings.\n",
    )
    .unwrap();
    fs::write(specs_dir.join("2026-06-10-myslug.md"), spec_content).unwrap();
    root
}

#[test]
fn design_gate_spec_no_approval_marker_fails_with_not_approved() {
    // Spec exists but has no Status line at all → new "not approved" failure.
    let root = root_with_spec("appr_none", "# Spec\n\nThis design is ready.\n");

    let (code, out) = run(&root, &["check", "design", "--feature", "myslug"]);
    assert_eq!(
        code, 1,
        "spec without approval marker should fail; out: {out}"
    );
    assert!(
        out.contains("not approved"),
        "output should say 'not approved'; got: {out}"
    );
    assert!(
        out.contains("Status: approved"),
        "output should suggest 'Status: approved'; got: {out}"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn design_gate_spec_with_draft_status_fails() {
    // `- **Status:** draft` is not approved.
    let root = root_with_spec("appr_draft", "# Spec\n\n- **Status:** draft\n\nBody.\n");

    let (code, out) = run(&root, &["check", "design", "--feature", "myslug"]);
    assert_eq!(code, 1, "spec with 'draft' Status should fail; out: {out}");
    assert!(
        out.contains("not approved"),
        "output should say 'not approved'; got: {out}"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn design_gate_spec_with_plain_status_approved_passes() {
    // Plain `Status: approved` form.
    let root = root_with_spec("appr_plain", "# Spec\n\nStatus: approved\n\nBody.\n");

    let (code, out) = run(&root, &["check", "design", "--feature", "myslug"]);
    assert_eq!(
        code, 0,
        "spec with 'Status: approved' should pass; out: {out}"
    );
    assert!(out.contains("PASS"), "output should say PASS; got: {out}");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn design_gate_spec_with_bold_status_approved_passes() {
    // `**Status:** approved` Markdown bold form.
    let root = root_with_spec("appr_bold", "# Spec\n\n**Status:** approved\n\nBody.\n");

    let (code, out) = run(&root, &["check", "design", "--feature", "myslug"]);
    assert_eq!(
        code, 0,
        "spec with '**Status:** approved' should pass; out: {out}"
    );
    assert!(out.contains("PASS"), "output should say PASS; got: {out}");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn design_gate_spec_with_bullet_bold_status_approved_passes() {
    // `- **Status:** approved` house-style bullet form.
    let root = root_with_spec(
        "appr_bullet",
        "# Spec\n\n- **Date:** 2026-06-10\n- **Status:** approved\n\nBody.\n",
    );

    let (code, out) = run(&root, &["check", "design", "--feature", "myslug"]);
    assert_eq!(
        code, 0,
        "spec with '- **Status:** approved' should pass; out: {out}"
    );
    assert!(out.contains("PASS"), "output should say PASS; got: {out}");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn design_gate_spec_status_approved_case_insensitive_passes() {
    // `Status:   Approved` — flexible whitespace + mixed case.
    let root = root_with_spec(
        "appr_case",
        "# Spec\n\nStatus:   Approved (2026-06-10)\n\nBody.\n",
    );

    let (code, out) = run(&root, &["check", "design", "--feature", "myslug"]);
    assert_eq!(
        code, 0,
        "spec with 'Status:   Approved' should pass; out: {out}"
    );
    assert!(out.contains("PASS"), "output should say PASS; got: {out}");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn design_gate_research_first_lock_takes_precedence_over_approval_check() {
    // Spec has approval marker but research note is absent → must fail on research-first, not "not
    // approved".  The sequence lock precedes the approval check.
    let root = scratch_root("appr_lock");
    let specs_dir = root.join("docs").join("specs");
    fs::create_dir_all(&specs_dir).unwrap();
    fs::create_dir_all(root.join("docs").join("research")).unwrap();
    fs::write(
        specs_dir.join("2026-06-10-myslug.md"),
        "# Spec\n\n- **Status:** approved\n\nBody.\n",
    )
    .unwrap();
    // No research note written.

    let (code, out) = run(&root, &["check", "design", "--feature", "myslug"]);
    assert_eq!(
        code, 1,
        "must fail when research note is absent even if spec is approved; out: {out}"
    );
    assert!(
        out.contains("research-first"),
        "output should say 'research-first' (sequence lock), not 'not approved'; got: {out}"
    );
    assert!(
        !out.contains("not approved"),
        "must not say 'not approved' when the real problem is no research note; got: {out}"
    );
    let _ = fs::remove_dir_all(&root);
}

// --- check docs ---

const VALID_SKILL_MD: &str = "---\nname: test-skill\ndescription: A test skill.\n---\n\nBody.\n";

/// Build a scratch root that satisfies all three docs lint rules (R1/R2/R3).
fn scratch_docs_root(tag: &str) -> PathBuf {
    let root = scratch_root(&format!("docs_{tag}"));

    // R1: one valid SKILL.md
    fs::create_dir_all(root.join("skills").join("my-skill")).unwrap();
    fs::write(
        root.join("skills").join("my-skill").join("SKILL.md"),
        VALID_SKILL_MD,
    )
    .unwrap();

    // R2: a docs/adr/0001-x.md linked from a scratch docs/adr/README.md
    fs::create_dir_all(root.join("docs").join("adr")).unwrap();
    fs::write(
        root.join("docs").join("adr").join("0001-x.md"),
        "# 0001 — X\n",
    )
    .unwrap();
    fs::write(
        root.join("docs").join("adr").join("README.md"),
        "| [0001](0001-x.md) | X | Accepted |\n",
    )
    .unwrap();

    // R3: ROADMAP.md citing a docs/verify/<f>.md that exists
    fs::create_dir_all(root.join("docs").join("verify")).unwrap();
    fs::write(
        root.join("docs").join("verify").join("2026-01-01-feat.md"),
        "# Verify\n",
    )
    .unwrap();
    fs::write(
        root.join("docs").join("ROADMAP.md"),
        "Evidence: `docs/verify/2026-01-01-feat.md`.\n",
    )
    .unwrap();

    root
}

#[test]
fn check_docs_clean_root_exits_0() {
    let root = scratch_docs_root("clean");
    let (code, out) = run(&root, &["check", "docs"]);
    assert_eq!(code, 0, "clean docs root should exit 0; out: {out}");
    assert!(out.contains("ok"), "output should say ok; got: {out}");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn check_docs_broken_skill_frontmatter_exits_1() {
    let root = scratch_docs_root("bad_skill");
    // Overwrite SKILL.md with missing frontmatter.
    fs::write(
        root.join("skills").join("my-skill").join("SKILL.md"),
        "No frontmatter here.\n",
    )
    .unwrap();

    let (code, out) = run(&root, &["check", "docs"]);
    assert_eq!(code, 1, "broken SKILL.md should exit 1; out: {out}");
    assert!(out.contains("R1"), "output should name R1 gap; got: {out}");
    assert!(out.contains("FAIL"), "output should say FAIL; got: {out}");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn check_docs_adr_absent_from_readme_exits_1() {
    let root = scratch_docs_root("bad_adr");
    // Add an ADR that is NOT in the README.
    fs::write(
        root.join("docs").join("adr").join("0002-y.md"),
        "# 0002 — Y\n",
    )
    .unwrap();

    let (code, out) = run(&root, &["check", "docs"]);
    assert_eq!(code, 1, "unlinked ADR should exit 1; out: {out}");
    assert!(out.contains("R2"), "output should name R2 gap; got: {out}");
    assert!(
        out.contains("0002-y.md"),
        "output should name the missing ADR; got: {out}"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn check_docs_roadmap_verify_pointer_missing_exits_1() {
    let root = scratch_docs_root("bad_r3");
    // Overwrite ROADMAP to reference a non-existent verify note.
    fs::write(
        root.join("docs").join("ROADMAP.md"),
        "Evidence: `docs/verify/2026-01-01-feat.md`. Also `docs/verify/missing.md`.\n",
    )
    .unwrap();

    let (code, out) = run(&root, &["check", "docs"]);
    assert_eq!(code, 1, "missing verify note should exit 1; out: {out}");
    assert!(out.contains("R3"), "output should name R3 gap; got: {out}");
    assert!(
        out.contains("missing.md"),
        "output should name the missing file; got: {out}"
    );
    let _ = fs::remove_dir_all(&root);
}

// ── AC-2: External-project artifacts under .claude/topology/ ──────────────
//
// A scratch git repo with TOPOLOGY_ROOT pointing at a separate scratch framework
// dir exercises the differing-roots path.

/// Build a minimal framework dir: skills/ + AGENTS.md (marks it as a topology root).
fn scratch_framework(tag: &str) -> PathBuf {
    let fw = std::env::temp_dir().join(format!("topo_check_fw_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&fw);
    fs::create_dir_all(fw.join("skills")).unwrap();
    fs::write(fw.join("AGENTS.md"), "").unwrap();
    fw
}

/// Build a scratch git project dir: git init, no topology root markers.
fn scratch_project(tag: &str) -> PathBuf {
    let proj = std::env::temp_dir().join(format!("topo_check_proj_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&proj);
    fs::create_dir_all(&proj).unwrap();
    // init a git repo so project_root() resolves here
    std::process::Command::new("git")
        .args(["-C", proj.to_str().unwrap(), "init", "-q", "-b", "main"])
        .status()
        .unwrap();
    proj
}

/// Run gatekeeper from `cwd` with TOPOLOGY_ROOT overridden to `fw`.
fn run_with_topology_root(cwd: &Path, fw: &Path, args: &[&str]) -> (i32, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_gatekeeper"))
        .current_dir(cwd)
        .env("TOPOLOGY_ROOT", fw)
        .args(args)
        .output()
        .unwrap();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
}

#[test]
fn external_project_design_fails_naming_claude_topology() {
    // AC-2: FAIL output names .claude/topology/ (not docs/).
    let fw = scratch_framework("ext_design_fail");
    let proj = scratch_project("ext_design_fail");

    let (code, out) = run_with_topology_root(
        &proj,
        &fw,
        &["check", "design", "--feature", "installer-v2"],
    );
    assert_eq!(code, 1, "should fail when artifacts absent; out:\n{out}");
    // FAIL message must name .claude/topology/, not docs/
    assert!(
        out.contains(".claude/topology/"),
        "FAIL must name .claude/topology/; got:\n{out}"
    );
    assert!(
        !out.contains("no docs/research"),
        "must NOT name docs/research; got:\n{out}"
    );
    let _ = fs::remove_dir_all(&fw);
    let _ = fs::remove_dir_all(&proj);
}

#[test]
fn external_project_design_passes_after_artifacts_placed_in_claude_topology() {
    // AC-2: placing research + spec under .claude/topology/ → PASS.
    let fw = scratch_framework("ext_design_pass");
    let proj = scratch_project("ext_design_pass");

    // Place research note under .claude/topology/research/
    let research_dir = proj.join(".claude").join("topology").join("research");
    fs::create_dir_all(&research_dir).unwrap();
    fs::write(
        research_dir.join("2026-06-10-installer-v2.md"),
        "# Research\n\nFindings.\n",
    )
    .unwrap();

    // Without spec → design still fails (but names .claude/topology/).
    let (code, out) = run_with_topology_root(
        &proj,
        &fw,
        &["check", "design", "--feature", "installer-v2"],
    );
    assert_eq!(code, 1, "no spec yet → fail; out:\n{out}");
    assert!(
        out.contains(".claude/topology/"),
        "FAIL must still name .claude/topology/; got:\n{out}"
    );

    // Place spec under .claude/topology/specs/ → PASS.
    let specs_dir = proj.join(".claude").join("topology").join("specs");
    fs::create_dir_all(&specs_dir).unwrap();
    fs::write(
        specs_dir.join("2026-06-10-installer-v2.md"),
        "# Spec\n\n- **Status:** approved\n\nReady.\n",
    )
    .unwrap();

    let (code, out) = run_with_topology_root(
        &proj,
        &fw,
        &["check", "design", "--feature", "installer-v2"],
    );
    assert_eq!(
        code, 0,
        "design should PASS after both artifacts placed (with approval marker); out:\n{out}"
    );
    assert!(out.contains("PASS"), "output should say PASS; got:\n{out}");

    let _ = fs::remove_dir_all(&fw);
    let _ = fs::remove_dir_all(&proj);
}

#[test]
fn external_project_docs_root_not_found() {
    // AC-2: files under the project's root docs/ are NOT found (old location genuinely moved).
    let fw = scratch_framework("ext_docs_ignored");
    let proj = scratch_project("ext_docs_ignored");

    // Place research + spec under the project's root docs/ (the old location).
    let research_dir = proj.join("docs").join("research");
    let specs_dir = proj.join("docs").join("specs");
    fs::create_dir_all(&research_dir).unwrap();
    fs::create_dir_all(&specs_dir).unwrap();
    fs::write(
        research_dir.join("2026-06-10-installer-v2.md"),
        "# Research\n",
    )
    .unwrap();
    fs::write(specs_dir.join("2026-06-10-installer-v2.md"), "# Spec\n").unwrap();

    // Gate should still FAIL: artifacts in project's root docs/ are not found.
    let (code, _out) = run_with_topology_root(
        &proj,
        &fw,
        &["check", "design", "--feature", "installer-v2"],
    );
    assert_eq!(
        code, 1,
        "artifacts in project docs/ must NOT be found when roots differ"
    );

    let _ = fs::remove_dir_all(&fw);
    let _ = fs::remove_dir_all(&proj);
}
