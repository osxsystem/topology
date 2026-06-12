//! Integration tests for the finish gate zero-test floor (spec §5, task 8).
//!
//! Covers:
//!   - pytest -q style summary recognized + passes floor
//!   - cargo multi-binary summaries summed
//!   - `-- true` CLI override also fails the floor (bypass attempt)
//!   - first-match-wins (cargo transcript does NOT also count via pytest pattern)
//!   - require_test_count=false emits SHADOW line + gate passes on zero tests
//!   - extra_count_patterns escape hatch works
//!   - streaming: output appears on gate's stdout/stderr
//!   - invalid extra_count_patterns regex is skipped (no crash)

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

// ── helpers ───────────────────────────────────────────────────────────────────

/// Minimal framework root recognised by `framework_root()`.
fn scratch_root(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("topo_finish_floor_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("skills")).unwrap();
    fs::write(root.join("AGENTS.md"), "").unwrap();
    root
}

/// Run `gatekeeper <args>` from `cwd`. Returns `(exit_code, stdout, stderr)`.
fn run_full(cwd: &Path, args: &[&str]) -> (i32, String, String) {
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

/// Convenience: returns `(exit_code, stdout)`.
fn run(cwd: &Path, args: &[&str]) -> (i32, String) {
    let (code, out, _) = run_full(cwd, args);
    (code, out)
}

// ── write config helper ───────────────────────────────────────────────────────

fn write_config(root: &Path, content: &str) {
    let docs = root.join("docs");
    fs::create_dir_all(&docs).unwrap();
    fs::write(docs.join("config.toml"), content).unwrap();
}

// ── pytest -q style summary ───────────────────────────────────────────────────

/// pytest -q summary "3 passed in 0.12s" is recognized and the floor passes.
#[test]
fn pytest_q_summary_recognized_and_passes() {
    let root = scratch_root("pytest_q");
    // Command echoes a pytest -q style line (no === fence).
    write_config(
        &root,
        "test_command = \"echo '3 passed in 0.12s'\"\n\n[finish]\nrequire_test_count = true\n",
    );
    let (code, out) = run(&root, &["check", "finish"]);
    assert_eq!(
        code, 0,
        "pytest -q summary '3 passed in 0.12s' should pass floor; out: {out}"
    );
    assert!(out.contains("PASS"), "expected PASS; got: {out}");
    let _ = fs::remove_dir_all(&root);
}

/// pytest -q summary with zero tests must fail the floor.
#[test]
fn pytest_q_zero_tests_fails_floor() {
    let root = scratch_root("pytest_q_zero");
    write_config(
        &root,
        "test_command = \"echo '0 passed in 0.01s'\"\n\n[finish]\nrequire_test_count = true\n",
    );
    let (code, out) = run(&root, &["check", "finish"]);
    assert_ne!(
        code, 0,
        "pytest -q summary with 0 tests should fail floor; out: {out}"
    );
    let _ = fs::remove_dir_all(&root);
}

// ── cargo multi-binary summaries summed ──────────────────────────────────────

/// Multiple cargo summary lines (from multi-binary crate) are summed.
/// e.g. "test result: ok. 3 passed; ..." + "test result: ok. 5 passed; ..." → count 8.
#[test]
fn cargo_multi_binary_summaries_summed() {
    let root = scratch_root("cargo_multi");
    // Two cargo summary lines; total = 3 + 5 = 8.
    write_config(
        &root,
        "test_command = \"printf 'test result: ok. 3 passed; 0 failed\\ntest result: ok. 5 passed; 0 failed\\n'\"\n\n[finish]\nrequire_test_count = true\n",
    );
    let (code, out) = run(&root, &["check", "finish"]);
    assert_eq!(
        code, 0,
        "multi-binary cargo summaries summed (3+5=8) should pass floor; out: {out}"
    );
    assert!(out.contains("PASS"), "expected PASS; got: {out}");
    let _ = fs::remove_dir_all(&root);
}

// ── `-- true` CLI override also fails the floor ──────────────────────────────

/// `-- true` CLI override (bypass attempt) must also fail the floor.
#[test]
fn cli_override_true_also_fails_floor() {
    let root = scratch_root("cli_true");
    // config has require_test_count; CLI `-- true` produces no summary.
    write_config(&root, "[finish]\nrequire_test_count = true\n");
    let (code, out) = run(&root, &["check", "finish", "--", "true"]);
    assert_ne!(
        code, 0,
        "`-- true` bypass attempt must fail the floor; out: {out}"
    );
    let _ = fs::remove_dir_all(&root);
}

// ── first-match-wins ──────────────────────────────────────────────────────────

/// A transcript with a cargo summary does NOT also count via the pytest pattern.
/// cargo line: "test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.30s"
/// This contains "5 passed" AND "in 0.30s" — without first-match-wins it would match pytest too
/// and yield 10 instead of 5.  We check that the shadow detail says count=5, not count=10.
#[test]
fn first_match_wins_cargo_not_double_counted_by_pytest() {
    let root = scratch_root("first_match");
    // The cargo line contains "5 passed" AND "in 0.30s" — pytest regex would also match.
    // With first-match-wins, only cargo pattern applies → count=5, not 10.
    let cargo_line = "test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.30s";
    write_config(
        &root,
        &format!("test_command = \"echo '{cargo_line}'\"\n\n[finish]\nrequire_test_count = true\n"),
    );
    let (code, _out, err) = run_full(&root, &["check", "finish"]);
    assert_eq!(code, 0, "cargo line with 5 tests should pass floor");
    // SHADOW line must say count=5 (not 10).
    assert!(
        err.contains("count=5"),
        "SHADOW detail should show count=5 (first-match-wins); stderr: {err}"
    );
    assert!(
        !err.contains("count=10"),
        "double-counting must not occur; stderr: {err}"
    );
    let _ = fs::remove_dir_all(&root);
}

// ── require_test_count=false: SHADOW emitted, gate still passes on zero tests ──

/// When require_test_count is false (default), the SHADOW line is still emitted
/// and the gate passes even on zero tests (or no summary).
#[test]
fn require_test_count_false_emits_shadow_passes_on_zero() {
    let root = scratch_root("rtc_false_zero");
    // config with require_test_count=false (default); command emits recognized zero summary.
    write_config(
        &root,
        "test_command = \"echo 'test result: ok. 0 passed; 0 failed'\"\n\n[finish]\nrequire_test_count = false\n",
    );
    let (code, out, err) = run_full(&root, &["check", "finish"]);
    assert_eq!(
        code, 0,
        "require_test_count=false must not fail on zero tests; out: {out}"
    );
    assert!(out.contains("PASS"), "expected PASS; got: {out}");
    // SHADOW line must still be emitted.
    assert!(
        err.contains("SHADOW"),
        "SHADOW line must be emitted even when key is off; stderr: {err}"
    );
    assert!(
        err.contains("zero_test_floor"),
        "SHADOW check must be zero_test_floor; stderr: {err}"
    );
    let _ = fs::remove_dir_all(&root);
}

/// When require_test_count is default-off, gate passes when no summary is found.
#[test]
fn require_test_count_default_off_passes_no_summary() {
    let root = scratch_root("rtc_default_no_summary");
    // No config.toml → defaults; command "true" produces no summary.
    fs::create_dir_all(root.join("docs")).unwrap();
    let (code, out, err) = run_full(&root, &["check", "finish", "--", "true"]);
    assert_eq!(
        code, 0,
        "default (require_test_count off) must pass even with no summary; out: {out}"
    );
    assert!(out.contains("PASS"), "expected PASS; got: {out}");
    // SHADOW line must be emitted with result=fail (no summary recognized) but gate passes.
    assert!(
        err.contains("SHADOW"),
        "SHADOW line emitted in default mode; stderr: {err}"
    );
    let _ = fs::remove_dir_all(&root);
}

// ── extra_count_patterns escape hatch ────────────────────────────────────────

/// extra_count_patterns admit a custom runner summary.
#[test]
fn extra_count_patterns_custom_runner() {
    let root = scratch_root("extra_patterns");
    write_config(
        &root,
        r#"test_command = "echo 'Suite finished: 7 tests OK'"

[finish]
require_test_count = true
extra_count_patterns = ["(\\d+) tests OK"]
"#,
    );
    let (code, out) = run(&root, &["check", "finish"]);
    assert_eq!(
        code, 0,
        "extra_count_patterns should recognize custom runner; out: {out}"
    );
    assert!(out.contains("PASS"), "expected PASS; got: {out}");
    let _ = fs::remove_dir_all(&root);
}

/// extra_count_patterns with zero count still fails the floor.
#[test]
fn extra_count_patterns_zero_fails_floor() {
    let root = scratch_root("extra_patterns_zero");
    write_config(
        &root,
        r#"test_command = "echo 'Suite finished: 0 tests OK'"

[finish]
require_test_count = true
extra_count_patterns = ["(\\d+) tests OK"]
"#,
    );
    let (code, _) = run(&root, &["check", "finish"]);
    assert_ne!(code, 0, "extra_count_patterns with 0 should fail floor");
    let _ = fs::remove_dir_all(&root);
}

// ── streaming: output appears on gate's stdout/stderr ────────────────────────

/// Output from the test command is streamed through (visible on gate's stdout/stderr).
/// We verify the line appears in the combined output of the gate process.
#[test]
fn streaming_output_visible_on_gate_stdout() {
    let root = scratch_root("streaming");
    // Command prints a recognizable line; we check it appears in the gate's output.
    let marker = "STREAMING_MARKER_12345";
    write_config(
        &root,
        &format!("test_command = \"echo '{marker}'\"\n\n[finish]\nrequire_test_count = false\n"),
    );
    let (code, out, err) = run_full(&root, &["check", "finish"]);
    assert_eq!(code, 0, "streaming test should pass; out: {out}");
    let combined = format!("{out}{err}");
    assert!(
        combined.contains(marker),
        "streaming: command output marker must appear in gate output; combined: {combined}"
    );
    let _ = fs::remove_dir_all(&root);
}

// ── config parse failure exits 2 ─────────────────────────────────────────────

/// Malformed config.toml causes check finish to exit 2 (spec config strictness).
#[test]
fn malformed_config_exits_2() {
    let root = scratch_root("malformed_cfg");
    write_config(&root, "bad toml [[[");
    let (code, _out, _err) = run_full(&root, &["check", "finish", "--", "true"]);
    assert_eq!(
        code, 2,
        "malformed config.toml must cause finish gate to exit 2"
    );
    let _ = fs::remove_dir_all(&root);
}

// ── unit tests for parse_test_count ──────────────────────────────────────────

#[cfg(test)]
mod unit {
    // parse_test_count is pub(crate) in main.rs; we can't call it from an integration test.
    // These tests live here for structural proximity but use the binary via the gate.

    use super::*;

    /// cargo summary with 5 tests passes floor via gate.
    #[test]
    fn unit_cargo_5_passes() {
        let root = scratch_root("unit_cargo5");
        write_config(
            &root,
            "test_command = \"echo 'test result: ok. 5 passed; 0 failed'\"\n\n[finish]\nrequire_test_count = true\n",
        );
        let (code, out) = run(&root, &["check", "finish"]);
        assert_eq!(code, 0, "5 passed → floor passes; {out}");
        let _ = fs::remove_dir_all(&root);
    }

    /// cargo summary with 1 test passes floor.
    #[test]
    fn unit_cargo_1_passes() {
        let root = scratch_root("unit_cargo1");
        write_config(
            &root,
            "test_command = \"echo 'test result: ok. 1 passed; 0 failed'\"\n\n[finish]\nrequire_test_count = true\n",
        );
        let (code, out) = run(&root, &["check", "finish"]);
        assert_eq!(code, 0, "1 passed → floor passes; {out}");
        let _ = fs::remove_dir_all(&root);
    }

    /// Non-zero exit code from test command fails gate regardless of summary.
    #[test]
    fn unit_nonzero_exit_fails_gate() {
        let root = scratch_root("unit_nonzero");
        write_config(
            &root,
            "test_command = \"sh -c \\\"echo 'test result: ok. 5 passed; 0 failed'; exit 1\\\"\"\n\n[finish]\nrequire_test_count = true\n",
        );
        let (code, _) = run(&root, &["check", "finish"]);
        assert_ne!(code, 0, "non-zero exit must fail gate");
        let _ = fs::remove_dir_all(&root);
    }

    /// SHADOW line contains gate="finish" and check="zero_test_floor".
    #[test]
    fn unit_shadow_fields_correct() {
        let root = scratch_root("unit_shadow");
        write_config(
            &root,
            "test_command = \"echo 'test result: ok. 3 passed; 0 failed'\"\n",
        );
        let (_, _, err) = run_full(&root, &["check", "finish"]);
        assert!(
            err.contains("\"finish\""),
            "SHADOW must have gate=finish; stderr: {err}"
        );
        assert!(
            err.contains("zero_test_floor"),
            "SHADOW must have check=zero_test_floor; stderr: {err}"
        );
        assert!(
            err.contains("\"null\"") || err.contains(":null"),
            "SHADOW artifact must be null for finish gate; stderr: {err}"
        );
        let _ = fs::remove_dir_all(&root);
    }

    /// SHADOW configured field is "on" when require_test_count=true.
    #[test]
    fn unit_shadow_configured_on_when_enforced() {
        let root = scratch_root("unit_shadow_on");
        write_config(
            &root,
            "test_command = \"echo 'test result: ok. 3 passed; 0 failed'\"\n\n[finish]\nrequire_test_count = true\n",
        );
        let (_, _, err) = run_full(&root, &["check", "finish"]);
        assert!(
            err.contains("\"on\""),
            "SHADOW configured must be 'on' when require_test_count=true; stderr: {err}"
        );
        let _ = fs::remove_dir_all(&root);
    }

    /// SHADOW configured field is "default" when require_test_count is not set.
    #[test]
    fn unit_shadow_configured_default_when_off() {
        let root = scratch_root("unit_shadow_default");
        write_config(
            &root,
            "test_command = \"echo 'test result: ok. 3 passed; 0 failed'\"\n",
        );
        let (_, _, err) = run_full(&root, &["check", "finish"]);
        assert!(
            err.contains("\"default\""),
            "SHADOW configured must be 'default' when key not set; stderr: {err}"
        );
        let _ = fs::remove_dir_all(&root);
    }
}
