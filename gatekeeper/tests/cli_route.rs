//! Integration tests for `gatekeeper route --paths/--staged-paths` (path-triggered routing, Slice 1).
//!
//! `route` mirrors `activate`'s output grammar but keys on file paths instead of prompt keywords:
//!   - `route --paths <p...>` prints the skills whose `pathTriggers.globs` match any path.
//!   - a path matching no glob prints the "no skills" line.
//!   - `--help`/`-h` exit 0; an unrecognized flag exits 2.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

// ── helpers ──────────────────────────────────────────────────────────────────

/// Minimal framework root carrying a `hooks/skill-rules.json` with a `security-scanning`
/// skill that has a `pathTriggers` glob (`hooks/*`).
fn scratch_root(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("topo_route_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("skills")).unwrap();
    fs::write(root.join("AGENTS.md"), "").unwrap();
    fs::create_dir_all(root.join("security")).unwrap();
    fs::write(
        root.join("security").join("rules.toml"),
        "schema_version = 1\n",
    )
    .unwrap();
    fs::create_dir_all(root.join("hooks")).unwrap();
    fs::write(
        root.join("hooks").join("skill-rules.json"),
        r#"{
  "version": "1.0",
  "skills": {
    "security-scanning": {
      "type": "process",
      "enforcement": "require",
      "priority": "high",
      "pathTriggers": { "globs": ["hooks/*"] }
    }
  }
}
"#,
    )
    .unwrap();
    root
}

/// Run `gatekeeper <args>` from `cwd` with a closed stdin pipe. Returns `(exit_code, stdout, stderr)`.
fn run(cwd: &Path, args: &[&str]) -> (i32, String, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_gatekeeper"))
        .current_dir(cwd)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn gatekeeper");
    let mut child_stdin = child.stdin.take().unwrap();
    if let Err(e) = child_stdin.write_all(b"") {
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

// ── tests ────────────────────────────────────────────────────────────────────

/// A path matching a `pathTriggers` glob routes the skill as `[require]`.
#[test]
fn route_paths_matching_glob_routes_security() {
    let root = scratch_root("match");
    let (code, out, stderr) = run(&root, &["route", "--paths", "hooks/x.sh"]);
    assert_eq!(code, 0, "route --paths must exit 0; stderr: {stderr}");
    assert!(
        out.contains("- security-scanning [require]"),
        "expected security-scanning [require]; got: {out}"
    );
    let _ = fs::remove_dir_all(&root);
}

/// A path matching no glob prints the "no skills" line and still exits 0.
#[test]
fn route_paths_no_match_prints_no_skills() {
    let root = scratch_root("nomatch");
    let (code, out, stderr) = run(&root, &["route", "--paths", "README.md"]);
    assert_eq!(code, 0, "route --paths must exit 0; stderr: {stderr}");
    assert!(
        !out.contains("security-scanning"),
        "README.md must not route security-scanning; got: {out}"
    );
    assert!(
        out.to_lowercase().contains("no ") && out.to_lowercase().contains("skills"),
        "expected a 'no skills' line; got: {out}"
    );
    let _ = fs::remove_dir_all(&root);
}

/// `route --help` exits 0 and prints usage.
#[test]
fn route_help_exits_0() {
    let root = scratch_root("help");
    let (code, out, _) = run(&root, &["route", "--help"]);
    assert_eq!(code, 0, "route --help must exit 0; stdout: {out}");
    assert!(
        out.contains("route"),
        "route --help output should mention 'route'; got: {out}"
    );
    let _ = fs::remove_dir_all(&root);
}

/// `route --bogus` is an unrecognized flag → exit 2, names the flag.
#[test]
fn route_unknown_flag_exits_2() {
    let root = scratch_root("unk");
    let (code, _out, stderr) = run(&root, &["route", "--bogus"]);
    assert_eq!(code, 2, "route --bogus must exit 2; stderr: {stderr}");
    assert!(
        stderr.contains("unknown flag") && stderr.contains("--bogus"),
        "stderr must name the unknown flag; got: {stderr}"
    );
    let _ = fs::remove_dir_all(&root);
}
