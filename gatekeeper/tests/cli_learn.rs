use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

// ── helpers for governed / in-repo anchoring tests ───────────────────────────

/// Build a minimal framework dir: skills/ + AGENTS.md + instincts/ + security/rules.toml.
fn scratch_framework(tag: &str) -> PathBuf {
    let fw = std::env::temp_dir().join(format!("topo_learn_fw_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&fw);
    fs::create_dir_all(fw.join("skills")).unwrap();
    fs::create_dir_all(fw.join("instincts")).unwrap();
    fs::write(fw.join("AGENTS.md"), "").unwrap();
    fs::create_dir_all(fw.join("security")).unwrap();
    fs::write(
        fw.join("security").join("rules.toml"),
        "schema_version = 1\n",
    )
    .unwrap();
    fw
}

/// Build a minimal project dir with a .git dir (no framework markers).
fn scratch_project(tag: &str) -> PathBuf {
    let proj = std::env::temp_dir().join(format!("topo_learn_proj_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&proj);
    fs::create_dir_all(&proj).unwrap();
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
    stdin: &[u8],
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
    if let Err(e) = child_stdin.write_all(stdin) {
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

/// A minimal framework root: a `skills/` marker (so `framework_root()` resolves here), an empty
/// `instincts/` dir, and a `security/rules.toml` so the `scan` / `instinct` / `list` surfaces a
/// promoted operator must satisfy are all live.
fn scratch_root(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("topo_learn_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("skills")).unwrap();
    fs::create_dir_all(root.join("instincts")).unwrap();
    fs::create_dir_all(root.join("security")).unwrap();
    fs::write(
        root.join("security").join("rules.toml"),
        "schema_version = 1\n",
    )
    .unwrap();
    root
}

/// Run `gatekeeper <args>` from `cwd`, feeding `stdin`. Returns (exit code, stdout).
///
/// TOPOLOGY_ROOT is pinned to `cwd` so that after Phase 11 binary-adjacent resolution
/// does not silently point at the actual topology repo instead of the scratch root.
fn run(cwd: &Path, args: &[&str], stdin: &[u8]) -> (i32, String) {
    let canonical_cwd = std::fs::canonicalize(cwd).unwrap_or_else(|_| cwd.to_path_buf());
    let mut child = Command::new(env!("CARGO_BIN_EXE_gatekeeper"))
        .current_dir(cwd)
        .args(args)
        .env("TOPOLOGY_ROOT", &canonical_cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    // The child may reject its args and exit before reading stdin, closing the pipe; a
    // partial write then races to BrokenPipe. We assert on the exit code and output, not on
    // stdin being consumed, so tolerate EPIPE here.
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
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
}

#[test]
fn capture_creates_ledger_file_with_entry() {
    let root = scratch_root("file");
    let (code, out) = run(
        &root,
        &[
            "learn",
            "capture",
            "--summary",
            "first ever gotcha",
            "--id",
            "first-one",
        ],
        b"",
    );
    assert_eq!(code, 0);
    assert!(out.contains("captured 'first-one'"), "{out}");
    let led = fs::read_to_string(root.join("docs/learn/ledger.md")).unwrap();
    assert!(led.contains("# Gotcha ledger"));
    assert!(led.contains("## first-one"));
    assert!(led.contains("> first ever gotcha"));
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn capture_appends_then_list_counts_recurrence() {
    let root = scratch_root("recur");
    let (c1, _) = run(
        &root,
        &[
            "learn",
            "capture",
            "--summary",
            "same gotcha",
            "--id",
            "again",
            "--kind",
            "skill",
        ],
        b"",
    );
    assert_eq!(c1, 0);
    let (c2, _) = run(
        &root,
        &[
            "learn",
            "capture",
            "--summary",
            "same gotcha, second time",
            "--id",
            "again",
            "--kind",
            "skill",
        ],
        b"",
    );
    assert_eq!(c2, 0);
    let (lc, lout) = run(&root, &["learn", "list"], b"");
    assert_eq!(lc, 0);
    let row = lout
        .lines()
        .find(|l| l.starts_with("again\t"))
        .expect("an 'again' row");
    assert_eq!(
        row, "again\t2\tskill",
        "recurrence counts to 2, proposed kind shown"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn list_missing_ledger_is_empty_exit_0() {
    let root = scratch_root("noledger");
    let (code, out) = run(&root, &["learn", "list"], b"");
    assert_eq!(code, 0, "a missing ledger is the empty set, not an error");
    assert!(out.is_empty());
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn list_on_malformed_ledger_exits_2() {
    let root = scratch_root("badledger");
    fs::create_dir_all(root.join("docs/learn")).unwrap();
    fs::write(
        root.join("docs/learn/ledger.md"),
        "## x\n\n- bogus: 1\n\n> body\n",
    )
    .unwrap();
    let (code, _) = run(&root, &["learn", "list"], b"");
    assert_eq!(
        code, 2,
        "an unknown field fails loud, mirroring `instinct list`/`scan`"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn promote_instinct_passes_instinct_list() {
    let root = scratch_root("inst");
    run(
        &root,
        &[
            "learn",
            "capture",
            "--summary",
            "Unit green is not the verify gate; record a re-runnable command",
            "--id",
            "verify-not-unit",
            "--kind",
            "instinct",
            "--trigger",
            "gate-failure",
            "--gate",
            "verify",
        ],
        b"",
    );
    let (code, out) = run(
        &root,
        &["learn", "promote", "--id", "verify-not-unit", "--yes"],
        b"",
    );
    assert_eq!(code, 0, "promote instinct exits 0; out={out}");
    let made = fs::read_to_string(root.join("instincts/verify-not-unit.md")).unwrap();
    assert!(
        made.contains("source: ledger:verify-not-unit"),
        "promoted instinct back-links to its ledger entry"
    );
    // The promoted instinct must load under the instinct surface — the Phase-3 verify criterion.
    let (lc, lout) = run(&root, &["instinct", "list"], b"");
    assert_eq!(lc, 0);
    assert!(
        lout.contains("verify-not-unit"),
        "promoted instinct appears in `instinct list`: {lout}"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn promote_skill_appears_in_gatekeeper_list() {
    let root = scratch_root("skill");
    run(
        &root,
        &[
            "learn",
            "capture",
            "--summary",
            "Always re-read a file before editing it",
            "--id",
            "read-before-edit",
            "--kind",
            "skill",
        ],
        b"",
    );
    let (code, _) = run(
        &root,
        &["learn", "promote", "--id", "read-before-edit", "--yes"],
        b"",
    );
    assert_eq!(code, 0);
    assert!(root.join("skills/read-before-edit/SKILL.md").exists());
    let (lc, lout) = run(&root, &["list"], b"");
    assert_eq!(lc, 0);
    assert!(
        lout.contains("read-before-edit"),
        "promoted skill shows in `gatekeeper list`: {lout}"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn promote_rule_loads_under_scan() {
    let root = scratch_root("rule");
    run(
        &root,
        &[
            "learn",
            "capture",
            "--summary",
            "FIXME-SECRET markers keep leaking into commits",
            "--id",
            "leaky-marker",
            "--kind",
            "rule",
        ],
        b"",
    );
    let (code, out) = run(
        &root,
        &[
            "learn",
            "promote",
            "--id",
            "leaky-marker",
            "--pattern",
            r"\bFIXME-SECRET\b",
            "--severity",
            "block",
            "--yes",
        ],
        b"",
    );
    assert_eq!(code, 0, "promote rule exits 0; out={out}");
    // rules.toml still loads (a clean input returns 0, not the fail-closed 2) ...
    let (c_clean, _) = run(&root, &["scan", "--content"], b"nothing here\n");
    assert_eq!(c_clean, 0, "rules.toml still loads after the appended rule");
    // ... and the promoted block rule matches + vetoes its pattern.
    let (c_hit, _) = run(
        &root,
        &["scan", "--content"],
        b"oops FIXME-SECRET in here\n",
    );
    assert_eq!(c_hit, 1, "promoted block rule vetoes its pattern");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn promote_requires_confirmation() {
    let root = scratch_root("confirm");
    run(
        &root,
        &[
            "learn",
            "capture",
            "--summary",
            "some lesson worth keeping",
            "--id",
            "needs-ok",
            "--kind",
            "instinct",
        ],
        b"",
    );
    // No --yes, answer 'n': nothing is written, and a decline is not an error.
    let (code, _) = run(&root, &["learn", "promote", "--id", "needs-ok"], b"n\n");
    assert_eq!(code, 0, "a declined promotion is not an error");
    assert!(
        !root.join("instincts/needs-ok.md").exists(),
        "decline writes nothing"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn promote_unknown_id_exits_2() {
    let root = scratch_root("unknown");
    run(
        &root,
        &[
            "learn",
            "capture",
            "--summary",
            "something",
            "--id",
            "present",
            "--kind",
            "instinct",
        ],
        b"",
    );
    let (code, _) = run(
        &root,
        &[
            "learn", "promote", "--id", "absent", "--kind", "instinct", "--yes",
        ],
        b"",
    );
    assert_eq!(code, 2, "an unknown ledger id fails loud");
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn promote_rule_without_pattern_exits_2() {
    let root = scratch_root("nopat");
    run(
        &root,
        &[
            "learn",
            "capture",
            "--summary",
            "needs a pattern",
            "--id",
            "ruleish",
            "--kind",
            "rule",
        ],
        b"",
    );
    let (code, _) = run(
        &root,
        &["learn", "promote", "--id", "ruleish", "--yes"],
        b"",
    );
    assert_eq!(
        code, 2,
        "a rule promotion without --pattern is a usage error"
    );
    let _ = fs::remove_dir_all(&root);
}

// ── Task 3: ledger anchors to the artifacts root ──────────────────────────────
//
// "Governed" scenario: project ≠ framework (separate dirs, TOPOLOGY_ROOT points at framework)
//   → artifacts_root() = <project>/.claude/topology
//   → ledger path    = <project>/.claude/topology/learn/ledger.md
//
// "In-repo" scenario: project == framework (same dir has .git + skills/ + AGENTS.md)
//   → artifacts_root() = <root>/docs
//   → ledger path    = <root>/docs/learn/ledger.md  (unchanged)

#[test]
fn governed_capture_lands_at_claude_topology_learn() {
    let fw = scratch_framework("gov-cap");
    let proj = scratch_project("gov-cap");

    let (code, stdout, stderr) = run_with_topology_root(
        &proj,
        &fw,
        &[
            "learn",
            "capture",
            "--summary",
            "governed gotcha lands in project",
            "--id",
            "gov-gotcha",
        ],
        b"",
    );
    assert_eq!(code, 0, "capture must succeed; stderr={stderr}");
    assert!(
        stdout.contains("gov-gotcha"),
        "capture output must name the id; stdout={stdout}"
    );

    let ledger_path = proj
        .join(".claude")
        .join("topology")
        .join("learn")
        .join("ledger.md");
    assert!(
        ledger_path.exists(),
        "ledger must be at .claude/topology/learn/ledger.md in a governed project"
    );
    let contents = fs::read_to_string(&ledger_path).unwrap();
    assert!(
        contents.contains("## gov-gotcha"),
        "ledger must contain the entry"
    );
    assert!(
        contents.contains("> governed gotcha lands in project"),
        "ledger must contain the summary"
    );

    // Old framework-root path must NOT be created.
    assert!(
        !fw.join("docs").join("learn").exists(),
        "ledger must NOT be written into the framework root in a governed project"
    );

    let _ = fs::remove_dir_all(&fw);
    let _ = fs::remove_dir_all(&proj);
}

#[test]
fn governed_list_reads_back_captured_entries() {
    let fw = scratch_framework("gov-list");
    let proj = scratch_project("gov-list");

    // Capture two entries.
    let (c1, _, e1) = run_with_topology_root(
        &proj,
        &fw,
        &[
            "learn",
            "capture",
            "--summary",
            "first governed lesson",
            "--id",
            "lesson-one",
            "--kind",
            "instinct",
        ],
        b"",
    );
    assert_eq!(c1, 0, "first capture must succeed; stderr={e1}");

    let (c2, _, e2) = run_with_topology_root(
        &proj,
        &fw,
        &[
            "learn",
            "capture",
            "--summary",
            "second governed lesson",
            "--id",
            "lesson-two",
            "--kind",
            "skill",
        ],
        b"",
    );
    assert_eq!(c2, 0, "second capture must succeed; stderr={e2}");

    let (lcode, lout, lerr) = run_with_topology_root(&proj, &fw, &["learn", "list"], b"");
    assert_eq!(lcode, 0, "list must succeed; stderr={lerr}");
    assert!(
        lout.contains("lesson-one"),
        "list must include lesson-one; stdout={lout}"
    );
    assert!(
        lout.contains("lesson-two"),
        "list must include lesson-two; stdout={lout}"
    );

    let _ = fs::remove_dir_all(&fw);
    let _ = fs::remove_dir_all(&proj);
}

#[test]
fn in_repo_capture_still_lands_at_docs_learn() {
    // In-repo: project == framework → artifacts_root() = <root>/docs → ledger = docs/learn/ledger.md.
    let root = scratch_root("inrepo-cap");
    // Add AGENTS.md so is_marked_root() treats root as framework root.
    fs::write(root.join("AGENTS.md"), "").unwrap();
    // Add .git so project_root() resolves to root.
    std::process::Command::new("git")
        .args(["init", root.to_str().unwrap()])
        .output()
        .expect("git init must succeed");

    let (code, _, stderr) = run_with_topology_root(
        &root,
        &root,
        &[
            "learn",
            "capture",
            "--summary",
            "in-repo ledger path is unchanged",
            "--id",
            "inrepo-check",
        ],
        b"",
    );
    assert_eq!(code, 0, "in-repo capture must succeed; stderr={stderr}");

    let ledger_path = root.join("docs").join("learn").join("ledger.md");
    assert!(
        ledger_path.exists(),
        "in-repo ledger must still be at docs/learn/ledger.md"
    );

    let _ = fs::remove_dir_all(&root);
}

// ── Task 4: promote refuses in governed projects ──────────────────────────────

#[test]
fn governed_promote_exits_nonzero_writes_nothing() {
    let fw = scratch_framework("gov-promote");
    let proj = scratch_project("gov-promote");

    // First capture an entry in the governed project's ledger.
    let (cc, _, ce) = run_with_topology_root(
        &proj,
        &fw,
        &[
            "learn",
            "capture",
            "--summary",
            "a governed gotcha to try promoting",
            "--id",
            "gov-promote-id",
            "--kind",
            "instinct",
        ],
        b"",
    );
    assert_eq!(cc, 0, "capture must succeed; stderr={ce}");

    // Attempt promote — must be refused (exit non-zero).
    let (code, stdout, stderr) = run_with_topology_root(
        &proj,
        &fw,
        &["learn", "promote", "--id", "gov-promote-id", "--yes"],
        b"",
    );
    assert_ne!(code, 0, "promote in a governed project must exit non-zero");

    // The refusal message must name the ledger path.
    let combined = format!("{stdout}{stderr}");
    let ledger_path = proj
        .join(".claude")
        .join("topology")
        .join("learn")
        .join("ledger.md");
    assert!(
        combined.contains(ledger_path.to_str().unwrap())
            || combined.contains(".claude/topology/learn/ledger.md"),
        "refusal must name the ledger path; output={combined}"
    );

    // The refusal message must cite ADR-0013.
    assert!(
        combined.contains("ADR-0013"),
        "refusal must cite ADR-0013; output={combined}"
    );

    // Nothing must be written inside the framework dir (payload stays read-only).
    let fw_instincts = fw.join("instincts");
    let new_files: Vec<_> = fs::read_dir(&fw_instincts)
        .map(|rd| rd.flatten().collect())
        .unwrap_or_default();
    assert!(
        new_files.is_empty(),
        "promote must write nothing into the framework root; files={new_files:?}"
    );

    let _ = fs::remove_dir_all(&fw);
    let _ = fs::remove_dir_all(&proj);
}
