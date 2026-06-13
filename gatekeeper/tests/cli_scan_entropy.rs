//! Entropy detection lane (Task 4) + `[scan] exclude_paths` (Task 5) — integration tests.
//!
//! Provenance: docs/plans/2026-06-13-entropy-scanner.md (Tasks 4 & 5) and
//! docs/specs/2026-06-13-entropy-scanner.md.
//!
//! RED BY DESIGN at commit 5007044: entropy rules are PARSED but not yet APPLIED during scanning,
//! and `[scan] exclude_paths` does not exist. So the detection tests (1–2, 4) observe no `WARN `
//! line where one is expected, and the exclude tests (6, 8) cannot pass until both the entropy
//! lane (Task 4) and the exclude wiring (Task 5) land.
//!
//! Rules-injection mechanism (confirmed by reading gatekeeper/src/main.rs::resolve_root and
//! gatekeeper/tests/cli_scan.rs): `gatekeeper scan` reads `<framework_root>/security/rules.toml`.
//! The framework root is resolved with `$TOPOLOGY_ROOT` taking precedence (step 1) over the
//! binary-adjacent walk (step 3) that would otherwise point at the real topology repo (whose
//! rules.toml is still schema 1 with no entropy rules). So each test pins `TOPOLOGY_ROOT` to a
//! scratch root carrying its OWN schema-2 rules.toml — exactly as cli_scan.rs does. The scratch
//! root needs a `skills/` marker (so it is a "marked root") and `security/rules.toml`.
//!
//! Detection lives on stderr: `report()` prints `BLOCK <id>: …` / `WARN <id>: …`, and warn
//! findings never flip the exit code (severity = warn = shadow, exit 0).
//!
//! Hygiene: the high-entropy tokens here are assembled from shorter pieces at runtime, so this
//! file contains no secret-shaped literal run of ≥20 chars of [A-Za-z0-9+/=_-] (same posture as
//! cli_scan_bench.rs and cli_scan.rs).

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

// ── Scratch-root + runner harness (mirrors cli_scan.rs) ───────────────────────

/// A schema-2 rules.toml carrying a hex entropy rule (warn). `min_length = 32`, `threshold = 3.0`
/// — a 64-char random-hex token (~4.0 bits/char) clears it; ordinary prose tokens do not.
const RULES_HEX_ENTROPY: &str = "schema_version = 2\n\
    [[rule]]\n\
    id = \"hex-high-entropy\"\n\
    kind = \"entropy\"\n\
    severity = \"warn\"\n\
    description = \"high-entropy hex run\"\n\
    charset = \"hex\"\n\
    min_length = 32\n\
    threshold_bits_per_char = 3.0\n";

/// A schema-2 rules.toml carrying a base64 entropy rule (warn). `min_length = 20`,
/// `threshold = 4.5` — a 40-char base64 token (~5+ bits/char) clears it.
const RULES_BASE64_ENTROPY: &str = "schema_version = 2\n\
    [[rule]]\n\
    id = \"base64-high-entropy\"\n\
    kind = \"entropy\"\n\
    severity = \"warn\"\n\
    description = \"high-entropy base64 run\"\n\
    charset = \"base64\"\n\
    min_length = 20\n\
    threshold_bits_per_char = 4.5\n";

