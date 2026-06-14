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

/// Run `gatekeeper <args>` from `cwd`, piping `stdin_data` to the child's stdin.
/// Returns `(exit_code, stdout, stderr)`.
fn run_with_stdin(cwd: &Path, args: &[&str], stdin_data: &str) -> (i32, String, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_gatekeeper"))
        .current_dir(cwd)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn gatekeeper");
    let mut child_stdin = child.stdin.take().unwrap();
    if let Err(e) = child_stdin.write_all(stdin_data.as_bytes()) {
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

/// `route --hook` reads a PostToolUse JSON event on stdin and routes by its `file_path`.
#[test]
fn route_hook_routes_from_json() {
    let root = scratch_root("hook");
    // A trigger path inside the PostToolUse JSON routes security-scanning, exit 0.
    let trigger = r#"{"tool_name":"Edit","tool_input":{"file_path":"hooks/x.sh"}}"#;
    let (code, out, stderr) = run_with_stdin(&root, &["route", "--hook"], trigger);
    assert_eq!(code, 0, "route --hook must exit 0; stderr: {stderr}");
    assert!(
        out.contains("- security-scanning [require]"),
        "expected security-scanning [require]; got: {out}"
    );
    // A non-trigger path prints the "no skills" line, still exit 0.
    let non_trigger = r#"{"tool_name":"Edit","tool_input":{"file_path":"README.md"}}"#;
    let (code, out, stderr) = run_with_stdin(&root, &["route", "--hook"], non_trigger);
    assert_eq!(code, 0, "route --hook must exit 0; stderr: {stderr}");
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

/// `route` with no path-selecting flag still exits 2 (Slice 1 behavior preserved).
#[test]
fn route_no_flag_exits_2() {
    let root = scratch_root("noflag");
    let (code, _out, stderr) = run(&root, &["route"]);
    assert_eq!(code, 2, "bare route must exit 2; stderr: {stderr}");
    let _ = fs::remove_dir_all(&root);
}

/// The advisory PostToolUse hook script routes from JSON and always exits 0 (fail-open).
#[test]
fn post_tool_routing_hook_routes_and_fails_open() {
    let root = scratch_root("hookscript");
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let script = repo_root.join("hooks").join("post-tool-routing.sh");
    let gk = env!("CARGO_BIN_EXE_gatekeeper");

    let spawn = |stdin_data: &str| -> (i32, String) {
        let mut child = Command::new("bash")
            .arg(&script)
            // CLAUDE_PLUGIN_ROOT points at the scratch root (where skill-rules.json lives);
            // GATEKEEPER_BIN points at the built binary.
            .env("CLAUDE_PLUGIN_ROOT", &root)
            .env("GATEKEEPER_BIN", gk)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("failed to spawn post-tool-routing.sh");
        let mut si = child.stdin.take().unwrap();
        if let Err(e) = si.write_all(stdin_data.as_bytes()) {
            if e.kind() != std::io::ErrorKind::BrokenPipe {
                panic!("failed to write hook stdin: {e}");
            }
        }
        drop(si);
        let out = child.wait_with_output().unwrap();
        let mut combined = String::from_utf8_lossy(&out.stdout).into_owned();
        combined.push_str(&String::from_utf8_lossy(&out.stderr));
        (out.status.code().unwrap_or(-1), combined)
    };

    // A trigger path surfaces the skill, exit 0.
    let trigger = r#"{"tool_name":"Edit","tool_input":{"file_path":"hooks/x.sh"}}"#;
    let (code, out) = spawn(trigger);
    assert_eq!(code, 0, "hook must exit 0; got: {out}");
    assert!(
        out.contains("security-scanning"),
        "expected security-scanning in hook output; got: {out}"
    );

    // Malformed stdin still exits 0 (fail-open).
    let (code, out) = spawn("}{ not json");
    assert_eq!(code, 0, "malformed stdin must still exit 0; got: {out}");

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
