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
fn run_stdin(cwd: &Path, args: &[&str], body: &[u8]) -> (i32, String, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_gatekeeper"))
        .current_dir(cwd)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn gatekeeper");
    child.stdin.take().unwrap().write_all(body).unwrap();
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

    let written = fs::read(
        root.join("memory")
            .join("artifacts")
            .join("my-feat.handoff.md"),
    )
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
            .join("memory")
            .join("artifacts")
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
            .join("memory")
            .join("artifacts")
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
            .join("memory")
            .join("artifacts")
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
        root.join("memory")
            .join("artifacts")
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
        !root.join("memory").join("artifacts").exists(),
        "artifacts dir must not be created on validation failure"
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
        !root.join("memory").join("artifacts").exists(),
        "artifacts dir must not be created on validation failure"
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
        !root.join("memory").join("artifacts").exists(),
        "artifacts dir must not be created when body opens a second frontmatter block"
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
