//! Security scanning — the deterministic safety floor.
//!
//! Matches a versioned `security/rules.toml` against stdin-delivered inputs. Two rule kinds:
//! `content` (secrets, run on every input) and `command` (dangerous shells, run only on command
//! strings). The scanner never emits a matched value — diagnostics carry a redacted hint only.
//! See docs/specs/2026-06-06-security-scanning.md.

use std::collections::HashSet;
use std::fs;
use std::io::Read;
use std::path::Path;

use regex::bytes::{Regex, RegexSet};
use serde::Deserialize;

const SCHEMA_VERSION: u32 = 1;

// ---------- raw (deserialized) model ----------

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RulesFile {
    schema_version: u32,
    #[serde(default)]
    rule: Vec<RawRule>,
    #[serde(default)]
    allow: Vec<RawAllow>,
    #[serde(default)]
    allow_blob: Vec<AllowBlob>,
    #[serde(default)]
    integrity: Integrity,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRule {
    id: String,
    kind: Kind,
    severity: Severity,
    description: String,
    pattern: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Kind {
    Content,
    Command,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Severity {
    Block,
    Warn,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAllow {
    rule: String,
    #[serde(default)]
    value: Option<String>,
    #[serde(default)]
    pattern: Option<String>,
    #[serde(default)]
    reason: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AllowBlob {
    path: String,
    blob_oid: String,
    #[serde(default)]
    reason: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Integrity {
    #[serde(default)]
    protected_paths: Vec<String>,
}

// ---------- compiled model ----------

#[derive(Debug)]
struct CompiledRule {
    id: String,
    severity: Severity,
    description: String,
    re: Regex,
}

#[derive(Debug)]
enum AllowMatch {
    Exact(Vec<u8>),
    Pattern(Regex),
}

#[derive(Debug)]
struct CompiledAllow {
    rule: String,
    matcher: AllowMatch,
}

/// The fully validated, compiled rule set.
#[derive(Debug)]
pub struct Rules {
    content: Vec<CompiledRule>,
    content_set: RegexSet,
    command: Vec<CompiledRule>,
    command_set: RegexSet,
    allows: Vec<CompiledAllow>,
    allow_blobs: Vec<AllowBlob>,
    protected: Vec<String>,
}

/// Read and fully validate the rules file at `path`.
pub fn load_rules(path: &Path) -> Result<Rules, String> {
    let raw =
        fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    parse_rules(&raw)
}

/// Validate + compile from TOML text. Any defect is an Err (the caller maps it to exit 2).
fn parse_rules(raw: &str) -> Result<Rules, String> {
    let parsed: RulesFile =
        toml::from_str(raw).map_err(|e| format!("rules.toml parse/validation error: {e}"))?;
    if parsed.schema_version != SCHEMA_VERSION {
        return Err(format!(
            "unsupported schema_version {} (expected {SCHEMA_VERSION})",
            parsed.schema_version
        ));
    }

    let mut seen = HashSet::new();
    for r in &parsed.rule {
        if !seen.insert(r.id.as_str()) {
            return Err(format!("duplicate rule id '{}'", r.id));
        }
    }

    let mut content = Vec::new();
    let mut command = Vec::new();
    for r in &parsed.rule {
        let re =
            Regex::new(&r.pattern).map_err(|e| format!("rule '{}': invalid pattern: {e}", r.id))?;
        let cr = CompiledRule {
            id: r.id.clone(),
            severity: r.severity,
            description: r.description.clone(),
            re,
        };
        match r.kind {
            Kind::Content => content.push(cr),
            Kind::Command => command.push(cr),
        }
    }
    let content_set = RegexSet::new(content.iter().map(|c| c.re.as_str()))
        .map_err(|e| format!("content rule set: {e}"))?;
    let command_set = RegexSet::new(command.iter().map(|c| c.re.as_str()))
        .map_err(|e| format!("command rule set: {e}"))?;

    let mut allows = Vec::new();
    for a in &parsed.allow {
        let matcher = match (&a.value, &a.pattern) {
            (Some(v), None) => AllowMatch::Exact(v.clone().into_bytes()),
            (None, Some(p)) => AllowMatch::Pattern(
                Regex::new(p)
                    .map_err(|e| format!("allow for '{}': invalid pattern: {e}", a.rule))?,
            ),
            (Some(_), Some(_)) => {
                return Err(format!(
                    "allow for '{}': set value OR pattern, not both",
                    a.rule
                ))
            }
            (None, None) => {
                return Err(format!(
                    "allow for '{}': requires a concrete value or pattern (rule=\"*\" included)",
                    a.rule
                ))
            }
        };
        allows.push(CompiledAllow {
            rule: a.rule.clone(),
            matcher,
        });
    }

    Ok(Rules {
        content,
        content_set,
        command,
        command_set,
        allows,
        allow_blobs: parsed.allow_blob,
        protected: parsed.integrity.protected_paths,
    })
}

/// One block/warn finding. Carries only a redacted hint — never the matched value.
struct Finding {
    rule_id: String,
    severity: Severity,
    description: String,
    redacted: String,
    location: String,
}

/// Non-reversible hint: up to four leading graphic bytes, then the total length.
fn redact(span: &[u8]) -> String {
    let prefix: String = span
        .iter()
        .take(4)
        .map(|&b| if b.is_ascii_graphic() { b as char } else { '.' })
        .collect();
    format!("{prefix}…<len={}>", span.len())
}

fn line_of(data: &[u8], offset: usize) -> usize {
    1 + data[..offset].iter().filter(|&&b| b == b'\n').count()
}

fn is_allowed(allows: &[CompiledAllow], rule_id: &str, span: &[u8]) -> bool {
    allows.iter().any(|a| {
        if a.rule != "*" && a.rule != rule_id {
            return false;
        }
        match &a.matcher {
            AllowMatch::Exact(v) => v.as_slice() == span,
            AllowMatch::Pattern(re) => re.is_match(span),
        }
    })
}

/// One-pass `RegexSet` to learn which rules hit, then `find_iter` per hit to recover spans.
fn scan_with(
    set: &RegexSet,
    rules: &[CompiledRule],
    data: &[u8],
    allows: &[CompiledAllow],
    file: Option<&str>,
) -> Vec<Finding> {
    let mut findings = Vec::new();
    for idx in set.matches(data).iter() {
        let rule = &rules[idx];
        for m in rule.re.find_iter(data) {
            let span = &data[m.start()..m.end()];
            if is_allowed(allows, &rule.id, span) {
                continue;
            }
            let location = match file {
                Some(f) => format!("{f}:{}", line_of(data, m.start())),
                None => format!("offset {}", m.start()),
            };
            findings.push(Finding {
                rule_id: rule.id.clone(),
                severity: rule.severity,
                description: rule.description.clone(),
                redacted: redact(span),
                location,
            });
        }
    }
    findings
}

/// Print findings to stderr (redacted) and return an exit code: 1 if any `block`, else 0.
fn report(findings: &[Finding]) -> i32 {
    let mut blocked = false;
    for f in findings {
        let tag = match f.severity {
            Severity::Block => {
                blocked = true;
                "BLOCK"
            }
            Severity::Warn => "WARN",
        };
        eprintln!(
            "{tag} {}: {} [{}] (redacted: {})",
            f.rule_id, f.description, f.location, f.redacted
        );
    }
    if blocked {
        1
    } else {
        0
    }
}

fn read_stdin_bytes(cap: usize) -> Result<Vec<u8>, String> {
    // Bound the allocation: take(cap+1) caps the read, so a giant/hostile stdin cannot be fully
    // read into memory before the size check runs. cap+1 distinguishes "exactly at cap" from "over".
    let mut buf = Vec::new();
    std::io::stdin()
        .lock()
        .take(cap as u64 + 1)
        .read_to_end(&mut buf)
        .map_err(|e| format!("stdin read error: {e}"))?;
    if buf.len() > cap {
        return Err(format!("input exceeds {cap}-byte cap"));
    }
    Ok(buf)
}

#[cfg(test)]
mod match_tests {
    use super::*;

    fn rules() -> Rules {
        // One content rule + a span-scoped allow for the AWS example key.
        let toml = "schema_version = 1\n\n[[rule]]\nid = \"aws\"\nkind = \"content\"\nseverity = \"block\"\ndescription = \"AWS key\"\npattern = '\\b(AKIA|ASIA)[0-9A-Z]{16}\\b'\n\n[[allow]]\nrule = \"aws\"\nvalue = \"AKIAIOSFODNN7EXAMPLE\"\n";
        parse_rules(toml).unwrap()
    }

    #[test]
    fn blocks_planted_aws_key() {
        let r = rules();
        let key = format!("AKIA{}", "1234567890ABCDEF"); // built by concat; 20 chars total
        let payload = format!("export AWS_KEY={key}\n");
        let f = scan_with(
            &r.content_set,
            &r.content,
            payload.as_bytes(),
            &r.allows,
            None,
        );
        assert_eq!(f.len(), 1);
        assert_eq!(report(&f), 1);
        // The raw key never appears in the redacted hint.
        assert!(!f[0].redacted.contains(&key));
        assert!(f[0].redacted.starts_with("AKIA…<len=20>"));
    }
    #[test]
    fn clean_input_passes() {
        let r = rules();
        let f = scan_with(
            &r.content_set,
            &r.content,
            b"nothing to see here\n",
            &r.allows,
            None,
        );
        assert!(f.is_empty());
        assert_eq!(report(&f), 0);
    }
    #[test]
    fn allow_is_span_scoped() {
        let r = rules();
        // The exact example key is allowed -> no finding ...
        let f = scan_with(
            &r.content_set,
            &r.content,
            b"AKIAIOSFODNN7EXAMPLE\n",
            &r.allows,
            None,
        );
        assert!(f.is_empty());
        // ... but a different real key on the same line still blocks.
        let key = format!("AKIA{}", "ZZ34567890ABCDEF");
        let line = format!("AKIAIOSFODNN7EXAMPLE and {key}\n");
        let f2 = scan_with(&r.content_set, &r.content, line.as_bytes(), &r.allows, None);
        assert_eq!(f2.len(), 1);
    }
    #[test]
    fn matches_non_utf8_bytes() {
        let r = rules();
        let mut payload = vec![0xff, 0xfe, 0x00, b'\n']; // invalid UTF-8 + NUL
        payload.extend_from_slice(format!("AKIA{}", "1234567890ABCDEF").as_bytes());
        let f = scan_with(&r.content_set, &r.content, &payload, &r.allows, None);
        assert_eq!(f.len(), 1, "byte-regex must scan non-UTF8/NUL input");
    }
    #[test]
    fn crlf_content_still_detected() {
        let r = rules();
        let key = format!("AKIA{}", "1234567890ABCDEF");
        let cr = char::from(13u8); // carriage return — built from a code point, no escape
        let lf = char::from(10u8); // line feed
                                   // CRLF endings must not hide the secret, and the reported line must be correct.
        let payload = format!("line one{cr}{lf}KEY={key}{cr}{lf}last{cr}{lf}");
        let f = scan_with(
            &r.content_set,
            &r.content,
            payload.as_bytes(),
            &r.allows,
            Some("f"),
        );
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].location, "f:2", "secret is on line 2 even with CRLF");
    }
    #[test]
    fn perf_5mib_under_generous_ceiling() {
        // Deterministic GATE (not a p95 assertion): a ~1000x-margin ceiling that only trips on an
        // architectural blowup — O(n^2), a per-call recompile, or catastrophic backtracking.
        let r = rules();
        let mut data = Vec::with_capacity(5 * 1024 * 1024 + 32);
        while data.len() < 5 * 1024 * 1024 {
            data.extend_from_slice(b"benign line, nothing here to match at all\n");
        }
        data.extend_from_slice(format!("AKIA{}", "1234567890ABCDEF").as_bytes());
        let t = std::time::Instant::now();
        let f = scan_with(&r.content_set, &r.content, &data, &r.allows, None);
        assert_eq!(f.len(), 1, "planted key at EOF is found");
        assert!(
            t.elapsed().as_secs() < 2,
            "5 MiB scan must stay well under 2s, took {:?}",
            t.elapsed()
        );
    }
    #[test]
    fn perf_partial_match_storm_stays_linear() {
        // A storm of near-matches that would thrash a backtracking engine; the linear-time
        // RegexSet must shrug it off (proves the no-look-around property in practice).
        let r = rules();
        let data = "AKIA1 ".repeat(200_000); // ~1.2 MiB of incomplete AWS-key prefixes
        let t = std::time::Instant::now();
        let f = scan_with(&r.content_set, &r.content, data.as_bytes(), &r.allows, None);
        assert!(f.is_empty(), "no complete key -> no finding");
        assert!(
            t.elapsed().as_secs() < 2,
            "partial-match storm must stay linear"
        );
    }
}

#[cfg(test)]
mod load_tests {
    use super::*;

    const VALID: &str = "schema_version = 1\n\n[[rule]]\nid = \"k\"\nkind = \"content\"\nseverity = \"block\"\ndescription = \"d\"\npattern = '\\bAKIA[0-9A-Z]{16}\\b'\n";

    #[test]
    fn valid_rules_load() {
        let r = parse_rules(VALID).unwrap();
        assert_eq!(r.content.len(), 1);
        assert_eq!(r.command.len(), 0);
    }
    #[test]
    fn bad_schema_version_rejected() {
        assert!(
            parse_rules(&VALID.replacen("schema_version = 1", "schema_version = 9", 1)).is_err()
        );
    }
    #[test]
    fn unknown_field_rejected() {
        assert!(parse_rules(&VALID.replacen(
            "description = \"d\"",
            "description = \"d\"\nbogus = 1",
            1
        ))
        .is_err());
    }
    #[test]
    fn bad_kind_rejected() {
        assert!(
            parse_rules(&VALID.replacen("kind = \"content\"", "kind = \"nonsense\"", 1)).is_err()
        );
    }
    #[test]
    fn bad_severity_rejected() {
        assert!(
            parse_rules(&VALID.replacen("severity = \"block\"", "severity = \"loud\"", 1)).is_err()
        );
    }
    #[test]
    fn duplicate_id_rejected() {
        let dup = format!("{VALID}\n[[rule]]\nid = \"k\"\nkind = \"content\"\nseverity = \"block\"\ndescription = \"d2\"\npattern = 'x'\n");
        assert!(parse_rules(&dup).is_err());
    }
    #[test]
    fn uncompilable_pattern_names_id() {
        let bad = VALID.replacen("'\\bAKIA[0-9A-Z]{16}\\b'", "'(unclosed'", 1);
        let err = parse_rules(&bad).unwrap_err();
        assert!(
            err.contains("'k'"),
            "error should name the offending rule id: {err}"
        );
    }
    #[test]
    fn allow_star_without_value_rejected() {
        let bad = format!("{VALID}\n[[allow]]\nrule = \"*\"\n");
        assert!(parse_rules(&bad).is_err());
    }
    #[test]
    fn allow_with_value_ok() {
        let ok = format!("{VALID}\n[[allow]]\nrule = \"k\"\nvalue = \"AKIAIOSFODNN7EXAMPLE\"\n");
        assert!(parse_rules(&ok).is_ok());
    }
}
