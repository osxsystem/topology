//! Security scanning — the deterministic safety floor.
//!
//! Matches a versioned `security/rules.toml` against stdin-delivered inputs. Two rule kinds:
//! `content` (secrets, run on every input) and `command` (dangerous shells, run only on command
//! strings). The scanner never emits a matched value — diagnostics carry a redacted hint only.
//! See docs/specs/2026-06-06-security-scanning.md.

use std::collections::HashSet;
use std::fs;
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