/// Build a marked framework root carrying the given `rules.toml` text.
fn scratch_root(tag: &str, rules_toml: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("topo_scan_entropy_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("skills")).unwrap();
    fs::create_dir_all(root.join("security")).unwrap();
    fs::write(root.join("security").join("rules.toml"), rules_toml).unwrap();
    root
}

/// Run `gatekeeper <args>` from `cwd` with `TOPOLOGY_ROOT` pinned to the (canonicalized) scratch
/// root, feeding `stdin`. Returns (exit code, stderr). Stderr is the channel for findings.
fn run(cwd: &Path, args: &[&str], stdin: &[u8]) -> (i32, String) {
    let canonical_cwd = fs::canonicalize(cwd).unwrap_or_else(|_| cwd.to_path_buf());
    let mut child = Command::new(env!("CARGO_BIN_EXE_gatekeeper"))
        .current_dir(cwd)
        .args(args)
        .env("TOPOLOGY_ROOT", &canonical_cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // Tolerate EPIPE: a child that rejects its args may close stdin before we finish writing.
    let mut child_stdin = child.stdin.take().unwrap();
    if let Err(e) = child_stdin.write_all(stdin) {
        if e.kind() != std::io::ErrorKind::BrokenPipe {
            panic!("failed to write child stdin: {e}");
        }
    }
    drop(child_stdin);
    let out = child.wait_with_output().unwrap();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// True iff a `WARN ` finding line names `rule_id` (the second whitespace token, `<id>:`).
fn warned(stderr: &str, rule_id: &str) -> bool {
    stderr
        .lines()
        .filter(|l| l.starts_with("WARN "))
        .any(|l| l.split_whitespace().nth(1).map(|t| t.trim_end_matches(':')) == Some(rule_id))
}

/// True iff a `BLOCK ` finding line names `rule_id`.
fn blocked(stderr: &str, rule_id: &str) -> bool {
    stderr
        .lines()
        .filter(|l| l.starts_with("BLOCK "))
        .any(|l| l.split_whitespace().nth(1).map(|t| t.trim_end_matches(':')) == Some(rule_id))
}

/// A 64-char uniform-random-looking hex token, assembled from pieces (no literal run in source).
fn hex64() -> String {
    format!(
        "{}{}{}{}",
        "9f86d081884c7d65", "9a2feaa0c55ad015", "a3bf4f1b2b0b822c", "d15d6c15b0f00a08"
    )
}

/// A 40-char base64 token, assembled from pieces.
fn base64_40() -> String {
    format!("{}{}", "Zm9vYmFyYmF6cXV4Y29y", "Z2VncmF1bHQxMjM0NTY3OA")
}

// ── Task 4 — entropy detection lane (severity = warn) ─────────────────────────

#[test]
fn entropy_flags_unlabeled_hex64() {
    let root = scratch_root("hex64", RULES_HEX_ENTROPY);
    let token = hex64();
    let (code, stderr) = run(
        &root,
        &["scan", "--content"],
        format!("{token}\n").as_bytes(),
    );
    assert!(
        warned(&stderr, "hex-high-entropy"),
        "a bare 64-char hex token must produce a WARN naming `hex-high-entropy`.\nstderr:\n{stderr}"
    );
    assert_eq!(
        code, 0,
        "a warn-severity entropy hit must NOT block (exit 0)"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn entropy_flags_unlabeled_base64_40() {
    let root = scratch_root("base64", RULES_BASE64_ENTROPY);
    let token = base64_40();
    let (code, stderr) = run(
        &root,
        &["scan", "--content"],
        format!("{token}\n").as_bytes(),
    );
    assert!(
        warned(&stderr, "base64-high-entropy"),
        "a 40-char base64 token must produce a WARN naming `base64-high-entropy`.\nstderr:\n{stderr}"
    );
    assert_eq!(
        code, 0,
        "a warn-severity entropy hit must NOT block (exit 0)"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn entropy_ignores_low_entropy_text() {
    let root = scratch_root("prose", RULES_HEX_ENTROPY);
    // Ordinary English broken into short, low-entropy tokens — none should clear the threshold,
    // and none is a long enough hex run to even be a candidate.
    let (code, stderr) = run(
        &root,
        &["scan", "--content"],
        b"the quick brown fox jumps over the lazy dog near the river bank\n",
    );
    assert!(
        !warned(&stderr, "hex-high-entropy"),
        "ordinary prose must NOT produce an entropy WARN.\nstderr:\n{stderr}"
    );
    assert_eq!(code, 0, "clean prose exits 0");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn entropy_never_blocks() {
    // Even when the entropy rule fires, a warn-severity finding must never flip the exit code.
    let root = scratch_root("never_block", RULES_HEX_ENTROPY);
    let token = hex64();
    let (code, stderr) = run(
        &root,
        &["scan", "--content"],
        format!("{token}\n").as_bytes(),
    );
    assert!(
        warned(&stderr, "hex-high-entropy"),
        "precondition: the entropy rule must fire for this test to be meaningful.\nstderr:\n{stderr}"
    );
    assert_eq!(
        code, 0,
        "entropy rule at severity=warn must exit 0 even when it fires"
    );
    let _ = fs::remove_dir_all(&root);
}

// ── Task 5 — [scan] exclude_paths (path-bearing lanes only) ───────────────────

/// A v2 rules.toml with a hex entropy rule, an AWS block rule, and `[scan] exclude_paths`.
/// The AWS rule lets us prove excludes scope entropy ONLY — regex block rules still fire.
const RULES_ENTROPY_AWS_EXCLUDE: &str = "schema_version = 2\n\
    [[rule]]\n\
    id = \"hex-high-entropy\"\n\
    kind = \"entropy\"\n\
    severity = \"warn\"\n\
    description = \"high-entropy hex run\"\n\
    charset = \"hex\"\n\
    min_length = 32\n\
    threshold_bits_per_char = 3.0\n\
    [[rule]]\n\
    id = \"aws-access-key-id\"\n\
    kind = \"content\"\n\
    severity = \"block\"\n\
    description = \"AWS access key id\"\n\
    pattern = '\\b(AKIA|ASIA)[0-9A-Z]{16}\\b'\n\
    [scan]\n\
    exclude_paths = [\"*.lock\"]\n\
    [integrity]\n\
    protected_paths = []\n";

fn git(root: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .status()
        .unwrap()
        .success();
    assert!(ok, "git {args:?} failed");
}

/// scratch_root() + git init + an initial commit, so staging operations have a HEAD.
fn git_root(tag: &str, rules_toml: &str) -> PathBuf {
    let root = scratch_root(tag, rules_toml);
    git(&root, &["init", "-q", "-b", "main"]);
    git(&root, &["config", "user.email", "t@t.t"]);
    git(&root, &["config", "user.name", "t"]);
    git(&root, &["add", "."]);
    git(&root, &["commit", "-q", "-m", "init"]);
    root
}

/// An AWS-shaped key built by concatenation, so this test file never contains a literal key.
fn planted_aws_key() -> String {
    format!("AKIA{}", "1234567890ABCDEF")
}

#[test]
fn exclude_paths_suppresses_entropy_on_staged() {
    // A *.lock-matching path carrying a high-entropy token: the entropy lane must be suppressed
    // because the path is in exclude_paths. (Today there is no entropy lane and no excludes, so
    // this is red on the missing entropy WARN once the lane lands and red on the missing exclude.)
    let root = git_root("excl_entropy", RULES_ENTROPY_AWS_EXCLUDE);
    let token = hex64();
    fs::write(root.join("Cargo.lock"), format!("checksum = \"{token}\"\n")).unwrap();
    git(&root, &["add", "Cargo.lock"]);
    let (code, stderr) = run(&root, &["scan", "--staged"], b"");
    assert!(
        !warned(&stderr, "hex-high-entropy"),
        "a *.lock-excluded path must NOT produce an entropy WARN.\nstderr:\n{stderr}"
    );
    assert_eq!(code, 0, "no block expected for an excluded lock file");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn exclude_paths_does_not_suppress_regex_rules() {
    // The same *.lock-excluded path also carries a labeled AWS key. The exclude scopes ENTROPY
    // only — the regex block rule must still fire and block.
    let root = git_root("excl_regex", RULES_ENTROPY_AWS_EXCLUDE);
    let key = planted_aws_key();
    fs::write(
        root.join("Cargo.lock"),
        format!("aws_access_key_id = {key}\n"),
    )
    .unwrap();
    git(&root, &["add", "Cargo.lock"]);
    let (code, stderr) = run(&root, &["scan", "--staged"], b"");
    assert!(
        blocked(&stderr, "aws-access-key-id"),
        "exclude_paths must NOT suppress regex block rules — the AWS key must still BLOCK.\nstderr:\n{stderr}"
    );
    assert_eq!(
        code, 1,
        "a blocking regex rule on an excluded path still blocks"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn exclude_paths_not_applied_on_content() {
    // `scan --content` carries no path, so a path-based exclude cannot apply — the entropy rule
    // must still fire. (Red today because the entropy lane does not exist; once it lands, the
    // exclude must NOT spuriously swallow the no-path case.)
    let root = scratch_root("excl_content", RULES_ENTROPY_AWS_EXCLUDE);
    let token = hex64();
    let (code, stderr) = run(
        &root,
        &["scan", "--content"],
        format!("{token}\n").as_bytes(),
    );
    assert!(
        warned(&stderr, "hex-high-entropy"),
        "exclude_paths must not apply to --content (no path) — entropy must still fire.\nstderr:\n{stderr}"
    );
    assert_eq!(code, 0, "warn-severity entropy hit exits 0");
    let _ = fs::remove_dir_all(&root);
}
