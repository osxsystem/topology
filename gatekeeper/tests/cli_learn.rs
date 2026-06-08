use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

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
fn run(cwd: &Path, args: &[&str], stdin: &[u8]) -> (i32, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_gatekeeper"))
        .current_dir(cwd)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(stdin).unwrap();
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
