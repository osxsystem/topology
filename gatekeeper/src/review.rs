//! Code-review gate — validate an *untrusted* review artifact against git state.
//!
//! The artifact is untrusted input and this parser is **fail-closed**: any deviation
//! from the strict grammar, or any git-state mismatch, is a veto (exit 1), never a
//! pass. See docs/specs/2026-06-05-code-review-gate.md.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Validated machine header of a review artifact.
#[derive(Debug, PartialEq, Eq)]
pub struct ParsedReview {
    pub verdict_pass: bool,
    pub head: String,
    pub base: String,
}

/// Strip an optional leading UTF-8 BOM, normalize CRLF/CR -> LF, and trim
/// trailing whitespace from every line. Returns the normalized text (LF-joined).
fn normalize(raw: &str) -> String {
    let no_bom = raw.strip_prefix('\u{feff}').unwrap_or(raw);
    let unix = no_bom.replace("\r\n", "\n").replace('\r', "\n");
    unix.split('\n')
        .map(|l| l.trim_end())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Parse `"<key>: <sha>"`, requiring a full 40- or 64-char lowercase hex sha.
fn parse_sha_line(line: &str, key: &str) -> Result<String, String> {
    let prefix = format!("{key}: ");
    let sha = line
        .strip_prefix(&prefix)
        .ok_or_else(|| format!("line must start with '{key}: '; got '{line}'"))?;
    let len_ok = sha.len() == 40 || sha.len() == 64;
    let hex_ok = !sha.is_empty()
        && sha.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    if len_ok && hex_ok {
        Ok(sha.to_string())
    } else {
        Err(format!("{key} must be a full 40/64-char lowercase hex sha; got '{sha}'"))
    }
}

/// Indices of lines that, after normalization, equal `heading` exactly.
fn heading_indices(lines: &[&str], heading: &str) -> Vec<usize> {
    lines
        .iter()
        .enumerate()
        .filter(|(_, l)| **l == heading)
        .map(|(i, _)| i)
        .collect()
}

/// Lines of the section beginning at `start` (its heading line), running until
/// the next H2 (`## `) line or EOF. The heading line itself is excluded. An H3
/// (`### `) does NOT end the section.
fn section_lines<'a>(lines: &[&'a str], start: usize) -> Vec<&'a str> {
    let mut out = Vec::new();
    for l in &lines[start + 1..] {
        if l.starts_with("## ") {
            break;
        }
        out.push(*l);
    }
    out
}

/// True if any line opens an HTML comment.
fn contains_comment(lines: &[&str]) -> bool {
    lines.iter().any(|l| l.contains("<!--"))
}

/// Non-empty, trimmed content lines of a section.
fn content_lines<'a>(lines: &[&'a str]) -> Vec<&'a str> {
    lines
        .iter()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect()
}

/// Verify a `### ` subsection exists within the criteria block and has at least
/// one non-empty, non-heading content line before the next `### ` or the end.
fn check_subsection(criteria: &[&str], sub: &str) -> Result<(), String> {
    let start = criteria
        .iter()
        .position(|l| *l == sub)
        .ok_or_else(|| format!("missing '{sub}' under '## Criteria checked'"))?;
    let has_content = criteria[start + 1..]
        .iter()
        .take_while(|l| !l.starts_with("### "))
        .any(|l| !l.trim().is_empty() && !l.starts_with('#'));
    if has_content {
        Ok(())
    } else {
        Err(format!("'{sub}' has no content line"))
    }
}

