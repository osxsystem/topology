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
