//! Integration tests for `gatekeeper memory`: write, read, list, and all guard cases.
//!
//! Mirrors the scratch-root / run_stdin helper style of `cli_adapt.rs` and `cli_check.rs`;
//! no `assert_cmd`/`predicates`. Uses `env!("CARGO_BIN_EXE_gatekeeper")`.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// A minimal framework root: a `skills/` marker so `framework_root()` resolves here,
/// plus a `security/rules.toml` that the secret-refusal scan requires.
fn scratch_root(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("topo_mem_cli_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("skills")).unwrap();
    fs::create_dir_all(root.join("security")).unwrap();
    fs::write(
        root.join("security").join("rules.toml"),
        "schema_version = 1\n\n\
         [[rule]]\n\
         id = \"aws-key\"\n\
         kind = \"content\"\n\
         severity = \"block\"\n\
         description = \"AWS access key id\"\n\
         pattern = '\\b(AKIA|ASIA)[0-9A-Z]{16}\\b'\n\
         \n\
         [[allow]]\n\
         rule = \"aws-key\"\n\
         value = \"AKIAIOSFODNN7EXAMPLE\"\n\
         reason = \"canonical AWS documentation example key\"\n",
    )
    .unwrap();
    root
}

/// Run `gatekeeper <args>` from `cwd`, piping `body` to stdin.
/// Returns `(exit_code, stdout, stderr)`.
///
/// TOPOLOGY_ROOT is pinned to `cwd` so that after Phase 11 binary-adjacent resolution
/// does not silently point at the actual topology repo instead of the scratch root.
fn run_stdin(cwd: &Path, args: &[&str], body: &[u8]) -> (i32, String, String) {
    let canonical_cwd = std::fs::canonicalize(cwd).unwrap_or_else(|_| cwd.to_path_buf());
    let mut child = Command::new(env!("CARGO_BIN_EXE_gatekeeper"))
        .current_dir(cwd)
        .args(args)
        .env("TOPOLOGY_ROOT", &canonical_cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn gatekeeper");
    // The child may reject its args and exit before reading stdin, closing the pipe; a
    // partial write then races to BrokenPipe. We assert on the exit code and on-disk effect,
    // not on stdin being consumed, so tolerate EPIPE here.
    let mut child_stdin = child.stdin.take().unwrap();
    if let Err(e) = child_stdin.write_all(body) {
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

/// Run `gatekeeper <args>` from `cwd` with no stdin body (null stdin).
/// Returns `(exit_code, stdout, stderr)`.
fn run(cwd: &Path, args: &[&str]) -> (i32, String, String) {
    run_stdin(cwd, args, b"")
}

// --- write + read round-trip ---

#[test]
fn write_then_read_is_byte_equal() {
    let root = scratch_root("write-read");
    let body = b"## Goal\nRound-trip test.\n\n## State\n- done: nothing yet\n";

    let (code, _, stderr) = run_stdin(
        &root,
        &[
            "memory",
            "write",
            "--feature",
            "my-feat",
            "--date",
            "2026-06-08",
        ],
        body,
    );
    assert_eq!(code, 0, "write must succeed; stderr={stderr}");

    // In the scratch root: project_root() == framework_root() (both fall back to cwd), so
    // artifacts_root() = scratch_root/docs → memory dir = scratch_root/docs/memory.
    let written = fs::read(root.join("docs").join("memory").join("my-feat.handoff.md"))
        .expect("artifact must exist after successful write");

    // Verify stamped frontmatter fields are present.
    let content = std::str::from_utf8(&written).expect("artifact must be valid UTF-8");
    assert!(
        content.contains("feature: my-feat\n"),
        "must have feature field"
    );
    assert!(
        content.contains("created: 2026-06-08\n"),
        "must have created field"
    );
    assert!(
        content.contains("status: in-progress\n"),
        "must have status field"
    );
    assert!(
        content.contains("verified_by: \n"),
        "must have verified_by field"
    );

    let (rcode, stdout, rstderr) = run(&root, &["memory", "read", "--feature", "my-feat"]);
    assert_eq!(rcode, 0, "read must succeed; stderr={rstderr}");
    assert_eq!(
        stdout.as_bytes(),
        written.as_slice(),
        "read output must be byte-equal to the written file"
    );
    let _ = fs::remove_dir_all(&root);
}

// --- read unknown slug ---

#[test]
fn read_missing_slug_exits_1() {
    let root = scratch_root("read-missing");
    let (code, _, _) = run(&root, &["memory", "read", "--feature", "nonexistent"]);
    assert_eq!(code, 1, "reading an unknown slug must exit 1");
    let _ = fs::remove_dir_all(&root);
}

// --- secret in body ---

#[test]
fn secret_in_body_exits_1_no_file() {
    let root = scratch_root("secret-body");
    // Use a fake AWS key that is NOT in the allowlist.
    let body = format!("## Goal\nAKIA{}\n", "1234567890ABCDEF");
    let (code, _, _) = run_stdin(
        &root,
        &[
            "memory",
            "write",
            "--feature",
            "my-feat",
            "--date",
            "2026-06-08",
        ],
        body.as_bytes(),
    );
    assert_eq!(code, 1, "non-allowlisted secret in body must exit 1");
    assert!(
        !root
            .join("docs")
            .join("memory")
            .join("my-feat.handoff.md")
            .exists(),
        "target file must not be written on secret refusal"
    );
    let _ = fs::remove_dir_all(&root);
}

// --- secret reachable only via a stamped frontmatter field ---

#[test]
fn secret_via_stamped_field_exits_1_no_file() {
    // The scan runs on the full RENDERED artifact, not just the stdin body — so a secret
    // reaching a stamped frontmatter field (here `verified_by`, an unvalidated passthrough)
    // is refused even when the body is clean. This is the distinction from the body case.
    let root = scratch_root("secret-stamp");
    let secret = format!("AKIA{}", "1234567890ABCDEF"); // non-allowlisted; split so the source file stays clean
    let (code, _, stderr) = run_stdin(
        &root,
        &[
            "memory",
            "write",
            "--feature",
            "my-feat",
            "--date",
            "2026-06-08",
            "--verified-by",
            secret.as_str(),
        ],
        b"## Goal\nclean body, secret is only in the stamped field\n",
    );
    assert_eq!(
        code, 1,
        "secret in a stamped frontmatter field must exit 1; stderr={stderr}"
    );
    assert!(
        !root
            .join("docs")
            .join("memory")
            .join("my-feat.handoff.md")
            .exists(),
        "target file must not be written on secret refusal"
    );
    let _ = fs::remove_dir_all(&root);
}

// --- status done without verify note ---

#[test]
fn status_done_without_verify_note_exits_1() {
    let root = scratch_root("done-no-verify");
    let (code, _, _) = run_stdin(
        &root,
        &[
            "memory",
            "write",
            "--feature",
            "my-feat",
            "--date",
            "2026-06-08",
            "--status",
            "done",
            "--verified-by",
            "my-verify-note",
        ],
        b"## Goal\nbody\n",
    );
    assert_eq!(code, 1, "done without verify note must exit 1");
    assert!(
        !root
            .join("docs")
            .join("memory")
            .join("my-feat.handoff.md")
            .exists(),
        "target file must not be written when verify note is absent"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn status_done_with_verify_note_succeeds() {
    let root = scratch_root("done-with-verify");
    fs::create_dir_all(root.join("docs").join("verify")).unwrap();
    fs::write(
        root.join("docs")
            .join("verify")
            .join("2026-06-08-my-feat.md"),
        "# Verify note\n",
    )
    .unwrap();
    let (code, _, stderr) = run_stdin(
        &root,
        &[
            "memory",
            "write",
            "--feature",
            "my-feat",
            "--date",
            "2026-06-08",
            "--status",
            "done",
            "--verified-by",
            "my-feat",
        ],
        b"## Goal\nbody\n",
    );
    assert_eq!(
        code, 0,
        "done with verify note must succeed; stderr={stderr}"
    );
    assert!(
        root.join("docs")
            .join("memory")
            .join("my-feat.handoff.md")
            .exists(),
        "artifact must exist after successful write"
    );
    let _ = fs::remove_dir_all(&root);
}

// --- validation guards ---

#[test]
fn invalid_feature_exits_nonzero_no_file() {
    let root = scratch_root("inv-feat");
    let (code, _, _) = run_stdin(
        &root,
        &[
            "memory",
            "write",
            "--feature",
            "../escape",
            "--date",
            "2026-06-08",
        ],
        b"## Goal\nbody\n",
    );
    assert_ne!(code, 0, "invalid --feature must exit non-zero");
    assert!(
        !root.join("docs").join("memory").exists(),
        "memory dir must not be created on validation failure"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn malformed_date_exits_nonzero_no_file() {
    let root = scratch_root("bad-date");
    let (code, _, _) = run_stdin(
        &root,
        &[
            "memory",
            "write",
            "--feature",
            "my-feat",
            "--date",
            "06/08/2026",
        ],
        b"## Goal\nbody\n",
    );
    assert_ne!(code, 0, "malformed --date must exit non-zero");
    assert!(
        !root.join("docs").join("memory").exists(),
        "memory dir must not be created on validation failure"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn double_frontmatter_body_exits_nonzero_no_file() {
    let root = scratch_root("double-fm");
    let body = b"---\nextra: frontmatter\n---\n\nbody\n";
    let (code, _, _) = run_stdin(
        &root,
        &[
            "memory",
            "write",
            "--feature",
            "my-feat",
            "--date",
            "2026-06-08",
        ],
        body,
    );
    assert_ne!(code, 0, "double frontmatter body must exit non-zero");
    assert!(
        !root.join("docs").join("memory").exists(),
        "memory dir must not be created when body opens a second frontmatter block"
    );
    let _ = fs::remove_dir_all(&root);
}

// --- memory list ---

#[test]
fn list_shows_written_entries() {
    let root = scratch_root("list-two");
    let body = b"## Goal\nTest.\n";

    let (w1, _, e1) = run_stdin(
        &root,
        &[
            "memory",
            "write",
            "--feature",
            "feat-alpha",
            "--date",
            "2026-01-01",
        ],
        body,
    );
    assert_eq!(w1, 0, "write feat-alpha must succeed; stderr={e1}");

    let (w2, _, e2) = run_stdin(
        &root,
        &[
            "memory",
            "write",
            "--feature",
            "feat-beta",
            "--date",
            "2026-06-08",
        ],
        body,
    );
    assert_eq!(w2, 0, "write feat-beta must succeed; stderr={e2}");

    let (code, stdout, _) = run(&root, &["memory", "list"]);
    assert_eq!(code, 0, "list must succeed");
    assert!(
        stdout.contains("feat-alpha"),
        "list must show feat-alpha:\n{stdout}"
    );
    assert!(
        stdout.contains("feat-beta"),
        "list must show feat-beta:\n{stdout}"
    );
    assert!(
        stdout.contains("2026-01-01"),
        "list must show created date of feat-alpha:\n{stdout}"
    );
    assert!(
        stdout.contains("2026-06-08"),
        "list must show created date of feat-beta:\n{stdout}"
    );
    let _ = fs::remove_dir_all(&root);
}

// --- unknown subcommand ---

#[test]
fn unknown_subcommand_exits_2() {
    let root = scratch_root("unknown-sub");
    let (code, _, _) = run(&root, &["memory", "frobnicate"]);
    assert_eq!(code, 2, "unknown subcommand must exit 2");
    let _ = fs::remove_dir_all(&root);
}

// ── artifacts-root anchoring tests ────────────────────────────────────────────
//
// These tests exercise the ADR-0013 rule: handoffs anchor to the artifacts root,
// not the framework root.
//
// "In-repo" scenario  — project == framework (same dir has .git + skills/ + AGENTS.md)
//   → artifacts_root() = <root>/docs
//   → handoff path = <root>/docs/memory/<slug>.handoff.md
//
// "Governed" scenario — project ≠ framework (separate dirs, TOPOLOGY_ROOT points at framework)
//   → artifacts_root() = <project>/.claude/topology
//   → handoff path = <project>/.claude/topology/memory/<slug>.handoff.md

/// Build a minimal framework dir: skills/ + AGENTS.md marker + security/rules.toml.
fn scratch_framework(tag: &str) -> PathBuf {
    let fw = std::env::temp_dir().join(format!("topo_mem_fw_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&fw);
    fs::create_dir_all(fw.join("skills")).unwrap();
    fs::write(fw.join("AGENTS.md"), "").unwrap();
    fs::create_dir_all(fw.join("security")).unwrap();
    fs::write(
        fw.join("security").join("rules.toml"),
        "schema_version = 1\n\n\
         [[rule]]\n\
         id = \"aws-key\"\n\
         kind = \"content\"\n\
         severity = \"block\"\n\
         description = \"AWS access key id\"\n\
         pattern = '\\b(AKIA|ASIA)[0-9A-Z]{16}\\b'\n\
         \n\
         [[allow]]\n\
         rule = \"aws-key\"\n\
         value = \"AKIAIOSFODNN7EXAMPLE\"\n\
         reason = \"canonical AWS documentation example key\"\n",
    )
    .unwrap();
    fw
}

/// Build a minimal project dir with a .git dir (no framework markers).
fn scratch_project(tag: &str) -> PathBuf {
    let proj = std::env::temp_dir().join(format!("topo_mem_proj_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&proj);
    fs::create_dir_all(&proj).unwrap();
    // Initialise a bare .git directory so resolve_project_root() finds it.
    std::process::Command::new("git")
        .args(["init", proj.to_str().unwrap()])
        .output()
        .expect("git init must succeed");
    proj
}

/// Run with an explicit TOPOLOGY_ROOT env var pointing at the framework dir.
fn run_with_topology_root(
    cwd: &Path,
    topology_root: &Path,
    args: &[&str],
    body: &[u8],
) -> (i32, String, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_gatekeeper"))
        .current_dir(cwd)
        .env("TOPOLOGY_ROOT", topology_root)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn gatekeeper");
    let mut child_stdin = child.stdin.take().unwrap();
    if let Err(e) = child_stdin.write_all(body) {
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

#[test]
fn governed_write_lands_at_claude_topology_memory() {
    let fw = scratch_framework("gov-write");
    let proj = scratch_project("gov-write");
    let body = b"## Goal\nGoverned handoff.\n";

    let (code, stdout, stderr) = run_with_topology_root(
        &proj,
        &fw,
        &[
            "memory",
            "write",
            "--feature",
            "gov-feat",
            "--date",
            "2026-06-10",
        ],
        body,
    );
    assert_eq!(
        code, 0,
        "write must succeed in governed repo; stderr={stderr}"
    );

    let expected = proj
        .join(".claude")
        .join("topology")
        .join("memory")
        .join("gov-feat.handoff.md");
    assert!(
        expected.exists(),
        "handoff must land at .claude/topology/memory/<slug>.handoff.md; stdout={stdout}"
    );

    // Old path must NOT exist.
    assert!(
        !proj.join("memory").join("artifacts").exists(),
        "old memory/artifacts path must not be created"
    );

    let _ = fs::remove_dir_all(&fw);
    let _ = fs::remove_dir_all(&proj);
}

#[test]
fn governed_read_round_trips_write() {
    let fw = scratch_framework("gov-rt");
    let proj = scratch_project("gov-rt");
    let body = b"## Goal\nRound-trip in governed repo.\n";

    let (wcode, _, werr) = run_with_topology_root(
        &proj,
        &fw,
        &[
            "memory",
            "write",
            "--feature",
            "gov-rt-feat",
            "--date",
            "2026-06-10",
        ],
        body,
    );
    assert_eq!(wcode, 0, "write must succeed; stderr={werr}");

    let artifact_path = proj
        .join(".claude")
        .join("topology")
        .join("memory")
        .join("gov-rt-feat.handoff.md");
    let written = fs::read(&artifact_path).expect("artifact must exist after write");

    let (rcode, stdout, rerr) = run_with_topology_root(
        &proj,
        &fw,
        &["memory", "read", "--feature", "gov-rt-feat"],
        b"",
    );
    assert_eq!(rcode, 0, "read must succeed; stderr={rerr}");
    assert_eq!(
        stdout.as_bytes(),
        written.as_slice(),
        "read output must be byte-equal to the written file"
    );

    let _ = fs::remove_dir_all(&fw);
    let _ = fs::remove_dir_all(&proj);
}

#[test]
fn in_repo_write_lands_at_docs_memory() {
    // In-repo scenario: project == framework (same dir has .git + skills/ + AGENTS.md).
    // artifacts_root() = <root>/docs → handoff = <root>/docs/memory/<slug>.handoff.md.
    let root = scratch_root("inrepo-write");
    // Add AGENTS.md so is_marked_root() treats root as framework root.
    fs::write(root.join("AGENTS.md"), "").unwrap();
    // Add .git so project_root() resolves to root.
    std::process::Command::new("git")
        .args(["init", root.to_str().unwrap()])
        .output()
        .expect("git init must succeed");

    let body = b"## Goal\nIn-repo handoff.\n";
    // Run with TOPOLOGY_ROOT = root so framework_root() == project_root() == root.
    let (code, stdout, stderr) = run_with_topology_root(
        &root,
        &root,
        &[
            "memory",
            "write",
            "--feature",
            "inrepo-feat",
            "--date",
            "2026-06-10",
        ],
        body,
    );
    assert_eq!(
        code, 0,
        "write must succeed in-repo; stderr={stderr}; stdout={stdout}"
    );

    let expected = root
        .join("docs")
        .join("memory")
        .join("inrepo-feat.handoff.md");
    assert!(
        expected.exists(),
        "handoff must land at docs/memory/<slug>.handoff.md"
    );

    // Old path must NOT exist.
    assert!(
        !root.join("memory").join("artifacts").exists(),
        "old memory/artifacts path must not be created"
    );

    let _ = fs::remove_dir_all(&root);
}
