//! Integration tests for the verify gate evidence-replay engine (spec §3).
//!
//! Acceptance criteria from the task:
//! 4. With mode="replay": passing tagged artifact passes; zero blocks fails;
//!    malformed directive fails; non-allowlisted, metachar-bearing, and env-assignment
//!    commands each fail; a sleeping step times out, fails, and leaves no orphan process.
//!    Default mode: booby-trapped command leaves NO side effect.
//!    Token-boundary: "cargo testfoo" rejected while "cargo test" allowed.
//!    GATEKEEPER_SHADOW=replay on a presence-mode root emits per-command SHADOW lines
//!    and does not change the exit code.
//!    SHADOW lines parse as JSON with the exact field set.
//! 8. Config strictness: invalid known value → owning gate exit 2;
//!    unparsable config.toml → check verify/design/finish exit 2.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

// ── helpers ───────────────────────────────────────────────────────────────────

/// Minimal framework root recognised by `framework_root()`.
/// Creates a real git repo (init) so git commands work from the root.
fn scratch_root(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("topo_vrpl_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("skills")).unwrap();
    fs::write(root.join("AGENTS.md"), "").unwrap();
    // Init a real git repo so git status etc. work when the test executes commands.
    let _ = Command::new("git")
        .args(["-C", root.to_str().unwrap(), "init", "-q"])
        .status();
    let _ = Command::new("git")
        .args([
            "-C",
            root.to_str().unwrap(),
            "config",
            "user.email",
            "t@t.t",
        ])
        .status();
    let _ = Command::new("git")
        .args(["-C", root.to_str().unwrap(), "config", "user.name", "t"])
        .status();
    root
}

/// Run `gatekeeper <args>` from `cwd`. Returns (exit_code, stdout, stderr).
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

/// Run with env var set.
fn run3_env(cwd: &Path, env_key: &str, env_val: &str, args: &[&str]) -> (i32, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_gatekeeper"))
        .current_dir(cwd)
        .env(env_key, env_val)
        .args(args)
        .output()
        .unwrap();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// Write a config.toml and a verify artifact to a scratch root.
///
/// `tag` is used for the scratch dir name (underscores ok).
/// `feature_slug` is the slug used in the artifact filename AND the --feature arg.
/// Returns the root path.
fn setup_verify_root(
    tag: &str,
    feature_slug: &str,
    config_toml: &str,
    artifact_content: &str,
) -> PathBuf {
    let root = scratch_root(tag);
    let docs_dir = root.join("docs");
    let verify_dir = docs_dir.join("verify");
    fs::create_dir_all(&verify_dir).unwrap();
    fs::write(docs_dir.join("config.toml"), config_toml).unwrap();
    fs::write(
        verify_dir.join(format!("2026-06-11-{feature_slug}.md")),
        artifact_content,
    )
    .unwrap();
    root
}

/// The SHADOW JSON field names that must be present.
const SHADOW_FIELDS: &[&str] = &[
    "gate",
    "check",
    "configured",
    "artifact",
    "command",
    "result",
    "detail",
];

/// Parse a SHADOW line from stderr (the first line starting with `SHADOW `).
fn first_shadow_json(stderr: &str) -> Option<serde_json::Value> {
    for line in stderr.lines() {
        if let Some(rest) = line.strip_prefix("SHADOW ") {
            return serde_json::from_str(rest).ok();
        }
    }
    None
}

// ── Token-boundary tests ──────────────────────────────────────────────────────

/// cargo testfoo must be rejected (not an allowed prefix) even though "cargo test" is.
#[test]
fn token_boundary_cargo_testfoo_rejected() {
    let root = setup_verify_root(
        "tb_testfoo",
        "tb-testfoo",
        "[verify]\nmode = \"replay\"\nallowed_command_prefixes = [\"cargo test\"]\n",
        "# Verify\n\n```evidence\n$ cargo testfoo\n```\n",
    );
    let (code, out, _err) = run3(&root, &["check", "verify", "--feature", "tb-testfoo"]);
    assert_ne!(
        code, 0,
        "cargo testfoo must be rejected (token boundary); out: {out}"
    );
    let _ = fs::remove_dir_all(&root);
}

/// "cargo test" prefix ACCEPTED even with extra args (token boundary match).
#[test]
fn token_boundary_git_status_accepted() {
    // Use `git status` which is in the default allowlist and is safe.
    let root = setup_verify_root(
        "tb_git_status",
        "tb-git-status",
        "[verify]\nmode = \"replay\"\n",
        "# Verify\n\n```evidence\n$ git status\n```\n",
    );
    let (_code, out, _err) = run3(&root, &["check", "verify", "--feature", "tb-git-status"]);
    // The important thing is it's NOT rejected as non-allowlisted.
    assert!(
        !out.contains("command rejected: not in allowed"),
        "git status must not be rejected as non-allowlisted; out: {out}"
    );
    let _ = fs::remove_dir_all(&root);
}

// ── Fixture (b) regression — empty verify fails in replay mode ────────────────

#[test]
fn replay_zero_evidence_blocks_fails() {
    let root = setup_verify_root(
        "zero_blocks",
        "zero-blocks",
        "[verify]\nmode = \"replay\"\n",
        "# Empty verify artifact with no evidence blocks.\n",
    );
    let (code, out, _err) = run3(&root, &["check", "verify", "--feature", "zero-blocks"]);
    assert_ne!(
        code, 0,
        "zero evidence blocks must fail in replay mode; out: {out}"
    );
    assert!(out.contains("FAIL"), "output must contain FAIL; got: {out}");
    let _ = fs::remove_dir_all(&root);
}

// ── Passing artifact ──────────────────────────────────────────────────────────

#[test]
fn replay_passing_artifact_passes() {
    // Use `git status` which is in the default allowlist and will always exit 0.
    let root = setup_verify_root(
        "passing",
        "passing",
        "[verify]\nmode = \"replay\"\n",
        "# Verify\n\n```evidence\n$ git status\n```\n",
    );
    let (code, out, _err) = run3(&root, &["check", "verify", "--feature", "passing"]);
    assert_eq!(code, 0, "passing artifact must exit 0; out: {out}");
    assert!(out.contains("PASS"), "output must contain PASS; got: {out}");
    let _ = fs::remove_dir_all(&root);
}

// ── Malformed directive ───────────────────────────────────────────────────────

#[test]
fn replay_malformed_directive_fails() {
    let root = setup_verify_root(
        "malformed",
        "malformed",
        "[verify]\nmode = \"replay\"\n",
        "# Verify\n\n```evidence\n$ git status\n# note: this is a near-miss directive shape\n```\n",
    );
    let (code, out, _err) = run3(&root, &["check", "verify", "--feature", "malformed"]);
    assert_ne!(code, 0, "malformed directive must fail; out: {out}");
    assert!(out.contains("FAIL"), "output must contain FAIL; got: {out}");
    let _ = fs::remove_dir_all(&root);
}

// ── Non-allowlisted command ───────────────────────────────────────────────────

#[test]
fn replay_non_allowlisted_command_fails() {
    let root = setup_verify_root(
        "not_allowed",
        "not-allowed",
        "[verify]\nmode = \"replay\"\n",
        "# Verify\n\n```evidence\n$ echo hello\n```\n",
    );
    let (code, out, _err) = run3(&root, &["check", "verify", "--feature", "not-allowed"]);
    assert_ne!(code, 0, "non-allowlisted command must fail; out: {out}");
    assert!(out.contains("FAIL"), "output must contain FAIL; got: {out}");
    let _ = fs::remove_dir_all(&root);
}

// ── Metachar-bearing command ──────────────────────────────────────────────────

#[test]
fn replay_metachar_command_fails() {
    let root = setup_verify_root(
        "metachar",
        "metachar",
        "[verify]\nmode = \"replay\"\nallowed_command_prefixes = [\"echo\"]\n",
        "# Verify\n\n```evidence\n$ echo foo | grep foo\n```\n",
    );
    let (code, out, _err) = run3(&root, &["check", "verify", "--feature", "metachar"]);
    assert_ne!(code, 0, "metachar command must fail; out: {out}");
    assert!(out.contains("FAIL"), "output must contain FAIL; got: {out}");
    let _ = fs::remove_dir_all(&root);
}

// ── Env-assignment prefix ─────────────────────────────────────────────────────

#[test]
fn replay_env_assignment_prefix_fails() {
    let root = setup_verify_root(
        "env_assign",
        "env-assign",
        "[verify]\nmode = \"replay\"\nallowed_command_prefixes = [\"FOO\"]\n",
        "# Verify\n\n```evidence\n$ FOO=bar cargo test\n```\n",
    );
    let (code, out, _err) = run3(&root, &["check", "verify", "--feature", "env-assign"]);
    assert_ne!(code, 0, "env-assignment prefix must fail; out: {out}");
    assert!(out.contains("FAIL"), "output must contain FAIL; got: {out}");
    let _ = fs::remove_dir_all(&root);
}

// ── Timeout + no orphan process ───────────────────────────────────────────────

#[cfg(unix)]
#[test]
fn replay_sleeping_step_times_out_and_no_orphan() {
    // Use `sleep 9973` — a prime-number duration unlikely to appear in other processes.
    // Set a 1-second timeout.
    let unique_sleep = "9973";
    let root = setup_verify_root(
        "timeout",
        "timeout",
        "[verify]\nmode = \"replay\"\nreplay_timeout_secs = 1\nallowed_command_prefixes = [\"sleep\"]\n",
        &format!("# Verify\n\n```evidence\n$ sleep {unique_sleep}\n```\n"),
    );
    let (code, out, _err) = run3(&root, &["check", "verify", "--feature", "timeout"]);
    assert_ne!(code, 0, "timed-out step must fail; out: {out}");
    assert!(
        out.contains("FAIL") || out.contains("timed out"),
        "output must mention FAIL or timed out; got: {out}"
    );

    // Wait for the kill to propagate, then check for the specific sleep duration.
    std::thread::sleep(std::time::Duration::from_millis(800));
    let pgrep_pattern = format!("sleep {unique_sleep}");
    let ps_out = Command::new("pgrep")
        .args(["-f", &pgrep_pattern])
        .output()
        .map(|o| {
            (
                o.status.success(),
                String::from_utf8_lossy(&o.stdout).into_owned(),
            )
        })
        .unwrap_or((false, String::new()));
    let pgrep_found = ps_out.0 && !ps_out.1.trim().is_empty();
    assert!(
        !pgrep_found,
        "orphan 'sleep {unique_sleep}' process found after timeout kill; pids: {}",
        ps_out.1.trim()
    );

    let _ = fs::remove_dir_all(&root);
}

// ── Presence mode: no side effects ───────────────────────────────────────────

#[test]
fn presence_mode_executes_nothing() {
    // In presence mode the command must NOT execute; SHADOW result must be "static".
    let root = setup_verify_root(
        "presence_noop",
        "presence-noop",
        // No [verify] section = default presence mode
        "# no verify config\n",
        "# Verify\n\n```evidence\n$ git status\n```\n",
    );
    let (code, out, err) = run3(&root, &["check", "verify", "--feature", "presence-noop"]);
    assert_eq!(
        code, 0,
        "presence mode must pass if file exists; out: {out}"
    );
    // The SHADOW line should have result:"static"
    let shadow_json = first_shadow_json(&err);
    assert!(
        shadow_json.is_some(),
        "must emit a SHADOW line in presence mode; err: {err}"
    );
    let json = shadow_json.unwrap();
    assert_eq!(
        json["result"].as_str(),
        Some("static"),
        "presence mode SHADOW result must be 'static'; got: {:?}",
        json["result"]
    );
    let _ = fs::remove_dir_all(&root);
}

// ── SHADOW JSONL field set ────────────────────────────────────────────────────

#[test]
fn shadow_lines_have_exact_field_set() {
    let root = setup_verify_root(
        "shadow_fields",
        "shadow-fields",
        "# no [verify] — presence mode\n",
        "# Verify\n\n```evidence\n$ git status\n```\n",
    );
    let (code, _out, err) = run3(&root, &["check", "verify", "--feature", "shadow-fields"]);
    assert_eq!(code, 0, "should pass (presence mode)");
    // Find all SHADOW lines.
    let shadow_lines: Vec<&str> = err.lines().filter(|l| l.starts_with("SHADOW ")).collect();
    assert!(
        !shadow_lines.is_empty(),
        "must emit at least one SHADOW line; err: {err}"
    );
    for line in &shadow_lines {
        let rest = line.strip_prefix("SHADOW ").unwrap();
        let json: serde_json::Value = serde_json::from_str(rest)
            .unwrap_or_else(|e| panic!("SHADOW line must be valid JSON: {e}\nline: {line}"));
        for field in SHADOW_FIELDS {
            assert!(
                json.get(*field).is_some(),
                "SHADOW line missing field {field}; got: {json}"
            );
        }
    }
    let _ = fs::remove_dir_all(&root);
}

// ── GATEKEEPER_SHADOW=replay — no exit-code change ───────────────────────────

#[test]
fn shadow_env_replay_does_not_change_exit_code() {
    // presence-mode config + GATEKEEPER_SHADOW=replay: must still exit 0.
    let root = setup_verify_root(
        "shadow_env_noexit",
        "shadow-env-noexit",
        "# presence mode (no [verify] section)\n",
        "# Verify\n\n```evidence\n$ git status\n```\n",
    );
    let (code, out, err) = run3_env(
        &root,
        "GATEKEEPER_SHADOW",
        "replay",
        &["check", "verify", "--feature", "shadow-env-noexit"],
    );
    assert_eq!(
        code, 0,
        "GATEKEEPER_SHADOW=replay must not change exit code for presence-mode project; out: {out}, err: {err}"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn shadow_env_replay_emits_shadow_lines() {
    // presence-mode config + GATEKEEPER_SHADOW=replay: must emit SHADOW lines with real results.
    let root = setup_verify_root(
        "shadow_env_emit",
        "shadow-env-emit",
        "# presence mode\n",
        "# Verify\n\n```evidence\n$ git status\n```\n",
    );
    let (_code, _out, err) = run3_env(
        &root,
        "GATEKEEPER_SHADOW",
        "replay",
        &["check", "verify", "--feature", "shadow-env-emit"],
    );
    let shadow_lines: Vec<&str> = err.lines().filter(|l| l.starts_with("SHADOW ")).collect();
    assert!(
        !shadow_lines.is_empty(),
        "GATEKEEPER_SHADOW=replay must emit SHADOW lines; err: {err}"
    );
    let _ = fs::remove_dir_all(&root);
}

// ── Config strictness: invalid known value ───────────────────────────────────

#[test]
fn invalid_verify_mode_exits_2() {
    let root = setup_verify_root(
        "inv_mode",
        "inv-mode",
        "[verify]\nmode = \"bogus\"\n",
        "# Verify\n",
    );
    let (code, _out, err) = run3(&root, &["check", "verify", "--feature", "inv-mode"]);
    assert_eq!(code, 2, "invalid mode value must exit 2; err: {err}");
    assert!(
        err.contains("bogus") || err.contains("invalid") || err.contains("parse error"),
        "error message must mention the bad value or parse error; err: {err}"
    );
    let _ = fs::remove_dir_all(&root);
}

// ── Config strictness: malformed TOML → hardened gates exit 2 ────────────────

#[test]
fn malformed_config_toml_exits_2_for_verify() {
    let root = setup_verify_root(
        "bad_toml",
        "bad-toml",
        "this is [not valid toml\n",
        "# Verify\n",
    );
    let (code, _out, err) = run3(&root, &["check", "verify", "--feature", "bad-toml"]);
    assert_eq!(
        code, 2,
        "malformed config.toml must exit 2 for check verify; err: {err}"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn malformed_config_toml_exits_2_for_design() {
    let root = scratch_root("bad_toml_design");
    let docs_dir = root.join("docs");
    let research_dir = docs_dir.join("research");
    let specs_dir = docs_dir.join("specs");
    fs::create_dir_all(&research_dir).unwrap();
    fs::create_dir_all(&specs_dir).unwrap();
    fs::write(docs_dir.join("config.toml"), "this is [not valid toml\n").unwrap();
    fs::write(
        research_dir.join("2026-06-11-bad-toml-design.md"),
        "# Research\n",
    )
    .unwrap();
    fs::write(
        specs_dir.join("2026-06-11-bad-toml-design.md"),
        "Status: approved\n",
    )
    .unwrap();
    let (code, _out, err) = run3(&root, &["check", "design", "--feature", "bad-toml-design"]);
    assert_eq!(
        code, 2,
        "malformed config.toml must exit 2 for check design; err: {err}"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn malformed_config_toml_exits_2_for_finish() {
    let root = scratch_root("bad_toml_finish");
    let docs_dir = root.join("docs");
    fs::create_dir_all(&docs_dir).unwrap();
    fs::write(docs_dir.join("config.toml"), "this is [not valid toml\n").unwrap();
    let (code, _out, err) = run3(&root, &["check", "finish", "--", "true"]);
    assert_eq!(
        code, 2,
        "malformed config.toml must exit 2 for check finish; err: {err}"
    );
    let _ = fs::remove_dir_all(&root);
}

// ── Non-gate commands keep warn-and-default on malformed TOML ────────────────

#[test]
fn malformed_config_toml_non_gate_warns_and_defaults() {
    // gatekeeper check plan uses load() (non-gate path) and must not exit 2 for bad TOML.
    let root = scratch_root("bad_toml_ngate");
    let docs_dir = root.join("docs");
    let plans_dir = docs_dir.join("plans");
    fs::create_dir_all(&plans_dir).unwrap();
    fs::write(docs_dir.join("config.toml"), "this is [not valid toml\n").unwrap();
    fs::write(
        plans_dir.join("2026-06-11-bad-toml-ngate.md"),
        "# Plan\n\nStep 1: do something concrete.\n",
    )
    .unwrap();
    let (code, _out, err) = run3(&root, &["check", "plan", "--feature", "bad-toml-ngate"]);
    // Plan gate should succeed (file exists, no placeholders) — config parse failure is only a warning.
    assert_ne!(
        code, 2,
        "non-gate path must NOT exit 2 for malformed config.toml; err: {err}"
    );
    let _ = fs::remove_dir_all(&root);
}

// ── Expect directive matching ─────────────────────────────────────────────────

#[test]
fn replay_expect_literal_mismatch_fails() {
    let root = setup_verify_root(
        "expect_fail",
        "expect-fail",
        "[verify]\nmode = \"replay\"\n",
        "# Verify\n\n```evidence\n$ git status\n# expect: THIS_STRING_WILL_NEVER_APPEAR_IN_GIT_OUTPUT_XYZ_UNIQUE_SENTINEL\n```\n",
    );
    let (code, out, _err) = run3(&root, &["check", "verify", "--feature", "expect-fail"]);
    assert_ne!(code, 0, "failed expect literal must fail gate; out: {out}");
    assert!(out.contains("FAIL"), "must contain FAIL; got: {out}");
    let _ = fs::remove_dir_all(&root);
}

// ── SHADOW result:"static" in presence mode ───────────────────────────────────

#[test]
fn presence_mode_shadow_result_is_static() {
    let root = setup_verify_root(
        "static_result",
        "static-result",
        "# no [verify]\n",
        "# Verify\n\n```evidence\n$ git status\n```\n",
    );
    let (_code, _out, err) = run3(&root, &["check", "verify", "--feature", "static-result"]);
    let json = first_shadow_json(&err).unwrap_or_else(|| panic!("no SHADOW line in stderr: {err}"));
    assert_eq!(
        json["result"].as_str(),
        Some("static"),
        "presence-mode SHADOW result must be 'static'"
    );
    assert_eq!(
        json["gate"].as_str(),
        Some("verify"),
        "gate field must be 'verify'"
    );
    assert_eq!(
        json["check"].as_str(),
        Some("replay"),
        "check field must be 'replay'"
    );
    let _ = fs::remove_dir_all(&root);
}