/// Parse and fully validate the artifact. Ok only if the entire grammar holds.
pub fn parse_review(raw: &str) -> Result<ParsedReview, String> {
    let normalized = normalize(raw);
    let lines: Vec<&str> = normalized.split('\n').collect();
    if lines.len() < 3 {
        return Err("artifact has fewer than 3 header lines".into());
    }

    // Line 1: verdict is the sole authority.
    let verdict_pass = match lines[0] {
        "VERDICT: pass" => true,
        "VERDICT: fail" => false,
        other => return Err(format!("line 1 must be 'VERDICT: pass|fail'; got '{other}'")),
    };

    // Lines 2-3: full-hex HEAD / BASE.
    let head = parse_sha_line(lines[1], "HEAD")?;
    let base = parse_sha_line(lines[2], "BASE")?;

    // Exactly one '## Blocking findings'.
    let blk = heading_indices(&lines, "## Blocking findings");
    if blk.len() != 1 {
        return Err(format!(
            "expected exactly one '## Blocking findings', found {}",
            blk.len()
        ));
    }
    let blocking = section_lines(&lines, blk[0]);

    // No HTML comments in the header (lines 0..3) or the blocking section.
    if contains_comment(&lines[0..3]) || contains_comment(&blocking) {
        return Err("HTML comment in a machine-parsed region (fail-closed)".into());
    }

    // Blocking content vs. verdict.
    let content = content_lines(&blocking);
    if verdict_pass {
        if content.len() != 1 || content[0] != "None." {
            return Err("pass requires the blocking section to be exactly 'None.'".into());
        }
    } else {
        let has_item = content.iter().any(|l| l.starts_with("- "));
        let has_none = content.iter().any(|l| *l == "None.");
        if !has_item || has_none {
            return Err("fail requires >=1 blocking '- ' item and no 'None.' sentinel".into());
        }
    }

    // Exactly one '## Criteria checked' with both dimensions, each non-empty.
    let crit = heading_indices(&lines, "## Criteria checked");
    if crit.len() != 1 {
        return Err(format!(
            "expected exactly one '## Criteria checked', found {}",
            crit.len()
        ));
    }
    let criteria = section_lines(&lines, crit[0]);
    check_subsection(&criteria, "### Spec/plan")?;
    check_subsection(&criteria, "### Standards")?;

    Ok(ParsedReview { verdict_pass, head, base })
}

/// Run `git -C <root> <args>`, returning trimmed stdout on success.
fn git(root: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git").arg("-C").arg(root).args(args).output().ok()?;
    if out.status.success() {
        Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        None
    }
}

