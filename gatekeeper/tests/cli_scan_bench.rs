//! Secrets-detection benchmark — the FM5 scoreboard.
//!
//! Provenance: docs/plans/2026-06-11-five-failure-modes-roadmap.md (Phase 0, step 4) and
//! docs/specs/2026-06-11-day-zero-containment.md (§2). Corpus contract:
//! tests/fixtures/secrets-bench/README.md.
//!
//! Measures the LIVE ruleset — `security/rules.toml` is copied verbatim into the scratch
//! root — against 11 positive classes (runtime-assembled) and 6 negative fixtures (literal
//! `*.txt` files). Detection is rule-attributed: an in-scope class passes only when one of
//! its *expected* rule ids appears in the findings, so a lucky overlap from the wrong rule
//! cannot satisfy the floor.
//!
//! History: red by design at v0.4.0 (floor 9/11, 5 covering rules); green once the Phase 0
//! rules landed. The entropy phase (schema v2) then flipped the two former
//! `in_scope: false` entropy classes (`hex64-unlabeled`, `base64-unlabeled`) to in-scope,
//! detected via `WARN ` from the shipped `hex-high-entropy` / `base64-high-entropy` rules.
//!
//! Path-aware lane: `[scan] exclude_paths` suppresses the entropy lane only on path-bearing
//! lanes (`--staged`, `--hook`). This bench drives `gatekeeper scan --content`, which carries
//! NO path, so excludes never apply here — high-entropy benign negatives (cargo-lock hashes,
//! git OIDs, base64 vectors) DO `WARN ` under `--content`, by design. Entropy ships `warn`
//! (shadow) precisely because it cannot distinguish a benign high-entropy blob from a real
//! secret; the false-positive rate is measured in burn-in, not asserted away here. Hence the
//! negative bench asserts only that no negative produces a `BLOCK ` (a warn is expected and
//! allowed); see docs/specs/2026-06-13-entropy-scanner.md ("Fundamental limit").
//!
//! Hygiene: no *secret-shaped* literal here is ≥20 consecutive chars of [A-Za-z0-9+/=_-];
//! long values are assembled from shorter pieces at runtime. (Identifiers are still ≥20-char
//! candidate runs for the planned Phase 2 entropy tokenizer — the guarantee is that no
//! candidate in this file carries high entropy, not that no candidate exists.)

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// A scratch framework root carrying a copy of the repository's real `security/rules.toml`,
/// so the bench measures the shipped ruleset rather than a test replica that can drift.
fn scratch_root(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("topo_scan_bench_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("skills")).unwrap();
    fs::create_dir_all(root.join("security")).unwrap();
    let live_rules = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("security")
        .join("rules.toml");
    fs::copy(&live_rules, root.join("security").join("rules.toml")).unwrap();
    root
}

/// Run `gatekeeper scan --content` from `cwd`, feeding `stdin`. Returns (exit code, stderr).
/// Stderr is the channel that matters here: warn-severity findings are reported there without
/// flipping the exit code, and `report()` prints every finding to stderr.
fn run_scan_content(cwd: &Path, stdin: &[u8]) -> (i32, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_gatekeeper"))
        .current_dir(cwd)
        .args(["scan", "--content"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // The child may exit before reading stdin, closing the pipe; tolerate EPIPE and assert on
    // the exit code and output instead (same posture as cli_scan.rs).
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

/// Rule ids parsed from `report()` finding lines (`BLOCK <id>: …` / `WARN <id>: …`). The
/// stderr clause is load-bearing for warn-severity rules, which never flip the exit code —
/// counting `WARN ` here is what lets the positives floor credit shadow entropy detections.
fn fired_rules(stderr: &str) -> Vec<String> {
    stderr
        .lines()
        .filter(|l| l.starts_with("BLOCK ") || l.starts_with("WARN "))
        .filter_map(|l| l.split_whitespace().nth(1))
        .map(|id| id.trim_end_matches(':').to_string())
        .collect()
}

/// Rule ids from BLOCK-severity finding lines only (`BLOCK <id>: …`). The negatives bench
/// asserts on this — a `WARN ` from the shadow entropy lane on a benign high-entropy blob is
/// expected, so the contract is "no negative BLOCKs", not "no negative fires".
fn blocked_rules(stderr: &str) -> Vec<String> {
    stderr
        .lines()
        .filter(|l| l.starts_with("BLOCK "))
        .filter_map(|l| l.split_whitespace().nth(1))
        .map(|id| id.trim_end_matches(':').to_string())
        .collect()
}

struct Case {
    id: &'static str,
    /// Whether this class is asserted (vs. scoreboard-only). All eleven classes are now
    /// in-scope: the two entropy classes (`hex64-unlabeled`, `base64-unlabeled`) flipped to
    /// `true` when the schema-v2 entropy rules shipped — they are detected via `WARN ` (shadow).
    in_scope: bool,
    /// Rule ids accepted as this class's detector (any one suffices; empty for out-of-scope
    /// classes). Encodes the spec's recommended rule ids — a rename at approval is a
    /// one-token edit here.
    expect_rules: &'static [&'static str],
    payload: String,
}

/// The eleven positive classes. Every payload is assembled from short pieces so that no
/// secret-shaped literal exists in this file (see the hygiene note in the module docs).
fn positives() -> Vec<Case> {
    let jwt_header = format!("eyJ{}{}", "hbGciOiJIUzI1NiIs", "InR5cCI6IkpXVCJ9");
    let jwt_payload = format!("eyJ{}{}", "zdWIiOiIxMjM0NTY3", "ODkwIiwiYWRtIjp0fQ");
    let jwt_sig = format!(
        "{}{}{}",
        "dBjftJeZ4CVP-mB92", "K27uhbUJU1p1r-wW1", "gFWFOEjXk"
    );
    let sk_tail = format!("{}{}", "T3BlbkFJa1b2c3d4", "e5f6g7h8");
    // Underscore-bearing tail: the shape a pure-alnum tail class would miss (spec §1).
    let ant_tail = format!("{}-{}_{}", "kT3xV9mWqR7v", "Z2pL5nY8cD4f", "H6jB1sM3kQ9z");
    let aws_id = format!("AKIA{}{}", "J5K2M4N6", "P8Q0R2S4");
    let aws_secret = format!("{}{}{}", "wXa9bRfT3mK7pLq2", "ZsVu5nHj8cDe1gYw", "AiBoCmDn");
    let pem_word = "KEY";
    vec![
        Case {
            id: "jwt-bearer",
            in_scope: true,
            expect_rules: &["jwt-structural"],
            payload: format!("Authorization: Bearer {jwt_header}.{jwt_payload}.{jwt_sig}\n"),
        },
        Case {
            id: "openai-sk-proj",
            in_scope: true,
            expect_rules: &["openai-key"],
            payload: format!("OPENAI_API_KEY=sk-{}-{sk_tail}\n", "proj"),
        },
        Case {
            id: "anthropic-sk-ant",
            in_scope: true,
            expect_rules: &["openai-key"],
            payload: format!("ANTHROPIC_API_KEY=sk-{}-{}-{ant_tail}\n", "ant", "api03"),
        },
        Case {
            id: "github-pat",
            in_scope: true,
            expect_rules: &["github-token"],
            payload: format!(
                "export GITHUB_TOKEN=gh{}_{}{}\n",
                "p", "F4cD3adB33f5566778", "899aabbccddeeff042"
            ),
        },
        Case {
            id: "slack-bot",
            in_scope: true,
            expect_rules: &["slack-token"],
            payload: format!(
                "slack_webhook: xox{}-{}-{}-{}\n",
                "b", "13984329000", "1374912423", "AbCdEfGhIjKlMnOp"
            ),
        },
        Case {
            id: "pem-private-key",
            in_scope: true,
            expect_rules: &["private-key-block"],
            payload: format!(
                "-----BEGIN RSA PRIVATE {pem_word}-----\nMIIEpAIBAAKCAQEA\n-----END RSA PRIVATE {pem_word}-----\n"
            ),
        },
        Case {
            id: "hex64-unlabeled",
            in_scope: true, // entropy class — detected via WARN (shadow) by hex-high-entropy
            expect_rules: &["hex-high-entropy"],
            payload: format!(
                "{}{}{}{}\n",
                "9f86d081884c7d65", "9a2feaa0c55ad015", "a3bf4f1b2b0b822c", "d15d6c15b0f00a08"
            ),
        },
        Case {
            id: "base64-unlabeled",
            in_scope: true, // entropy class — detected via WARN (shadow) by base64-high-entropy
            expect_rules: &["base64-high-entropy"],
            payload: format!("{}{}{}\n", "Zm9vYmFyYmF6cXV4", "Y29yZ2VncmF1bHQx", "MjM0NTY3OA=="),
        },
        Case {
            id: "password-labeled",
            in_scope: true,
            expect_rules: &["labeled-secret-assignment"],
            payload: format!("{} = \"{}{}\"\n", "password", "correct-horse-19", "battery-staple-7"),
        },
        Case {
            id: "aws-key-pair",
            in_scope: true,
            expect_rules: &["aws-access-key-id", "aws-secret-access-key"],
            payload: format!(
                "aws_access_key_id = {aws_id}\naws_secret_{} = \"{aws_secret}\"\n",
                "access_key"
            ),
        },
        Case {
            id: "gcp-service-account",
            in_scope: true,
            expect_rules: &["gcp-service-account"],
            payload: format!(
                "{{\"type\": \"{}{}\", \"project_id\": \"bench-fixture\"}}\n",
                "service_", "account"
            ),
        },
    ]
}

#[test]
fn bench_positives_meet_phase0_floor() {
    let root = scratch_root("pos");
    let mut rows = Vec::new();
    let mut missed = Vec::new();
    let mut hits = 0;
    for case in positives() {
        let (code, stderr) = run_scan_content(&root, case.payload.as_bytes());
        let fired = fired_rules(&stderr);
        let hit = code == 1 || !fired.is_empty();
        if hit {
            hits += 1;
        }
        // In-scope classes must be caught by their expected rule, not by a lucky overlap —
        // this is what makes the spec's rule-attribution acceptance criterion machine-checked.
        let attributed = case
            .expect_rules
            .iter()
            .any(|r| fired.iter().any(|f| f == r));
        if case.in_scope && !attributed {
            missed.push(format!(
                "{} (expected rule: {:?})",
                case.id, case.expect_rules
            ));
        }
        rows.push(format!(
            "  {:<20} {:<9} {:<8} via {}",
            case.id,
            if case.in_scope {
                "in-scope"
            } else {
                "scoreboard"
            },
            if hit { "DETECTED" } else { "missed" },
            if fired.is_empty() {
                "-".to_string()
            } else {
                fired.join(",")
            }
        ));
    }
    let _ = fs::remove_dir_all(&root);
    assert!(
        missed.is_empty(),
        "secrets-bench floor not met: {hits}/11 detected, in-scope misses:\n  {}\n\
         (all 11 classes are in-scope; the two entropy classes are credited via WARN — \
         see docs/specs/2026-06-13-entropy-scanner.md)\n\
         scoreboard:\n{}",
        missed.join("\n  "),
        rows.join("\n")
    );
}

/// Negatives must not BLOCK. A `WARN ` from the shadow entropy lane on a genuinely
/// high-entropy benign blob (cargo-lock hash, git OID, base64 vector) is EXPECTED under
/// `--content` — that lane carries no path, so `[scan] exclude_paths` cannot apply, and
/// entropy cannot distinguish a benign blob from a real secret (it ships `warn` precisely for
/// that reason; FP rate is a burn-in measurement, not a bench assertion — see
/// docs/specs/2026-06-13-entropy-scanner.md "Fundamental limit"). So this asserts on
/// BLOCK-severity findings only: any negative that BLOCKs (e.g. a future labeled rule
/// misfiring on benign input) still fails the test. The non-vacuity guard below proves the
/// assertion would catch a BLOCK.
#[test]
fn bench_negatives_produce_no_block() {
    let root = scratch_root("neg");
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("secrets-bench")
        .join("negatives");
    // Only `*.txt` is corpus: a stray `.DS_Store` or editor swap file must not break the count.
    let mut names: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "txt"))
        .collect();
    names.sort();
    let mut blocked = Vec::new();
    for path in &names {
        let content = fs::read(path).unwrap();
        let (code, stderr) = run_scan_content(&root, &content);
        // A block-severity finding flips the exit code to 1; a shadow WARN keeps it 0. Assert
        // on both signals so a BLOCK can never slip through as "no parsed rule".
        let block_hits = blocked_rules(&stderr);
        if code == 1 || !block_hits.is_empty() {
            // Finding lines are redacted by design, so quoting them here leaks nothing.
            blocked.push(format!(
                "{}: exit {code}, {}",
                path.file_name().unwrap().to_string_lossy(),
                stderr.lines().next().unwrap_or("(no output)")
            ));
        }
    }
    let _ = fs::remove_dir_all(&root);
    assert_eq!(names.len(), 6, "expected the six negative fixtures");
    assert!(
        blocked.is_empty(),
        "BLOCK-severity false positives on the negative corpus (warns are expected and \
         permitted — entropy is shadow; see docs/specs/2026-06-13-entropy-scanner.md):\n  {}",
        blocked.join("\n  ")
    );
}

/// Non-vacuity guard for `bench_negatives_produce_no_block`: a synthetic input carrying a
/// labeled secret (which the shipped `private-key-block` rule BLOCKs) MUST be caught by the
/// same `blocked_rules` + exit-code signal the negatives test relies on. If this regresses,
/// the negatives assertion has gone blind to BLOCKs and the corpus contract is no longer
/// machine-checked.
#[test]
fn blocked_rules_signal_is_non_vacuous() {
    let root = scratch_root("nonvacuous");
    // A real BLOCK-severity trigger (PEM private-key header), assembled at RUNTIME (the `{pem_word}`
    // split keeps the committed source free of a literal the scanner would block) — mirrors the
    // `pem-private-key` positive case. Never a benign negative.
    let pem_word = "KEY";
    let payload =
        format!("-----BEGIN RSA PRIVATE {pem_word}-----\nMIIEpAIBAAKCAQEA\n-----END RSA PRIVATE {pem_word}-----\n");
    let (code, stderr) = run_scan_content(&root, payload.as_bytes());
    let _ = fs::remove_dir_all(&root);
    assert_eq!(
        code, 1,
        "a PEM private-key header must flip the exit code to 1"
    );
    assert!(
        blocked_rules(&stderr)
            .iter()
            .any(|r| r == "private-key-block"),
        "blocked_rules must parse the BLOCK line for private-key-block; got: {stderr}"
    );
}
