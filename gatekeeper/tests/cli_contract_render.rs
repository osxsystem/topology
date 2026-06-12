//! Integration tests for `gatekeeper adapt --contract <framework|project>` (Phase 10).
//!
//! These tests are `#[ignore]`-tagged and committed red in Task 1; Task 2 un-ignores
//! them once the production code exists. Force-run with:
//!
//!   cargo test --manifest-path gatekeeper/Cargo.toml \
//!     --test cli_contract_render -- --ignored --include-ignored
//!
//! Binary requirement: `TOPOLOGY_ROOT` is pinned to a scratch framework root whose
//! `templates/CONTRACT.template.md` is provided by the fixture. Tests exercise the
//! binary surface only (no direct calls into adapt internals).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

// ── fixture helpers ───────────────────────────────────────────────────────────

/// A minimal framework root: skills/ marker, AGENTS.md, and a valid template.
/// The template uses the three known placeholders so all three substitutions are exercised.
fn scratch_framework(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "topo_contract_fw_{tag}_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("skills")).unwrap();
    fs::create_dir_all(root.join("templates")).unwrap();
    fs::write(root.join("AGENTS.md"), "# Topology Agent\n\nContract.\n").unwrap();
    fs::write(
        root.join("templates").join("CONTRACT.template.md"),
        GOOD_TEMPLATE,
    )
    .unwrap();
    root
}

/// A framework root with a template that contains an unknown placeholder.
fn scratch_bad_template(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "topo_contract_bad_{tag}_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("skills")).unwrap();
    fs::create_dir_all(root.join("templates")).unwrap();
    fs::write(root.join("AGENTS.md"), "# Topology Agent\n\nContract.\n").unwrap();
    fs::write(
        root.join("templates").join("CONTRACT.template.md"),
        BAD_TEMPLATE,
    )
    .unwrap();
    root
}