/// The `review` gate. `root` is the framework root; all git runs with `-C root`
/// so the result is independent of the process working directory. Returns a
/// process exit code: 0 pass, 1 veto, 2 usage error.
pub fn gate_review(root: &Path, feature: &str, base_ref: Option<&str>) -> i32 {
    if feature.is_empty() {
        eprintln!("gatekeeper: --feature <slug> is required");
        return 2;
    }

    let head = match git(root, &["rev-parse", "HEAD"]) {
        Some(h) => h,
        None => {
            println!("FAIL review gate: not a git repository (git rev-parse HEAD failed)");
            return 1;
        }
    };

    // Clean worktree, ignoring untracked/modified paths under docs/reviews/.
    // `--untracked-files=all` lists files individually so an untracked directory is NOT
    // collapsed to a bare `docs/` entry (which would slip past the docs/reviews/ filter).
    // A failed status is fail-closed, never assumed clean.
    let porcelain = match git(root, &["status", "--porcelain", "--untracked-files=all"]) {
        Some(p) => p,
        None => {
            println!("FAIL review gate: git status failed");
            return 1;
        }
    };
    let dirty: Vec<&str> = porcelain
        .lines()
        .filter(|l| !l.get(3..).unwrap_or("").starts_with("docs/reviews/"))
        .collect();
    if !dirty.is_empty() {
        println!("FAIL review gate: uncommitted changes outside docs/reviews/:");
        for l in &dirty {
            println!("  {l}");
        }
        return 1;
    }

    let branch = base_ref.unwrap_or("main");
    let base = match git(root, &["merge-base", branch, "HEAD"]) {
        Some(b) => b,
        None => {
            println!("FAIL review gate: cannot resolve merge-base of '{branch}' and HEAD");
            return 1;
        }
    };

    // Select artifacts whose line-2 HEAD equals the current HEAD.
    let dir = root.join("docs").join("reviews");
    let suffix = format!("-{feature}.md");
    let want_head = format!("HEAD: {head}");
    let mut matches: Vec<PathBuf> = Vec::new();
    if let Ok(rd) = fs::read_dir(&dir) {
        for e in rd.flatten() {
            let p = e.path();
            let name = match p.file_name() {
                Some(n) => n.to_string_lossy().to_string(),
                None => continue,
            };
            if !name.ends_with(&suffix) {
                continue;
            }
            let txt = fs::read_to_string(&p).unwrap_or_default();
            if normalize(&txt).split('\n').nth(1) == Some(want_head.as_str()) {
                matches.push(p);
            }
        }
    }

    match matches.len() {
        0 => {
            println!("FAIL review gate: no review artifact names current HEAD {head}");
            1
        }
        1 => {
            let path = &matches[0];
            let text = fs::read_to_string(path).unwrap_or_default();
            match parse_review(&text) {
                Err(e) => {
                    println!("FAIL review gate: {} — {e}", path.display());
                    1
                }
                Ok(r) => {
                    if r.base != base {
                        println!(
                            "FAIL review gate: BASE {} != computed merge-base {base}",
                            r.base
                        );
                        return 1;
                    }
                    if !r.verdict_pass {
                        println!("FAIL review gate: verdict is fail ({})", path.display());
                        return 1;
                    }
                    println!("PASS review gate: {}", path.display());
                    0
                }
            }
        }
        _ => {
            println!(
                "FAIL review gate: {} artifacts name current HEAD (ambiguous):",
                matches.len()
            );
            for p in &matches {
                println!("  {}", p.display());
            }
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PASS: &str = "VERDICT: pass\nHEAD: 9f3c1a7e5b2d8c4f0a1b6e7d9c2f5a8b3e4d6c1a\nBASE: 2a7d4e1c9b6f3a8d5e2c1b0a9f8e7d6c5b4a3210\n\n# Review\n\n## Blocking findings\nNone.\n\n## Criteria checked\n### Spec/plan\n- crit one — met\n### Standards\n- adr rule — met\n";
    const FAIL_DOC: &str = "VERDICT: fail\nHEAD: 9f3c1a7e5b2d8c4f0a1b6e7d9c2f5a8b3e4d6c1a\nBASE: 2a7d4e1c9b6f3a8d5e2c1b0a9f8e7d6c5b4a3210\n\n# Review\n\n## Blocking findings\n- src/foo.rs:42 — wrong and why\n\n## Criteria checked\n### Spec/plan\n- crit — partial\n### Standards\n- rule — violated\n";

    // The literal pass/fail examples from the spec (docs/specs/2026-06-05-code-review-gate.md,
    // the two ```markdown blocks). Regression guard against a documented-but-invalid template.
    const SPEC_PASS_SAMPLE: &str = "VERDICT: pass\nHEAD: 9f3c1a7e5b2d8c4f0a1b6e7d9c2f5a8b3e4d6c1a\nBASE: 2a7d4e1c9b6f3a8d5e2c1b0a9f8e7d6c5b4a3210\n\n# Review: <feature> (<date>)\n\n## Blocking findings\nNone.\n\n## Non-blocking notes\n- <nits — never gate on these>\n\n## Criteria checked\n### Spec/plan\n- <acceptance criterion 1> — <how the diff satisfies it>\n### Standards\n- <ADR/AGENTS rule> — <conformance evidence>\n";
    const SPEC_FAIL_SAMPLE: &str = "VERDICT: fail\nHEAD: 9f3c1a7e5b2d8c4f0a1b6e7d9c2f5a8b3e4d6c1a\nBASE: 2a7d4e1c9b6f3a8d5e2c1b0a9f8e7d6c5b4a3210\n\n# Review: <feature> (<date>)\n\n## Blocking findings\n- src/foo.rs:42 — <what's wrong and why it blocks>\n\n## Non-blocking notes\n- ...\n\n## Criteria checked\n### Spec/plan\n- ...\n### Standards\n- ...\n";

    #[test]
    fn smoke_valid_pass_parses() {
        let r = parse_review(PASS).unwrap();
        assert!(r.verdict_pass);
        assert_eq!(r.head, "9f3c1a7e5b2d8c4f0a1b6e7d9c2f5a8b3e4d6c1a");
    }
    #[test]
    fn fail_doc_parses_as_fail() {
        assert!(!parse_review(FAIL_DOC).unwrap().verdict_pass);
    }
    #[test]
    fn bad_verdict_keyword_rejected() {
        let t = PASS.replacen("VERDICT: pass", "VERDICT: PASS", 1);
        assert!(parse_review(&t).is_err());
    }
    #[test]
    fn abbreviated_head_sha_rejected() {
        let t = PASS.replacen("9f3c1a7e5b2d8c4f0a1b6e7d9c2f5a8b3e4d6c1a", "9f3c1a7", 1);
        assert!(parse_review(&t).is_err());
    }
    #[test]
    fn abbreviated_base_sha_rejected() {
        let t = PASS.replacen("2a7d4e1c9b6f3a8d5e2c1b0a9f8e7d6c5b4a3210", "2a7d4e1", 1);
        assert!(parse_review(&t).is_err());
    }
    #[test]
    fn malformed_base_line_rejected() {
        let t = PASS.replacen("BASE: ", "BSE: ", 1);
        assert!(parse_review(&t).is_err());
    }
    #[test]
    fn pass_with_a_blocking_item_rejected() {
        let t = PASS.replacen("None.", "- src/x.rs:1 — sneaky", 1);
        assert!(parse_review(&t).is_err());
    }
    #[test]
    fn fail_without_items_rejected() {
        let t = FAIL_DOC.replacen("- src/foo.rs:42 — wrong and why", "None.", 1);
        assert!(parse_review(&t).is_err());
    }
    #[test]
    fn fail_with_none_and_item_rejected() {
        // A 'fail' that also carries the 'None.' sentinel is contradictory -> reject (CONCERN #2).
        let t = FAIL_DOC.replacen(
            "- src/foo.rs:42 — wrong and why",
            "None.\n- src/foo.rs:42 — wrong and why",
            1,
        );
        assert!(parse_review(&t).is_err());
    }
    #[test]
    fn two_blocking_headings_rejected() {
        let t = PASS.replacen("## Criteria checked", "## Blocking findings\nNone.\n\n## Criteria checked", 1);
        assert!(parse_review(&t).is_err());
    }
    #[test]
    fn zero_blocking_headings_rejected() {
        let t = PASS.replacen("## Blocking findings\nNone.\n\n", "", 1);
        assert!(parse_review(&t).is_err());
    }
    #[test]
    fn missing_standards_dimension_rejected() {
        let t = PASS.replacen("### Standards\n- adr rule — met\n", "", 1);
        assert!(parse_review(&t).is_err());
    }
    #[test]
    fn empty_specplan_dimension_rejected() {
        let t = PASS.replacen("### Spec/plan\n- crit one — met\n", "### Spec/plan\n", 1);
        assert!(parse_review(&t).is_err());
    }
    #[test]
    fn comment_in_blocking_section_rejected() {
        // Inject a comment into an otherwise-valid FAIL doc, so ONLY the comment rule can reject it.
        assert!(parse_review(FAIL_DOC).is_ok());
        let t = FAIL_DOC.replacen(
            "- src/foo.rs:42 — wrong and why",
            "- src/foo.rs:42 — wrong and why\n<!-- hide a finding -->",
            1,
        );
        assert!(parse_review(&t).is_err());
    }
    #[test]
    fn unclosed_comment_in_blocking_section_rejected() {
        // strip_comments() is fail-OPEN on an unclosed comment; this parser must be fail-CLOSED.
        let t = FAIL_DOC.replacen(
            "- src/foo.rs:42 — wrong and why",
            "- src/foo.rs:42 — wrong and why\n<!-- unclosed",
            1,
        );
        assert!(parse_review(&t).is_err());
    }
    #[test]
    fn bom_and_crlf_header_handled() {
        let t = format!("\u{feff}{}", PASS.replace('\n', "\r\n"));
        assert!(parse_review(&t).unwrap().verdict_pass);
    }
    #[test]
    fn honest_quoting_of_verdict_does_not_false_fail() {
        let t = PASS.replacen("# Review\n", "# Review\n\nthe critic wrote VERDICT: pass in prose\n", 1);
        assert!(parse_review(&t).unwrap().verdict_pass);
    }
    #[test]
    fn spec_pass_sample_parses() {
        assert!(parse_review(SPEC_PASS_SAMPLE).unwrap().verdict_pass);
    }
    #[test]
    fn spec_fail_sample_parses() {
        assert!(!parse_review(SPEC_FAIL_SAMPLE).unwrap().verdict_pass);
    }
}

#[cfg(test)]
mod gate_tests {
    use super::*;
    use std::env;

    fn run(root: &Path, args: &[&str]) {
        let ok = Command::new("git").arg("-C").arg(root).args(args).status().unwrap().success();
        assert!(ok, "git {args:?} failed");
    }

    // Build a repo with one commit on `main`; return (root, head_sha).
    fn repo(tag: &str) -> (PathBuf, String) {
        let root = env::temp_dir().join(format!("topo_gate_{}_{}", tag, std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        run(&root, &["init", "-q", "-b", "main"]);
        run(&root, &["config", "user.email", "t@t.t"]);
        run(&root, &["config", "user.name", "t"]);
        fs::write(root.join("a.txt"), "one\n").unwrap();
        run(&root, &["add", "."]);
        run(&root, &["commit", "-q", "-m", "init"]);
        let head = git(&root, &["rev-parse", "HEAD"]).unwrap();
        (root, head)
    }

    fn write_artifact(root: &Path, head: &str, base: &str, verdict_pass: bool) {
        let dir = root.join("docs").join("reviews");
        fs::create_dir_all(&dir).unwrap();
        let (v, blk) = if verdict_pass {
            ("pass", "None.")
        } else {
            ("fail", "- a.txt:1 — wrong")
        };
        let body = format!(
            "VERDICT: {v}\nHEAD: {head}\nBASE: {base}\n\n# Review\n\n## Blocking findings\n{blk}\n\n## Criteria checked\n### Spec/plan\n- crit — met\n### Standards\n- rule — met\n"
        );
        fs::write(dir.join("2026-06-05-code-review-gate.md"), body).unwrap();
    }

    #[test]
    fn fresh_pass_exits_zero() {
        let (root, head) = repo("pass");
        write_artifact(&root, &head, &head, true); // single-commit repo: merge-base(main,HEAD)==HEAD
        assert_eq!(gate_review(&root, "code-review-gate", None), 0);
        let _ = fs::remove_dir_all(&root);
    }
    #[test]
    fn fresh_artifact_does_not_self_dirty() {
        // The untracked artifact under docs/reviews/ is present (-uall shows it) ...
        let (root, head) = repo("selfdirty");
        write_artifact(&root, &head, &head, true);
        let porcelain = git(&root, &["status", "--porcelain", "--untracked-files=all"]).unwrap();
        assert!(porcelain.lines().any(|l| l.contains("docs/reviews/")));
        // ... yet the gate still passes, because the clean-tree check excludes docs/reviews/.
        assert_eq!(gate_review(&root, "code-review-gate", None), 0);
        let _ = fs::remove_dir_all(&root);
    }
    #[test]
    fn stale_head_exits_one() {
        let (root, head) = repo("stale");
        write_artifact(&root, "0000000000000000000000000000000000000000", &head, true);
        assert_eq!(gate_review(&root, "code-review-gate", None), 1);
        let _ = fs::remove_dir_all(&root);
    }
    #[test]
    fn dirty_outside_reviews_exits_one() {
        let (root, head) = repo("dirty");
        write_artifact(&root, &head, &head, true);
        fs::write(root.join("a.txt"), "changed\n").unwrap(); // tracked file modified
        assert_eq!(gate_review(&root, "code-review-gate", None), 1);
        let _ = fs::remove_dir_all(&root);
    }
    #[test]
    fn untracked_outside_reviews_exits_one() {
        // An untracked file OUTSIDE docs/reviews/ must dirty the tree (proves the filter scope).
        let (root, head) = repo("untracked_out");
        write_artifact(&root, &head, &head, true);
        fs::write(root.join("stray.txt"), "junk\n").unwrap();
        assert_eq!(gate_review(&root, "code-review-gate", None), 1);
        let _ = fs::remove_dir_all(&root);
    }
    #[test]
    fn wrong_base_exits_one() {
        let (root, head) = repo("wrongbase");
        write_artifact(&root, &head, "1111111111111111111111111111111111111111", true);
        assert_eq!(gate_review(&root, "code-review-gate", None), 1);
        let _ = fs::remove_dir_all(&root);
    }
    #[test]
    fn fail_verdict_exits_one() {
        let (root, head) = repo("failv");
        write_artifact(&root, &head, &head, false);
        assert_eq!(gate_review(&root, "code-review-gate", None), 1);
        let _ = fs::remove_dir_all(&root);
    }
    #[test]
    fn unresolvable_base_exits_one() {
        let (root, head) = repo("nobase");
        write_artifact(&root, &head, &head, true);
        assert_eq!(gate_review(&root, "code-review-gate", Some("no-such-branch")), 1);
        let _ = fs::remove_dir_all(&root);
    }
    #[test]
    fn not_a_repo_exits_one() {
        let root = env::temp_dir().join(format!("topo_gate_norepo_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        assert_eq!(gate_review(&root, "code-review-gate", None), 1);
        let _ = fs::remove_dir_all(&root);
    }
    #[test]
    fn ambiguous_two_artifacts_same_head_exits_one() {
        let (root, head) = repo("ambig");
        write_artifact(&root, &head, &head, true); // 2026-06-05-code-review-gate.md
        let body = fs::read_to_string(root.join("docs/reviews/2026-06-05-code-review-gate.md")).unwrap();
        fs::write(root.join("docs/reviews/2026-06-06-code-review-gate.md"), body).unwrap();
        assert_eq!(gate_review(&root, "code-review-gate", None), 1);
        let _ = fs::remove_dir_all(&root);
    }
    #[test]
    fn divergent_branch_uses_fork_point_as_base() {
        // Real divergence: main stays at C0; feature advances to C1. merge-base(main,HEAD)==C0.
        let (root, base) = repo("divergent");
        run(&root, &["checkout", "-q", "-b", "feature"]);
        fs::write(root.join("a.txt"), "two\n").unwrap();
        run(&root, &["add", "."]);
        run(&root, &["commit", "-q", "-m", "feature work"]);
        let head = git(&root, &["rev-parse", "HEAD"]).unwrap();
        assert_ne!(head, base);
        // Correct review: BASE is the fork point, not HEAD.
        write_artifact(&root, &head, &base, true);
        assert_eq!(gate_review(&root, "code-review-gate", None), 0);
        // A review lying with BASE == HEAD must be rejected (merge-base != HEAD here).
        write_artifact(&root, &head, &head, true);
        assert_eq!(gate_review(&root, "code-review-gate", None), 1);
        let _ = fs::remove_dir_all(&root);
    }
}