/// Run `gatekeeper <args>` from `cwd` with TOPOLOGY_ROOT pinned to `framework`.
/// Returns `(exit_code, stdout, stderr)`.
fn run_with_root(cwd: &Path, framework: &Path, args: &[&str]) -> (i32, String, String) {
    let canonical = fs::canonicalize(framework).unwrap_or_else(|_| framework.to_path_buf());
    let out = Command::new(env!("CARGO_BIN_EXE_gatekeeper"))
        .current_dir(cwd)
        .args(args)
        .env("TOPOLOGY_ROOT", &canonical)
        .output()
        .unwrap();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

// ── template constants ────────────────────────────────────────────────────────

/// A valid template that uses all three known placeholders.
const GOOD_TEMPLATE: &str = "\
# Topology Agent

Artifacts root: {{ARTIFACTS_ROOT}}

Gate sequence:

- Design gate: {{ARTIFACTS_ROOT}}/specs/<date>-<feature>.md
- Plan gate: {{ARTIFACTS_ROOT}}/plans/<date>-<feature>.md
- Verify gate: {{ARTIFACTS_ROOT}}/verify/<date>-<feature>.md
- Review gate: {{ARTIFACTS_ROOT}}/reviews/<date>-<feature>.md

Invoke the gate checker as: {{GATEKEEPER_CMD}} check design --feature <slug>

{{BINARY_NOTE}}
";

/// A template with an unknown placeholder — must cause exit 2.
const BAD_TEMPLATE: &str = "\
# Topology Agent

{{ARTIFACTS_ROOT}} is fine but {{UNKNOWN_PLACEHOLDER}} is not.

{{GATEKEEPER_CMD}} and {{BINARY_NOTE}} are fine.
";

// ── AC-2: framework render ────────────────────────────────────────────────────

/// Framework render must contain `docs/` paths and zero `.claude/topology` occurrences.
#[test]
fn framework_render_contains_docs_paths() {
    let root = scratch_framework("fw_render");
    let (code, stdout, stderr) = run_with_root(&root, &root, &["adapt", "--contract", "framework"]);
    assert_eq!(
        code, 0,
        "framework render must exit 0; stderr: {stderr}"
    );
    assert!(
        stdout.contains("docs/"),
        "framework render must contain 'docs/' paths; stdout:\n{stdout}"
    );
    assert!(
        !stdout.contains(".claude/topology"),
        "framework render must not contain '.claude/topology'; stdout:\n{stdout}"
    );
    let _ = fs::remove_dir_all(&root);
}

// ── AC-2: project render ──────────────────────────────────────────────────────

/// Project render must contain `.claude/topology/` paths and zero `docs/`-rooted artifact paths.
#[test]
fn project_render_contains_topology_paths() {
    let root = scratch_framework("proj_render");
    let (code, stdout, stderr) =
        run_with_root(&root, &root, &["adapt", "--contract", "project"]);
    assert_eq!(
        code, 0,
        "project render must exit 0; stderr: {stderr}"
    );
    assert!(
        stdout.contains(".claude/topology/"),
        "project render must contain '.claude/topology/' paths; stdout:\n{stdout}"
    );
    // Must not contain `docs/<kind>/` artifact paths (docs/ as prefix would be a framework path).
    // We check for the specific artifact path prefixes from the gate sequence.
    let has_docs_artifact = stdout.contains("docs/specs/")
        || stdout.contains("docs/plans/")
        || stdout.contains("docs/verify/")
        || stdout.contains("docs/reviews/");
    assert!(
        !has_docs_artifact,
        "project render must not contain docs/<kind>/ artifact paths; stdout:\n{stdout}"
    );
    let _ = fs::remove_dir_all(&root);
}

// ── AC-3: fail-closed on unknown placeholder ──────────────────────────────────

/// An unknown placeholder in the template must cause exit 2 and the stderr message
/// must name the offending placeholder.
#[test]
fn unknown_placeholder_exits_2_and_names_it() {
    let root = scratch_bad_template("bad_ph");
    let (code, _stdout, stderr) =
        run_with_root(&root, &root, &["adapt", "--contract", "framework"]);
    assert_eq!(
        code, 2,
        "unknown placeholder must cause exit 2; stderr: {stderr}"
    );
    assert!(
        stderr.contains("UNKNOWN_PLACEHOLDER"),
        "stderr must name the offending placeholder; stderr:\n{stderr}"
    );
    let _ = fs::remove_dir_all(&root);
}

// ── AC-4: AGENTS.md byte-equality with framework render ──────────────────────

/// The on-disk `AGENTS.md` at the repo root must byte-equal the output of
/// `gatekeeper adapt --contract framework` (which appends the dev-doc trailer).
/// This guards against hand-edits or template drift.
#[test]
fn agents_md_byte_equal_to_framework_render() {
    // Use the real repo root (not a scratch dir) — this is the dogfood assertion.
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest.parent().expect("manifest has parent").to_path_buf();

    let canonical = fs::canonicalize(&repo_root).unwrap_or_else(|_| repo_root.clone());
    let out = Command::new(env!("CARGO_BIN_EXE_gatekeeper"))
        .current_dir(&repo_root)
        .args(["adapt", "--contract", "framework"])
        .env("TOPOLOGY_ROOT", &canonical)
        .output()
        .expect("failed to spawn gatekeeper");

    let code = out.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();

    assert_eq!(
        code, 0,
        "adapt --contract framework must exit 0 at real repo root; stderr: {stderr}"
    );

    let agents_on_disk = fs::read_to_string(repo_root.join("AGENTS.md"))
        .expect("AGENTS.md must exist at repo root");

    assert_eq!(
        stdout, agents_on_disk,
        "AGENTS.md must byte-equal adapt --contract framework output"
    );
}

// ── AC-1/AC-3: unknown contract world exits 2 ────────────────────────────────

/// `adapt --contract <unknown>` must exit 2.
#[test]
fn unknown_contract_world_exits_2() {
    let root = scratch_framework("unk_world");
    let (code, _stdout, stderr) =
        run_with_root(&root, &root, &["adapt", "--contract", "frobnicate"]);
    assert_eq!(
        code, 2,
        "unknown contract world must exit 2; stderr: {stderr}"
    );
    let _ = fs::remove_dir_all(&root);
}

// ── missing template exits 2 ─────────────────────────────────────────────────

/// When `templates/CONTRACT.template.md` is absent the command must exit 2.
#[test]
fn missing_template_exits_2() {
    let root = scratch_framework("no_template");
    // Remove the template we just created.
    fs::remove_file(root.join("templates").join("CONTRACT.template.md")).unwrap();
    let (code, _stdout, stderr) =
        run_with_root(&root, &root, &["adapt", "--contract", "framework"]);
    assert_eq!(
        code, 2,
        "missing template must cause exit 2; stderr: {stderr}"
    );
    assert!(
        !stderr.is_empty(),
        "stderr must be non-empty on missing template"
    );
    let _ = fs::remove_dir_all(&root);
}
