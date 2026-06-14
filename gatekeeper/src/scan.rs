//! Security scanning — the deterministic safety floor.
//!
//! Matches a versioned `security/rules.toml` against stdin-delivered inputs. Two rule kinds:
//! `content` (secrets, run on every input) and `command` (dangerous shells, run only on command
//! strings). The scanner never emits a matched value — diagnostics carry a redacted hint only.
//! See docs/specs/2026-06-06-security-scanning.md.

use std::collections::HashSet;
use std::fs;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use regex::bytes::{Regex, RegexSet};
use serde::Deserialize;

const SCHEMA_VERSION: u32 = 2;

/// Expose the rules schema version to sibling modules without leaking the private const.
pub fn schema_version() -> u32 {
    SCHEMA_VERSION
}

/// PreToolUse inputs are latency-sensitive; cap at 5 MiB.
const HOOK_INPUT_CAP: usize = 5 * 1024 * 1024;
/// Pre-commit blobs can be large; cap generously at 50 MiB, over-cap blocks unless allowlisted.
const STAGED_BLOB_CAP: usize = 50 * 1024 * 1024;

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
    #[serde(default)]
    scan: ScanConfig,
}

/// `[scan]` table. Carries path globs whose files are exempt from the entropy lane (only). Regex
/// (content/command) rules always run — excludes never weaken labeled detection.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct ScanConfig {
    #[serde(default)]
    exclude_paths: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRule {
    id: String,
    kind: Kind,
    severity: Severity,
    description: String,
    // Regex rules (content/command) carry a `pattern`; entropy rules carry charset/length/threshold
    // instead. Optional so a v2 entropy rule parses; presence is validated per-kind in parse_rules.
    #[serde(default)]
    pattern: Option<String>,
    #[serde(default)]
    charset: Option<String>,
    #[serde(default)]
    min_length: Option<usize>,
    #[serde(default)]
    threshold_bits_per_char: Option<f64>,
    // path-mutation rules carry a list of protected path substrings instead of a regex `pattern`;
    // detection is the quote-aware tokenizer in `detect_path_mutation`, not a RegexSet.
    #[serde(default)]
    protected: Option<Vec<String>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Kind {
    Content,
    Command,
    Entropy,
    #[serde(rename = "path-mutation")]
    PathMutation,
}

impl Kind {
    fn as_str(self) -> &'static str {
        match self {
            Kind::Content => "content",
            Kind::Command => "command",
            Kind::Entropy => "entropy",
            Kind::PathMutation => "path-mutation",
        }
    }
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
    // Human documentation only: accepted + validated by `deny_unknown_fields`, never read by logic.
    #[serde(default)]
    #[allow(dead_code)]
    reason: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AllowBlob {
    path: String,
    blob_oid: String,
    // Human documentation only: accepted + validated by `deny_unknown_fields`, never read by logic.
    #[serde(default)]
    #[allow(dead_code)]
    reason: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Integrity {
    #[serde(default)]
    protected_paths: Vec<String>,
    // Introduced in the protected-path split (ADR-0013): entries resolved against the artifacts
    // root at runtime instead of the framework root.  Older committed rules.toml files that lack
    // this key still parse cleanly via the serde default.
    #[serde(default)]
    protected_artifact_paths: Vec<String>,
}

// ---------- compiled model ----------

#[derive(Debug)]
struct CompiledRule {
    id: String,
    severity: Severity,
    description: String,
    re: Regex,
}

/// Token alphabet an entropy rule scans for. Parsed from the rule's `charset` string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Charset {
    Base64,
    Hex,
}

impl Charset {
    fn parse(s: &str) -> Result<Charset, String> {
        match s {
            "base64" => Ok(Charset::Base64),
            "hex" => Ok(Charset::Hex),
            other => Err(format!(
                "unknown charset '{other}' (expected base64 or hex)"
            )),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Charset::Base64 => "base64",
            Charset::Hex => "hex",
        }
    }
}

// Lets the test compare a compiled rule's charset against a string literal (`er.charset == "hex"`).
impl PartialEq<&str> for Charset {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

#[derive(Debug)]
struct CompiledEntropyRule {
    id: String,
    severity: Severity,
    description: String,
    charset: Charset,
    min_length: usize,
    threshold: f64,
}

/// A path-mutation rule: blocks a command iff the quote-aware tokenizer (`detect_path_mutation`)
/// finds a mutating verb whose operand — or a redirect target — path-normalizes to a string
/// containing one of `protected`. Detection lives in Rust; only the token LIST is data.
#[derive(Debug)]
struct CompiledPathMutationRule {
    id: String,
    severity: Severity,
    description: String,
    protected: Vec<String>,
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
    /// Entropy rules — a separate lane from the regex `RegexSet`, applied alongside `scan_with`.
    entropy: Vec<CompiledEntropyRule>,
    /// Path-mutation rules — a tokenizer lane (not regex), applied to command strings only,
    /// alongside the command `RegexSet`.
    path_mutation: Vec<CompiledPathMutationRule>,
    /// Path globs (from `[scan] exclude_paths`) whose files skip the entropy lane only.
    exclude_paths: Vec<String>,
    allows: Vec<CompiledAllow>,
    allow_blobs: Vec<AllowBlob>,
    /// Paths protected under the FRAMEWORK root (e.g. security/rules.toml, gatekeeper/src/…).
    protected: Vec<String>,
    /// Paths protected under the ARTIFACTS root (e.g. "memory" → <artifacts_root>/memory/).
    /// Split from `protected` so governed-project handoffs resolve against the project root, not
    /// the framework root.  See the protected-path bypass fix (ADR-0013).
    protected_artifacts: Vec<String>,
}

/// Read and fully validate the rules file at `path`.
pub fn load_rules(path: &Path) -> Result<Rules, String> {
    let raw =
        fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    parse_rules(&raw)
}

/// Public re-export of `parse_rules` for in-module tests in sibling modules.
#[cfg(test)]
pub fn parse_rules_pub(raw: &str) -> Result<Rules, String> {
    parse_rules(raw)
}

/// Validate + compile from TOML text. Any defect is an Err (the caller maps it to exit 2).
fn parse_rules(raw: &str) -> Result<Rules, String> {
    let parsed: RulesFile =
        toml::from_str(raw).map_err(|e| format!("rules.toml parse/validation error: {e}"))?;
    if parsed.schema_version != 1 && parsed.schema_version != 2 {
        return Err(format!(
            "unsupported schema_version {} (expected 1 or 2)",
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
    let mut entropy = Vec::new();
    let mut path_mutation = Vec::new();
    for r in &parsed.rule {
        match r.kind {
            Kind::Content | Kind::Command => {
                let pattern = r.pattern.as_deref().ok_or_else(|| {
                    format!(
                        "rule '{}': {} rule requires a 'pattern'",
                        r.id,
                        r.kind.as_str()
                    )
                })?;
                let re = Regex::new(pattern)
                    .map_err(|e| format!("rule '{}': invalid pattern: {e}", r.id))?;
                let cr = CompiledRule {
                    id: r.id.clone(),
                    severity: r.severity,
                    description: r.description.clone(),
                    re,
                };
                match r.kind {
                    Kind::Content => content.push(cr),
                    Kind::Command => command.push(cr),
                    _ => unreachable!(),
                }
            }
            Kind::PathMutation => {
                let protected = r
                    .protected
                    .as_deref()
                    .filter(|p| !p.is_empty())
                    .ok_or_else(|| {
                        format!(
                            "rule '{}': path-mutation rule requires a non-empty 'protected'",
                            r.id
                        )
                    })?;
                path_mutation.push(CompiledPathMutationRule {
                    id: r.id.clone(),
                    severity: r.severity,
                    description: r.description.clone(),
                    protected: protected.to_vec(),
                });
            }
            Kind::Entropy => {
                let charset_str = r
                    .charset
                    .as_deref()
                    .ok_or_else(|| format!("rule '{}': entropy rule requires a 'charset'", r.id))?;
                let charset =
                    Charset::parse(charset_str).map_err(|e| format!("rule '{}': {e}", r.id))?;
                entropy.push(CompiledEntropyRule {
                    id: r.id.clone(),
                    severity: r.severity,
                    description: r.description.clone(),
                    charset,
                    min_length: r.min_length.unwrap_or(20),
                    threshold: r.threshold_bits_per_char.unwrap_or(4.0),
                });
            }
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
        entropy,
        path_mutation,
        exclude_paths: parsed.scan.exclude_paths,
        allows,
        allow_blobs: parsed.allow_blob,
        protected: parsed.integrity.protected_paths,
        protected_artifacts: parsed.integrity.protected_artifact_paths,
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

/// Shannon entropy of `token` in bits per character: `H = -Σ p_i·log2(p_i)` over its distinct
/// chars (`p_i` = count_i / len). An empty token is `0.0` (no symbols → guard the div-by-zero).
fn shannon_entropy(token: &str) -> f64 {
    let len = token.chars().count();
    if len == 0 {
        return 0.0;
    }
    let mut counts: std::collections::HashMap<char, usize> = std::collections::HashMap::new();
    for c in token.chars() {
        *counts.entry(c).or_insert(0) += 1;
    }
    let len = len as f64;
    counts
        .values()
        .map(|&count| {
            let p = count as f64 / len;
            -p * p.log2()
        })
        .sum()
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
            // Must match the WHOLE finding span, not a substring of it — an unanchored allow
            // pattern must not exempt a larger secret that merely contains the allowed text.
            AllowMatch::Pattern(re) => re
                .find(span)
                .is_some_and(|m| m.start() == 0 && m.end() == span.len()),
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

/// True iff `c` belongs to the candidate alphabet for `charset`. Hex is the strict hex alphabet
/// (so prose/base64 cannot trip a hex rule); base64 is the URL/standard base64 superset.
fn in_charset(charset: Charset, c: u8) -> bool {
    match charset {
        Charset::Hex => c.is_ascii_hexdigit(),
        Charset::Base64 => {
            c.is_ascii_alphanumeric() || matches!(c, b'+' | b'/' | b'=' | b'_' | b'-')
        }
    }
}

/// Entropy lane: for each rule, walk maximal runs of charset bytes; a run of `>= min_length`
/// whose Shannon entropy clears the rule threshold becomes a `Finding` (through `is_allowed`).
fn scan_entropy(
    entropy: &[CompiledEntropyRule],
    data: &[u8],
    allows: &[CompiledAllow],
    file: Option<&str>,
) -> Vec<Finding> {
    let mut findings = Vec::new();
    for rule in entropy {
        let mut i = 0;
        while i < data.len() {
            if !in_charset(rule.charset, data[i]) {
                i += 1;
                continue;
            }
            let start = i;
            while i < data.len() && in_charset(rule.charset, data[i]) {
                i += 1;
            }
            let span = &data[start..i];
            if span.len() < rule.min_length {
                continue;
            }
            // The candidate run is charset-only ASCII, so a lossless str view is sound.
            let token = String::from_utf8_lossy(span);
            if shannon_entropy(&token) < rule.threshold {
                continue;
            }
            if is_allowed(allows, &rule.id, span) {
                continue;
            }
            let location = match file {
                Some(f) => format!("{f}:{}", line_of(data, start)),
                None => format!("offset {start}"),
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

// ---------- path-mutation tokenizer (replaces the tamper-* regexes; see scan-tamper-false-positive) ----------

/// Mutating command verbs: a write to a protected operand by one of these blocks. `sed` is handled
/// separately (only with an in-place flag), so it is not in this set.
const MUTATING_VERBS: &[&str] = &[
    "tee", "cp", "mv", "ln", "chmod", "rm", "dd", "install", "truncate",
];

/// Command wrappers skipped as prefix tokens so the verb after them is still classified. They run
/// another command, so the protected-write check applies to that command's verb, not the wrapper.
const WRAPPER_CMDS: &[&str] = &[
    "sudo", "doas", "env", "command", "builtin", "exec", "nohup", "setsid", "timeout", "nice",
    "ionice", "stdbuf", "xargs",
];

/// Shell keywords / control-flow words skipped as prefix tokens: the real verb of a simple-command
/// follows them (`then cp …`, `do rm …`, `! tee …`).
const SHELL_KEYWORDS: &[&str] = &[
    "if", "then", "elif", "else", "fi", "for", "while", "until", "do", "done", "case", "esac",
    "select", "in", "function", "time", "coproc", "!",
];

/// One lexed shell word: the concatenated literal content of its adjacent segments, plus whether
/// ANY segment was inside single/double quotes. A quoted word can never be a command verb (a quoted
/// `"tee"` is an argument). Backslash-escaping a byte (`\tee`) does NOT set `quoted`.
#[derive(Debug, Clone)]
struct LexWord {
    text: String,
    quoted: bool,
}

/// A lexed token: either a word, an unquoted control operator that separates simple-commands, or a
/// redirect whose target word follows it.
#[derive(Debug, Clone)]
enum LexTok {
    Word(LexWord),
    /// Unquoted separator (`;` `&` `&&` `||` `|` newline `(` `)` `{` `}` backtick `$(`).
    Sep,
    /// A redirect operator (`>`, `>>`, `>|`, `>&`, `2>`, `&>`, …): its target is the next word.
    Redir,
    /// Process substitution `>(…)` / `<(…)`: an argument (a `/dev/fd` path), not a redirect target
    /// and not a separator. The inner command is scanned recursively for nested writes.
    ProcSub(String),
    /// An input redirect (`<`, `<<`, `<<<`, `<&`): the following word is a READ source, never a write.
    InRedir,
}

/// Consume a balanced parenthesised group starting at `open` (where `cmd[open] == b'('`). Returns the
/// inner bytes (excluding the outer parens) and the index just past the matching `)`.
fn consume_balanced_parens(cmd: &[u8], open: usize) -> (String, usize) {
    let n = cmd.len();
    let mut depth = 0i32;
    let mut i = open;
    let mut inner = String::new();
    while i < n {
        let b = cmd[i];
        match b {
            // Quoted segments are kept in `inner` but their `(`/`)` do not move `depth` — a quoted
            // `")"` must not close the process-sub early (it would desync the paren scan).
            b'\'' | b'"' => {
                if depth > 0 {
                    inner.push(b as char);
                }
                i += 1;
                while i < n && cmd[i] != b {
                    if depth > 0 {
                        inner.push(cmd[i] as char);
                    }
                    i += 1;
                }
                if i < n {
                    if depth > 0 {
                        inner.push(b as char);
                    }
                    i += 1; // closing quote
                }
            }
            b'\\' => {
                if depth > 0 {
                    inner.push('\\');
                }
                i += 1;
                if i < n {
                    if depth > 0 {
                        inner.push(cmd[i] as char);
                    }
                    i += 1;
                }
            }
            b'(' => {
                if depth > 0 {
                    inner.push('(');
                }
                depth += 1;
                i += 1;
            }
            b')' => {
                depth -= 1;
                i += 1;
                if depth == 0 {
                    break;
                }
                inner.push(')');
            }
            _ => {
                if depth > 0 {
                    inner.push(b as char);
                }
                i += 1;
            }
        }
    }
    (inner, i)
}

/// Lex a command byte string into tokens with quote/escape awareness.
/// - single-quote `'…'`: literal, no escapes
/// - double-quote `"…"`: literal word content
/// - backslash `\`: outside single-quotes, escapes the next byte (kept literally, word stays unquoted)
/// - adjacent segments concatenate into one word (`a"b"c` → `abc`)
///
/// Redirect operators and the separators above are recognised only when UNQUOTED.
fn lex_command(cmd: &[u8]) -> Vec<LexTok> {
    let mut toks: Vec<LexTok> = Vec::new();
    let mut i = 0;
    let n = cmd.len();
    // Accumulator for the current word and whether it has started / was quoted.
    let mut cur = String::new();
    let mut cur_started = false;
    let mut cur_quoted = false;
    macro_rules! flush_word {
        () => {
            if cur_started {
                toks.push(LexTok::Word(LexWord {
                    text: std::mem::take(&mut cur),
                    quoted: cur_quoted,
                }));
                cur_started = false;
                cur_quoted = false;
            }
        };
    }
    while i < n {
        let b = cmd[i];
        match b {
            b'\'' => {
                // Single-quoted segment: literal until the next `'` (no escapes inside).
                cur_started = true;
                cur_quoted = true;
                i += 1;
                while i < n && cmd[i] != b'\'' {
                    cur.push(cmd[i] as char);
                    i += 1;
                }
                i += 1; // consume closing quote (tolerate unterminated: i == n)
            }
            b'"' => {
                // Double-quoted segment: literal word content (we do not expand `$`/backslash here;
                // a `$var` inside stays as the literal `$var`, which never path-normalizes to a
                // concrete protected path — the documented variable-built-path residual).
                cur_started = true;
                cur_quoted = true;
                i += 1;
                while i < n && cmd[i] != b'"' {
                    cur.push(cmd[i] as char);
                    i += 1;
                }
                i += 1;
            }
            b'\\' => {
                // Escape the next byte: keep it literally, word stays unquoted.
                cur_started = true;
                if i + 1 < n {
                    cur.push(cmd[i + 1] as char);
                    i += 2;
                } else {
                    i += 1;
                }
            }
            b' ' | b'\t' => {
                flush_word!();
                i += 1;
            }
            b'\n' | b';' | b'(' | b')' | b'{' | b'}' | b'`' => {
                flush_word!();
                toks.push(LexTok::Sep);
                i += 1;
            }
            b'$' if i + 1 < n && cmd[i + 1] == b'(' => {
                // `$(` opens a command substitution — a fresh simple-command context.
                flush_word!();
                toks.push(LexTok::Sep);
                i += 2;
            }
            b'&' => {
                flush_word!();
                // `&>` / `&>>` are redirects; `&` and `&&` are separators.
                if i + 1 < n && cmd[i + 1] == b'>' {
                    toks.push(LexTok::Redir);
                    i += 2;
                    if i < n && cmd[i] == b'>' {
                        i += 1;
                    }
                } else {
                    toks.push(LexTok::Sep);
                    i += 1;
                    if i < n && cmd[i] == b'&' {
                        i += 1; // `&&`
                    }
                }
            }
            b'|' => {
                flush_word!();
                toks.push(LexTok::Sep);
                i += 1;
                if i < n && cmd[i] == b'|' {
                    i += 1; // `||`
                }
            }
            b'>' | b'<' if i + 1 < n && cmd[i + 1] == b'(' => {
                // Process substitution `>(…)` / `<(…)`: an argument (a `/dev/fd` path), not a redirect
                // to a file and not a separator. Consume the balanced group as one throwaway token;
                // its inner command is scanned recursively (`tee >(cp x rules.toml)`).
                flush_word!();
                let (inner, next) = consume_balanced_parens(cmd, i + 1);
                toks.push(LexTok::ProcSub(inner));
                i = next;
            }
            b'>' => {
                flush_word!();
                toks.push(LexTok::Redir);
                i += 1;
                // Consume the redirect operator's trailing form: `>>`, `>|`, `>&`.
                if i < n && matches!(cmd[i], b'>' | b'|' | b'&') {
                    i += 1;
                }
            }
            b'<' => {
                // Input redirect (`<`, `<<`, `<<<`, `<&`): its target is a READ source, never a write.
                // Emit InRedir so the detector skips the following source word (not a written operand).
                flush_word!();
                toks.push(LexTok::InRedir);
                i += 1;
                if i < n && matches!(cmd[i], b'<' | b'&') {
                    i += 1;
                }
            }
            // A digit/`&` immediately followed by `>` is an fd-prefixed redirect (`2>`, `2>>`).
            b'0'..=b'9' if !cur_started && i + 1 < n && cmd[i + 1] == b'>' => {
                toks.push(LexTok::Redir);
                i += 2;
                if i < n && matches!(cmd[i], b'>' | b'|' | b'&') {
                    i += 1;
                }
            }
            _ => {
                cur_started = true;
                cur.push(b as char);
                i += 1;
            }
        }
    }
    // Final flush (inlined, consuming the accumulator — no reset needed at end of input; the macro's
    // in-loop resets stay live because this check still reads `cur_started`).
    if cur_started {
        toks.push(LexTok::Word(LexWord {
            text: cur,
            quoted: cur_quoted,
        }));
    }
    toks
}

/// Path-normalize a word for protected-substring matching: collapse repeated `/`, drop `.`
/// components, and resolve `..` lexically (`security/foo/../rules.toml` → `security/rules.toml`,
/// `security/./rules.toml` → `security/rules.toml`). Leading and trailing `/` are preserved so a
/// directory-prefix token like `docs/memory/` still matches. (Quotes are removed by the lexer.)
fn normalize_mutation_path(word: &str) -> String {
    let leading_slash = word.starts_with('/');
    let trailing_slash = word.len() > 1 && word.ends_with('/');
    let mut out: Vec<&str> = Vec::new();
    for comp in word.split('/') {
        match comp {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            c => out.push(c),
        }
    }
    let mut joined = out.join("/");
    if leading_slash {
        joined.insert(0, '/');
    }
    if trailing_slash && !joined.ends_with('/') {
        joined.push('/');
    }
    joined
}

/// True iff `word` (path-normalized) contains one of the rule's protected substrings.
fn writes_protected(word: &str, protected: &[String]) -> bool {
    let norm = normalize_mutation_path(word);
    protected.iter().any(|p| norm.contains(p.as_str()))
}

/// True iff `word` is an unquoted `VAR=value` assignment prefix (`^[A-Za-z_][A-Za-z0-9_]*=`).
fn is_assignment_prefix(word: &LexWord) -> bool {
    if word.quoted {
        return false;
    }
    let bytes = word.text.as_bytes();
    let Some(eq) = word.text.find('=') else {
        return false;
    };
    if eq == 0 {
        return false;
    }
    if !(bytes[0].is_ascii_alphabetic() || bytes[0] == b'_') {
        return false;
    }
    bytes[1..eq]
        .iter()
        .all(|&c| c.is_ascii_alphanumeric() || c == b'_')
}

/// Tokenized detection: block a command iff a simple-command's verb is mutating AND one of its
/// operands (or any redirect target in that simple-command) path-normalizes to a protected path.
/// One `Finding` per rule whose `protected` list matched.
fn detect_path_mutation(cmd: &[u8], rules: &[CompiledPathMutationRule]) -> Vec<Finding> {
    let mut findings = Vec::new();
    if rules.is_empty() {
        return findings;
    }
    let toks = lex_command(cmd);

    // Walk simple-commands (token runs between `Sep`s). For each, collect the verb word, its
    // operand words (flags excluded), whether the verb is `sed`-in-place, and all redirect targets.
    let mut idx = 0;
    while idx < toks.len() {
        // Gather one simple-command's tokens up to the next separator.
        let mut verb: Option<&str> = None;
        let mut sed_in_place = false;
        let mut operands: Vec<&str> = Vec::new();
        let mut redirect_targets: Vec<&str> = Vec::new();
        let mut verb_seen = false;

        while idx < toks.len() {
            match &toks[idx] {
                LexTok::Sep => {
                    idx += 1;
                    break;
                }
                LexTok::Redir => {
                    // The next word (if any) is this redirect's target.
                    idx += 1;
                    if let Some(LexTok::Word(w)) = toks.get(idx) {
                        redirect_targets.push(w.text.as_str());
                        idx += 1;
                    }
                }
                LexTok::ProcSub(inner) => {
                    // A write nested inside `>(…)` is still a write — scan it recursively. The
                    // process-sub is an argument, so it neither ends the simple-command nor counts
                    // as an operand of the outer verb.
                    findings.extend(detect_path_mutation(inner.as_bytes(), rules));
                    idx += 1;
                }
                LexTok::InRedir => {
                    // The following word is a READ source (`tee out < rules.toml` reads rules.toml),
                    // not a written operand — skip it so it is not mistaken for a write.
                    idx += 1;
                    if let Some(LexTok::Word(_)) = toks.get(idx) {
                        idx += 1;
                    }
                }
                LexTok::Word(w) => {
                    if !verb_seen {
                        // Prefix-skipping: keywords, wrappers, VAR=val assignments, option flags.
                        let is_keyword = !w.quoted && SHELL_KEYWORDS.contains(&w.text.as_str());
                        let is_wrapper = !w.quoted && WRAPPER_CMDS.contains(&w.text.as_str());
                        let is_flag = !w.quoted && w.text.starts_with('-');
                        if is_keyword || is_wrapper || is_assignment_prefix(w) || is_flag {
                            idx += 1;
                            continue;
                        }
                        // First non-prefix word is the VERB — even if quoted: the shell removes the
                        // quotes and runs it (`"tee" f`, `'cp' a b` are real commands). Quote-awareness
                        // only matters for telling the verb from later argument words, which the
                        // first-word-wins position already handles.
                        verb_seen = true;
                        // Use the command basename so a path-qualified verb (`/bin/cp`, `./rm`,
                        // `../bin/tee`) is still recognized — the shell runs the trailing program name.
                        verb = Some(w.text.rsplit('/').next().unwrap_or(w.text.as_str()));
                        idx += 1;
                    } else {
                        // After the verb: detect sed's in-place flag. Flags are NOT dropped — a flag
                        // can carry a write target (`cp --target-directory=DIR`, `cp -tDIR`), so every
                        // post-verb word is a candidate path for the protected check.
                        if !w.quoted
                            && w.text.starts_with('-')
                            && verb == Some("sed")
                            && is_sed_in_place_flag(&w.text)
                        {
                            sed_in_place = true;
                        }
                        operands.push(w.text.as_str());
                        idx += 1;
                    }
                }
            }
        }

        // Is this simple-command's verb a write?
        let mutating = match verb {
            Some(v) => MUTATING_VERBS.contains(&v) || (v == "sed" && sed_in_place),
            None => false,
        };

        for rule in rules {
            // A redirect into a protected target is a write regardless of the verb (`:> rules.toml`,
            // `echo x 2> rules.toml`). A mutating verb writing a protected operand is a write too.
            let redirect_hit = redirect_targets
                .iter()
                .any(|t| writes_protected(t, &rule.protected));
            let operand_hit = mutating
                && operands
                    .iter()
                    .any(|o| writes_protected(o, &rule.protected));
            if redirect_hit || operand_hit {
                findings.push(Finding {
                    rule_id: rule.id.clone(),
                    severity: rule.severity,
                    description: rule.description.clone(),
                    redacted: redact(cmd),
                    location: "offset 0".to_string(),
                });
            }
        }
    }
    findings
}

/// True iff `flag` is a `sed` in-place flag: `-i`, `-i.bak`, `-ie`, etc. (a `-` then an `i`, with
/// any suffix, no other letters before the `i`).
fn is_sed_in_place_flag(flag: &str) -> bool {
    if flag == "--in-place" || flag.starts_with("--in-place=") {
        return true;
    }
    // GNU sed parses short options getopt-style, so `-i` enables in-place anywhere in a leading
    // `-<letters>` bundle (`-i`, `-ri`, `-ni`, `-Ei`, `-i.bak`). Scan the option letters until a
    // non-letter (a `.bak`-style suffix or `=`) ends the bundle.
    let bytes = flag.as_bytes();
    if bytes.len() < 2 || bytes[0] != b'-' || bytes[1] == b'-' {
        return false;
    }
    for &b in &bytes[1..] {
        if b == b'i' {
            return true;
        }
        if !b.is_ascii_alphabetic() {
            break;
        }
    }
    false
}

#[cfg(test)]
mod path_mutation_tests {
    use super::*;

    fn rule(protected: &[&str]) -> CompiledPathMutationRule {
        CompiledPathMutationRule {
            id: "t".into(),
            severity: Severity::Block,
            description: "t".into(),
            protected: protected.iter().map(|s| s.to_string()).collect(),
        }
    }
    fn blocks(cmd: &str, protected: &[&str]) -> bool {
        !detect_path_mutation(cmd.as_bytes(), &[rule(protected)]).is_empty()
    }
    const P: &[&str] = &["security/rules.toml"];

    #[test]
    fn quoted_verb_in_command_position_blocks() {
        assert!(blocks("\"tee\" security/rules.toml", P));
        assert!(blocks("'cp' /tmp/x security/rules.toml", P));
    }
    #[test]
    fn quoted_verb_as_argument_does_not_block() {
        // grep's pattern is an argument, not the command verb.
        assert!(!blocks("grep \"tee\" security/rules.toml", P));
        assert!(!blocks("grep -n tee security/rules.toml", P));
    }
    #[test]
    fn keyword_and_wrapper_prefixes_are_skipped() {
        assert!(blocks("if true; then cp /tmp/x security/rules.toml; fi", P));
        assert!(blocks("for f in a; do rm security/rules.toml; done", P));
        assert!(blocks(
            "case $x in y) cp /tmp/z security/rules.toml ;; esac",
            P
        ));
        assert!(blocks("! tee security/rules.toml", P));
        assert!(blocks("sudo -- tee security/rules.toml", P));
        assert!(blocks("env FOO=1 nohup cp a security/rules.toml", P));
    }
    #[test]
    fn redirect_target_blocks_regardless_of_verb() {
        assert!(blocks("echo x > security/rules.toml", P));
        assert!(blocks("echo x >| security/rules.toml", P));
        assert!(blocks("echo x 2> security/rules.toml", P));
        assert!(!blocks("echo x 2>/dev/null", P));
        assert!(!blocks("grep x security/rules.toml 2>/dev/null", P)); // read with stderr redirect
    }
    #[test]
    fn path_normalization_collapses_slashes_and_dot() {
        assert!(blocks("cp a security//rules.toml", P));
        assert!(blocks("cp a ./security/rules.toml", P));
        assert_eq!(
            normalize_mutation_path("security//rules.toml"),
            "security/rules.toml"
        );
        assert_eq!(
            normalize_mutation_path("./security/rules.toml"),
            "security/rules.toml"
        );
        assert_eq!(
            normalize_mutation_path("security/./rules.toml"),
            "security/rules.toml"
        );
        assert_eq!(
            normalize_mutation_path("security/foo/../rules.toml"),
            "security/rules.toml"
        );
        assert_eq!(normalize_mutation_path("docs/memory/"), "docs/memory/");
        assert!(blocks("cp a security/./rules.toml", P));
        assert!(blocks("cp a security/foo/../rules.toml", P));
    }
    #[test]
    fn process_substitution_handled() {
        // The `>(…)` argument must not orphan the real operand, and a write nested inside it counts.
        assert!(blocks("tee >(cat) security/rules.toml", P));
        assert!(blocks("echo x >(cp /tmp/y security/rules.toml)", P));
        assert!(!blocks("tee >(cat) /tmp/out", P));
        // A quoted `)` inside the process-sub must not close it early (quote-aware paren scan).
        assert!(blocks(
            "echo x >(echo \")\"; cp /tmp/y security/rules.toml)",
            P
        ));
    }
    #[test]
    fn flag_carrying_path_is_checked() {
        // A flag may carry a write target; it must not be dropped before the protected check.
        assert!(blocks(
            "cp --target-directory=gatekeeper/src/ x",
            &["gatekeeper/src/"]
        ));
        assert!(blocks("cp -tgatekeeper/src/ x", &["gatekeeper/src/"]));
        assert!(blocks(
            "install --target-directory=docs/memory/ x",
            &["docs/memory/"]
        ));
    }
    #[test]
    fn input_redirect_source_is_a_read() {
        assert!(!blocks("tee /tmp/out < security/rules.toml", P));
        assert!(!blocks("dd < security/rules.toml of=/tmp/x", P));
        // …but a real OUTPUT write to the same path still blocks.
        assert!(blocks("tee security/rules.toml < /tmp/x", P));
    }
    #[test]
    fn path_qualified_verb_is_basenamed() {
        assert!(blocks("/bin/cp /tmp/x security/rules.toml", P));
        assert!(blocks("./rm security/rules.toml", P));
        assert!(blocks("/usr/bin/tee security/rules.toml", P));
        assert!(blocks("../bin/cp /tmp/x security/rules.toml", P));
    }
    #[test]
    fn mutating_verb_with_protected_operand_blocks_conservatively() {
        // Documented conservative over-block: a mutating verb touching a protected path in ANY
        // operand position blocks — correct for `mv`/`rm`/`sed -i` (which mutate the path), a
        // fail-closed over-block for `cp <protected> dest` (which only reads it). To READ a protected
        // file, use a non-mutating verb (`cat`/`grep`/`less`), which is allowed.
        assert!(blocks("cp security/rules.toml /tmp/backup", P)); // over-block (read source) — accepted
        assert!(blocks("mv security/rules.toml /tmp/x", P)); // correct: mv removes the source
    }
    #[test]
    fn variable_built_path_is_residual_allow() {
        // Documented residual: a runtime-resolved path can't be matched statically.
        assert!(!blocks("d=security/rules.toml; cp /tmp/x $d", P));
    }
    #[test]
    fn assignment_prefix_detection() {
        let w = |t: &str, q: bool| LexWord {
            text: t.into(),
            quoted: q,
        };
        assert!(is_assignment_prefix(&w("FOO=1", false)));
        assert!(!is_assignment_prefix(&w("FOO=1", true)));
        assert!(!is_assignment_prefix(&w("=x", false)));
        assert!(!is_assignment_prefix(&w("cp", false)));
    }
    #[test]
    fn sed_in_place_only_when_in_place_flag() {
        assert!(blocks("sed -i s/a/b/ security/rules.toml", P));
        assert!(blocks("sed --in-place s/a/b/ security/rules.toml", P));
        assert!(blocks("sed -ri s/a/b/ security/rules.toml", P)); // bundled, i not first
        assert!(blocks("sed -Ei s/a/b/ security/rules.toml", P));
        assert!(blocks("sed -i.bak s/a/b/ security/rules.toml", P)); // suffix form
        assert!(!blocks("sed -n 5p security/rules.toml", P)); // -n is not in-place
        assert!(!blocks("sed -E s/a/b/ security/rules.toml", P));
    }
}

/// Dep-free path glob: `*` matches any run of characters (including none); a trailing-`/` glob is a
/// directory prefix matching any path beneath it. Sufficient for `*.lock`/`*.min.js`/`tests/fixtures/`.
fn glob_match(path: &str, glob: &str) -> bool {
    if let Some(prefix) = glob.strip_suffix('/') {
        return path == prefix || path.starts_with(&format!("{prefix}/"));
    }
    // Split on `*`; each literal segment must appear in order, with the first anchored at the
    // start and the last anchored at the end (a `*`-free glob therefore matches exactly).
    let parts: Vec<&str> = glob.split('*').collect();
    let mut pos = 0;
    for (idx, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if idx == 0 {
            if !path[pos..].starts_with(part) {
                return false;
            }
            pos += part.len();
        } else if idx == parts.len() - 1 {
            // Last segment: must end the string (and not overlap an already-consumed prefix).
            return path[pos..].ends_with(part) && path.len() - pos >= part.len();
        } else {
            match path[pos..].find(part) {
                Some(off) => pos += off + part.len(),
                None => return false,
            }
        }
    }
    // No trailing literal to anchor: a glob ending in `*` (or all-`*`) matches the remainder.
    glob.ends_with('*') || pos == path.len()
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

/// Entry point for `gatekeeper scan ...`. `root` is the framework root; `artifacts_root` is the
/// artifacts root for the current project (<project>/docs when project == framework, else
/// <project>/.claude/topology). Returns the process exit code (0 clean / 1 veto / 2 usage or
/// load error). Rules load first so a broken rules file fails closed (exit 2) on every subcommand.
pub fn cmd_scan(args: &[String], root: &Path, artifacts_root: &Path, project_root: &Path) -> i32 {
    // Handle --help / -h before loading rules (avoid unnecessary I/O).
    if args.first().map(String::as_str) == Some("--help")
        || args.first().map(String::as_str) == Some("-h")
    {
        println!("{}", crate::lookup_usage("scan"));
        return 0;
    }

    let rules_path = root.join("security").join("rules.toml");
    let rules = match load_rules(&rules_path) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("gatekeeper scan: cannot load {}: {e}", rules_path.display());
            return 2;
        }
    };
    // Compute the cwd ONCE here, before dispatching to subcommands.  The hook and check-path lanes
    // receive relative paths whose base is the writer's own working directory; hook events from
    // Claude Code carry absolute paths (so the base only affects relative spellings), and the hook
    // wrapper preserves the project cwd so that a relative path is resolved honestly against the
    // caller's location — not hard-wired to the framework root.  We compute this here so that unit
    // tests can pass an explicit base without depending on or mutating the process cwd.
    let cwd = std::env::current_dir().unwrap_or_else(|_| root.to_path_buf());
    match args.first().map(String::as_str) {
        Some("--hook") => scan_hook(&rules, root, artifacts_root, &cwd),
        Some("--cmd") => scan_cmd_cmd(&rules),
        Some("--check-path") => scan_check_path(
            &rules,
            root,
            artifacts_root,
            &cwd,
            args.get(1).map(String::as_str),
        ),
        Some("--staged") => {
            scan_staged(&rules, root, project_root, artifacts_root, STAGED_BLOB_CAP)
        }
        Some("--content") => scan_content_cmd(&rules),
        Some(other) if other.starts_with('-') => {
            eprintln!(
                "gatekeeper scan: unknown flag '{other}'\n{}",
                crate::lookup_usage("scan")
            );
            2
        }
        _ => {
            eprintln!(
                "gatekeeper scan: expected --hook | --cmd | --content | --staged | --check-path <path>"
            );
            2
        }
    }
}

/// Join shell line-continuations (`\<newline>`) so command rules see the command the way the shell
/// executes it. Without this, `git push origin main \<newline> --force` reads as two lines and the
/// `.`/filler in a command pattern never spans the break — a fail-open. Applied to command-rule
/// input only; content/secret scanning still runs on the raw bytes.
fn strip_line_continuations(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    let mut i = 0;
    while i < data.len() {
        if data[i] == b'\\' && i + 1 < data.len() {
            if data[i + 1] == b'\n' {
                i += 2;
                continue;
            }
            if data[i + 1] == b'\r' && i + 2 < data.len() && data[i + 2] == b'\n' {
                i += 3;
                continue;
            }
        }
        out.push(data[i]);
        i += 1;
    }
    out
}

/// Returns `Ok(())` if no block-severity content rule fires; `Err(redacted_hint)` on the FIRST
/// block match (hint only — the matched value is never returned). Runs on raw bytes so
/// NUL / non-UTF-8 input is handled identically to `scan_content_cmd`.
pub fn scan_bytes_for_secrets(rules: &Rules, bytes: &[u8]) -> Result<(), String> {
    for idx in rules.content_set.matches(bytes).iter() {
        let rule = &rules.content[idx];
        if rule.severity != Severity::Block {
            continue;
        }
        for m in rule.re.find_iter(bytes) {
            let span = &bytes[m.start()..m.end()];
            if is_allowed(&rules.allows, &rule.id, span) {
                continue;
            }
            return Err(format!(
                "{}: {} (redacted: {})",
                rule.id,
                rule.description,
                redact(span)
            ));
        }
    }
    Ok(())
}

fn scan_content_cmd(rules: &Rules) -> i32 {
    let data = match read_stdin_bytes(HOOK_INPUT_CAP) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("BLOCK oversize-input: {e}");
            return 1; // fail closed
        }
    };
    let mut findings = scan_with(
        &rules.content_set,
        &rules.content,
        &data,
        &rules.allows,
        None,
    );
    findings.extend(scan_entropy(&rules.entropy, &data, &rules.allows, None));
    report(&findings)
}

fn scan_cmd_cmd(rules: &Rules) -> i32 {
    let data = match read_stdin_bytes(HOOK_INPUT_CAP) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("BLOCK oversize-input: {e}");
            return 1;
        }
    };
    // This is a COMMAND string: the shell joins line-continuations before executing, so both
    // content and command rules must see the joined form (a secret split across `\<newline>` is one
    // key to the shell). (File/blob content, by contrast, is scanned raw — there is no shell there.)
    let cmd = strip_line_continuations(&data);
    let mut findings = scan_with(
        &rules.content_set,
        &rules.content,
        &cmd,
        &rules.allows,
        None,
    );
    findings.extend(scan_with(
        &rules.command_set,
        &rules.command,
        &cmd,
        &rules.allows,
        None,
    ));
    findings.extend(scan_entropy(&rules.entropy, &cmd, &rules.allows, None));
    findings.extend(detect_path_mutation(&cmd, &rules.path_mutation));
    report(&findings)
}

/// Lexically normalize a relative path for comparison: forward slashes, `.`/`..`/empty resolved.
/// Used for repo-relative comparisons (e.g. the blob allowlist) where both sides come from git.
fn normalize_path(p: &str) -> String {
    let unified = p.replace('\\', "/");
    let mut out: Vec<&str> = Vec::new();
    for seg in unified.split('/') {
        match seg {
            "" | "." => {} // leading ./, doubled //, trailing /
            ".." => {
                out.pop(); // climb out of the previous segment
            }
            s => out.push(s),
        }
    }
    out.join("/")
}

/// Resolve a path to an absolute, lexically-normalized form against `root` (no filesystem access,
/// no symlink following). Relative inputs are joined onto root; `.`/`..` are resolved on the FULL
/// absolute path, so a parent-and-return alias (`<root>/../<root-dir>/security/rules.toml`) and an
/// internal alias (`security/../security/rules.toml`) BOTH collapse to the real protected path and
/// cannot dodge the `ask`/veto.
fn resolve_against_root(root: &Path, p: &str) -> PathBuf {
    let unified = p.replace('\\', "/");
    let joined = if Path::new(&unified).is_absolute() {
        PathBuf::from(unified)
    } else {
        root.join(unified)
    };
    let mut pb = PathBuf::new();
    for comp in joined.components() {
        match comp {
            Component::ParentDir => {
                pb.pop();
            }
            Component::CurDir => {}
            other => pb.push(other.as_os_str()),
        }
    }
    pb
}

/// Returns true if `target` (already resolved to an absolute, normalised path) matches any entry
/// in `protected` resolved against `anchor_root`.  Keeping target resolution separate from entry
/// resolution is the fix for the staged-lane double-prefix bug: in the staged lane the git-emitted
/// path must be resolved against the *framework root* (the repo root), not against whatever anchor
/// root a given protected set uses.  If both the target and the entry are resolved against the same
/// root the caller must pass the same path for both; but when those roots differ (staging lane vs
/// hook lane) this function only resolves the ENTRIES — the caller resolves the target once and
/// passes it in.
fn is_protected_with_target(anchor_root: &Path, protected: &[String], target: &Path) -> bool {
    protected.iter().any(|p| {
        let pr = resolve_against_root(anchor_root, p);
        // Exact match OR target is strictly beneath the protected directory.
        // Path::starts_with is component-wise: `docs/memory` matches
        // `docs/memory/x.md` but NOT `docs/memory-evil/x.md`.
        target == pr || target.starts_with(&pr)
    })
}

/// Convenience wrapper: resolve `path` against `root` and check it against the protected entries
/// also anchored at `root`.  All callers where the target base and the entry anchor are the SAME
/// root (e.g. checking a framework-anchored entry with a framework-rooted path) should use this
/// form — it preserves the original semantics exactly and keeps the call sites concise.
///
/// Production code routes through `is_protected_any` (which resolves the target once against an
/// explicit `target_base`); this wrapper is used directly by unit tests that exercise single-set
/// semantics.
#[cfg_attr(not(test), allow(dead_code))]
fn is_protected(root: &Path, protected: &[String], path: &str) -> bool {
    let target = resolve_against_root(root, path);
    is_protected_with_target(root, protected, &target)
}

/// Check protection against BOTH the framework-anchored set and the artifacts-anchored set.
///
/// `target_base` is the root against which the *input path* is resolved — distinct from the
/// per-set *anchor roots* (framework_root for framework entries, artifacts_root for artifact
/// entries).  Separating the two prevents the double-prefix bug: in the staged integrity lane git
/// emits repo-root-relative paths, so `target_base` = framework_root (the repo root), while the
/// artifact entries must still be resolved against `artifacts_root` (e.g. `<repo>/docs`).  If
/// target_base == framework_root (the common case for the hook and check-path lanes) the
/// framework-anchored check degenerates to the original `is_protected` call.
///
/// A path is protected if it matches EITHER set; the two sets resolve their entries against
/// distinct anchor roots so that governed-project handoffs (artifacts_root =
/// <project>/.claude/topology) are not silently bypassed when framework_root is a different
/// directory.
fn is_protected_any(
    target_base: &Path,
    framework_root: &Path,
    framework_protected: &[String],
    artifacts_root: &Path,
    artifact_protected: &[String],
    path: &str,
) -> bool {
    // Resolve the target ONCE against target_base; each set then compares against its own anchor.
    let target = resolve_against_root(target_base, path);
    is_protected_with_target(framework_root, framework_protected, &target)
        || is_protected_with_target(artifacts_root, artifact_protected, &target)
}

fn scan_check_path(
    rules: &Rules,
    root: &Path,
    artifacts_root: &Path,
    target_base: &Path,
    path: Option<&str>,
) -> i32 {
    match path {
        Some(p)
            if is_protected_any(
                target_base,
                root,
                &rules.protected,
                artifacts_root,
                &rules.protected_artifacts,
                p,
            ) =>
        {
            1
        }
        Some(_) => 0,
        None => {
            eprintln!("gatekeeper scan --check-path <path>  (path required)");
            2
        }
    }
}

fn git_raw(root: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(|e| format!("git {args:?} failed to start: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "git {args:?} exited {}",
            out.status.code().unwrap_or(-1)
        ));
    }
    Ok(out.stdout)
}

/// Split NUL-delimited git output into non-empty path strings.
fn git_paths_z(root: &Path, args: &[&str]) -> Result<Vec<String>, String> {
    Ok(git_raw(root, args)?
        .split(|&b| b == 0)
        .filter(|s| !s.is_empty())
        .map(|s| String::from_utf8_lossy(s).into_owned())
        .collect())
}

/// Parse `--name-status -z`: a status token, then 1 path (2 for renames/copies R*/C*).
fn git_name_status_z(root: &Path, args: &[&str]) -> Result<Vec<(String, Vec<String>)>, String> {
    let out = git_raw(root, args)?;
    let toks: Vec<&[u8]> = out.split(|&b| b == 0).filter(|s| !s.is_empty()).collect();
    let mut entries = Vec::new();
    let mut i = 0;
    while i < toks.len() {
        let status = String::from_utf8_lossy(toks[i]).into_owned();
        i += 1;
        let n = if status.starts_with('R') || status.starts_with('C') {
            2
        } else {
            1
        };
        let mut paths = Vec::new();
        for _ in 0..n {
            if i < toks.len() {
                paths.push(String::from_utf8_lossy(toks[i]).into_owned());
                i += 1;
            }
        }
        entries.push((status, paths));
    }
    Ok(entries)
}

fn git_blob_oid(root: &Path, path: &str) -> Result<String, String> {
    Ok(
        String::from_utf8_lossy(&git_raw(root, &["rev-parse", &format!(":{path}")])?)
            .trim()
            .to_string(),
    )
}

/// Cheap header read — the staged blob's byte size WITHOUT streaming its content into us.
fn git_blob_size(root: &Path, path: &str) -> Result<usize, String> {
    String::from_utf8_lossy(&git_raw(root, &["cat-file", "-s", &format!(":{path}")])?)
        .trim()
        .parse::<usize>()
        .map_err(|e| format!("git cat-file -s :{path}: unparsable size: {e}"))
}

/// True iff (path, git object id) is pinned in [[allow_blob]]. The OID is content-free, so this
/// works for an oversize blob we have deliberately NOT read.
fn is_blob_allowlisted(root: &Path, path: &str, allow_blobs: &[AllowBlob]) -> bool {
    match git_blob_oid(root, path) {
        Ok(oid) => allow_blobs
            .iter()
            .any(|a| normalize_path(&a.path) == normalize_path(path) && a.blob_oid == oid),
        Err(_) => false,
    }
}

/// Index mode for a staged path (e.g. "100644", "120000" symlink, "160000" gitlink). Reads the
/// INDEX, so it works even when a submodule's commit object is absent from this repo.
/// (Interim: the queued Q2 `--raw` redesign folds this into the single enumeration.)
fn git_index_mode(root: &Path, path: &str) -> Option<String> {
    let out = git_raw(root, &["ls-files", "-s", "-z", "--", path]).ok()?;
    // "<mode> <oid> <stage>\t<path>\0"
    String::from_utf8_lossy(&out)
        .split_whitespace()
        .next()
        .map(str::to_string)
}

/// Both protected sets from the COMMITTED rules.toml (HEAD), so the commit-time integrity guard
/// cannot be disarmed by a staged edit to rules.toml in the same commit.  Returns
/// `(framework_protected, artifact_protected)`; each is empty if HEAD lacks the file or it does
/// not parse (the working-tree set still applies; this only ever ADDS protection).
///
/// Note: older HEAD commits carry `docs/memory`/`.claude/topology/memory` in `protected_paths`,
/// not in `protected_artifact_paths`.  Those entries still resolve against the framework root via
/// the framework-anchored union, which is correct for the framework-repo case and benign (if
/// slightly over-wide) for governed projects during the transition period.
fn head_protected_paths(root: &Path) -> (Vec<String>, Vec<String>) {
    match git_raw(root, &["show", "HEAD:security/rules.toml"]) {
        Ok(bytes) => std::str::from_utf8(&bytes)
            .ok()
            .and_then(|s| parse_rules(s).ok())
            .map(|r| (r.protected, r.protected_artifacts))
            .unwrap_or_default(),
        Err(_) => (Vec::new(), Vec::new()),
    }
}

/// `fw_root` anchors framework-relative protected entries (and is where rules.toml lives);
/// `git_root` is the repo whose staged index is scanned — the PROJECT repo for governed
/// projects, identical to `fw_root` when the framework repo governs itself. All git
/// operations and repo-relative path resolution use `git_root`; without this split, a
/// governed project's pre-commit would silently scan the (clean) framework clone instead
/// of the commit actually being made.
fn scan_staged(
    rules: &Rules,
    fw_root: &Path,
    git_root: &Path,
    artifacts_root: &Path,
    cap: usize,
) -> i32 {
    let mut blocked = false;

    // (1) Scan enumeration: ACMRT — content of each added/copied/modified/renamed/type-changed
    // staged blob. T matters: a symlink→regular-file (or gitlink→file) type change introduces a
    // new content blob that would otherwise escape the "every staged blob is scanned" guarantee.
    match git_paths_z(
        git_root,
        &[
            "diff",
            "--cached",
            "--name-only",
            "-z",
            "--diff-filter=ACMRT",
        ],
    ) {
        Ok(paths) => {
            for path in paths {
                // Submodule gitlinks (mode 160000) are commit pointers, not content — skip (not
                // recursed); the pointed-to commit may not even be in this repo's object store.
                if git_index_mode(git_root, &path).as_deref() == Some("160000") {
                    continue;
                }
                // Size FIRST (a cheap header read), so an oversize blob never streams into memory.
                let size = match git_blob_size(git_root, &path) {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("BLOCK staged-size: {e}");
                        blocked = true;
                        continue;
                    }
                };
                if size > cap {
                    // Oversize: never read content; the OID allowlist check is content-free too.
                    if !is_blob_allowlisted(git_root, &path, &rules.allow_blobs) {
                        eprintln!("BLOCK unscannable-blob: {path} (over {cap}-byte cap); allowlist via [[allow_blob]] path + blob_oid");
                        blocked = true;
                    }
                    continue;
                }
                // Size is within the cap, so reading the content is now bounded.
                match git_raw(git_root, &["show", &format!(":{path}")]) {
                    Ok(blob) => {
                        // Whole-blob NUL sniff (not just a prefix window): a binary whose first NUL
                        // lands late must still be treated as unscannable and block by default.
                        if blob.contains(&0) {
                            // Binary/undecodable: block unless allowlisted by path + OID.
                            if !is_blob_allowlisted(git_root, &path, &rules.allow_blobs) {
                                eprintln!("BLOCK unscannable-blob: {path} (binary/undecodable); allowlist via [[allow_blob]] path + blob_oid");
                                blocked = true;
                            }
                            continue;
                        }
                        let mut f = scan_with(
                            &rules.content_set,
                            &rules.content,
                            &blob,
                            &rules.allows,
                            Some(&path),
                        );
                        // Entropy is suppressed for excluded paths (path-bearing lane); the regex
                        // content scan above always runs — excludes never weaken labeled detection.
                        if !rules.exclude_paths.iter().any(|g| glob_match(&path, g)) {
                            f.extend(scan_entropy(
                                &rules.entropy,
                                &blob,
                                &rules.allows,
                                Some(&path),
                            ));
                        }
                        if report(&f) == 1 {
                            blocked = true;
                        }
                    }
                    Err(e) => {
                        eprintln!("BLOCK staged-read: cannot read staged blob {path}: {e}");
                        blocked = true;
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("gatekeeper scan --staged: {e}");
            return 2;
        }
    }

    // (2) Integrity enumeration: ACDMRT — broader; both rename sides vs protected_paths. Honor the
    // working-tree AND the committed (HEAD) protected sets, so a commit cannot remove a path from
    // either set to slip its own weakening of rules.toml past this guard.
    //
    // The two sets (framework-anchored and artifacts-anchored) are unioned independently: entries
    // from HEAD's `protected_paths` continue to resolve against the framework root, and entries from
    // HEAD's `protected_artifact_paths` resolve against the artifacts root.  This preserves correct
    // resolution semantics across the transition from the old single-set layout.
    let (head_fw, head_art) = head_protected_paths(git_root);
    let mut protected_fw_union: Vec<String> = rules.protected.clone();
    for p in head_fw {
        if !protected_fw_union.contains(&p) {
            protected_fw_union.push(p);
        }
    }
    let mut protected_art_union: Vec<String> = rules.protected_artifacts.clone();
    for p in head_art {
        if !protected_art_union.contains(&p) {
            protected_art_union.push(p);
        }
    }
    match git_name_status_z(
        git_root,
        &[
            "diff",
            "--cached",
            "--name-status",
            "-z",
            "-M",
            "--diff-filter=ACDMRT",
        ],
    ) {
        Ok(entries) => {
            for (status, paths) in entries {
                for p in &paths {
                    // In the staged lane `git diff --cached` emits paths relative to the repo
                    // being committed, so target_base = git_root.  Framework entries resolve
                    // against fw_root and artifact entries against artifacts_root — each base is
                    // DIFFERENT in a governed project; this split is exactly what prevents the
                    // double-prefix bug: without it `docs/memory/x.handoff.md` would resolve to
                    // `<root>/docs/docs/memory/x.handoff.md` and miss the "memory" entry.
                    if is_protected_any(
                        git_root,
                        fw_root,
                        &protected_fw_union,
                        artifacts_root,
                        &protected_art_union,
                        p,
                    ) {
                        eprintln!("BLOCK protected-path: staged change ({status}) to {p}");
                        blocked = true;
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("gatekeeper scan --staged: {e}");
            return 2;
        }
    }

    if blocked {
        1
    } else {
        0
    }
}

#[derive(Deserialize)]
struct HookEvent {
    #[serde(default)]
    tool_name: String,
    #[serde(default)]
    tool_input: ToolInput,
}

#[derive(Default, Deserialize)]
struct ToolInput {
    #[serde(default)]
    command: Option<String>,
    #[serde(default)]
    file_path: Option<String>,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    old_string: Option<String>,
    #[serde(default)]
    new_string: Option<String>,
    #[serde(default)]
    replace_all: Option<bool>,
    #[serde(default)]
    edits: Option<Vec<EditOp>>,
}

#[derive(Deserialize)]
struct EditOp {
    #[serde(default)]
    old_string: String,
    #[serde(default)]
    new_string: String,
    #[serde(default)]
    replace_all: Option<bool>,
}

fn decision_json(decision: &str, reason: &str) -> String {
    serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": decision,
            "permissionDecisionReason": reason,
        }
    })
    .to_string()
}

/// Emit a deny decision (exit 0) on the first block; silent allow (exit 0) otherwise. Warns are
/// dropped on the hook path to keep stdout the sole channel.
fn emit_decision(findings: &[Finding]) -> i32 {
    if let Some(b) = findings.iter().find(|f| f.severity == Severity::Block) {
        let reason = format!(
            "Topology security veto: {} [{}] (redacted: {})",
            b.rule_id, b.location, b.redacted
        );
        println!("{}", decision_json("deny", &reason));
    }
    0
}

fn emit_ask(path: &str) -> i32 {
    emit_ask_reason(&format!(
        "Topology: '{path}' is a protected safety file — human approval required to modify it."
    ))
}

/// Emit an `ask` decision with an explicit reason (exit 0). Used both for protected paths and for
/// the fail-closed case where we cannot verify an edit (target unreadable / over the hook cap).
fn emit_ask_reason(reason: &str) -> i32 {
    println!("{}", decision_json("ask", reason));
    0
}

/// Apply one edit, but REFUSE to build a post-edit image larger than `cap`: a `replace_all` whose
/// replacement expands the file (e.g. one byte -> many, times many occurrences) could otherwise
/// blow up memory before any decision. Returns None on overflow so the caller fails closed (asks).
fn apply_edit_capped(
    text: &str,
    old: &str,
    new: &str,
    replace_all: bool,
    cap: usize,
) -> Option<String> {
    if old.is_empty() {
        // Empty old_string: the insertion point is ambiguous, so we cannot faithfully reconstruct
        // the post-edit image (a secret could complete by abutting existing text at the real
        // insertion point — appending with a separator would miss it). Fail closed: None makes the
        // caller `ask`, rather than fabricate an image we can't verify.
        return None;
    }
    let count = if replace_all {
        text.matches(old).count()
    } else {
        usize::from(text.contains(old))
    };
    if count == 0 {
        return Some(text.to_string());
    }
    // Project the size before allocating the result (no huge intermediate string).
    let delta = new.len() as i128 - old.len() as i128;
    let projected = text.len() as i128 + delta * count as i128;
    if projected > cap as i128 {
        return None;
    }
    Some(if replace_all {
        text.replace(old, new)
    } else {
        text.replacen(old, new, 1)
    })
}

/// Read at most cap+1 bytes of a file. None if it is unreadable OR over the cap — the caller then
/// fails closed (asks); the full-file secret is also caught at pre-commit.
fn read_file_capped(path: &str, cap: usize) -> Option<String> {
    let mut buf = Vec::new();
    fs::File::open(path)
        .ok()?
        .take(cap as u64 + 1)
        .read_to_end(&mut buf)
        .ok()?;
    if buf.len() > cap {
        return None;
    }
    Some(String::from_utf8_lossy(&buf).into_owned())
}

/// Reconstruct the full post-edit file (bounded read). Returns `None` when the target is unreadable,
/// over the hook cap, OR the post-edit image would exceed the cap (an expanding `replace_all`) — the
/// caller then FAILS CLOSED (asks) rather than scanning only the added text or allocating unboundedly,
/// because a secret could be completed across the unchanged text we cannot see.
fn reconstruct(file_path: &str, ti: &ToolInput, cap: usize) -> Option<String> {
    let mut text = read_file_capped(file_path, cap)?;
    if let Some(edits) = &ti.edits {
        for e in edits {
            text = apply_edit_capped(
                &text,
                &e.old_string,
                &e.new_string,
                e.replace_all.unwrap_or(false),
                cap,
            )?;
        }
    } else if let (Some(old), Some(new)) = (&ti.old_string, &ti.new_string) {
        text = apply_edit_capped(&text, old, new, ti.replace_all.unwrap_or(false), cap)?;
    }
    Some(text)
}

/// Returns true if `file_path` matches any framework-anchored OR artifacts-anchored protected entry.
/// resolve_against_root handles both absolute hook paths and `..` aliases that climb out of and
/// back into the repo, so a parent-and-return spelling cannot dodge the protected-file gate.
///
/// `target_base` is the process cwd at the time the hook fires — hook events from Claude Code
/// carry absolute paths, so the base only matters when the path is relative (in which case the
/// honest resolution is against the cwd, not hard-wired to the framework root).
fn hook_path_protected(
    framework_protected: &[String],
    artifact_protected: &[String],
    file_path: &str,
    framework_root: &Path,
    artifacts_root: &Path,
    target_base: &Path,
) -> bool {
    is_protected_any(
        target_base,
        framework_root,
        framework_protected,
        artifacts_root,
        artifact_protected,
        file_path,
    )
}

fn scan_hook(rules: &Rules, root: &Path, artifacts_root: &Path, target_base: &Path) -> i32 {
    let data = match read_stdin_bytes(HOOK_INPUT_CAP) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("gatekeeper scan --hook: {e}");
            return 2; // wrapper fails closed
        }
    };
    let event: HookEvent = match serde_json::from_slice(&data) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("gatekeeper scan --hook: malformed event JSON: {e}");
            return 2; // wrapper fails closed (covers deep nesting -> serde_json recursion limit)
        }
    };
    // A missing/empty tool_name is a malformed event (a real PreToolUse always names its tool): we
    // cannot tell whether it is one we gate, so fail closed rather than fall through to silent allow.
    if event.tool_name.is_empty() {
        eprintln!("gatekeeper scan --hook: event missing 'tool_name'");
        return 2;
    }
    // A RECOGNIZED tool whose operative field is missing is a malformed event on a tool we DO
    // gate: fail closed (exit 2 -> the wrapper denies), never silently allow.
    match event.tool_name.as_str() {
        "Bash" => {
            let Some(cmd) = event.tool_input.command else {
                eprintln!("gatekeeper scan --hook: Bash event missing 'command'");
                return 2;
            };
            // COMMAND string: join line-continuations for BOTH content and command rules (the shell
            // joins them before executing, so a secret split across `\<newline>` is one key).
            let joined = strip_line_continuations(cmd.as_bytes());
            let mut f = scan_with(
                &rules.content_set,
                &rules.content,
                &joined,
                &rules.allows,
                None,
            );
            f.extend(scan_with(
                &rules.command_set,
                &rules.command,
                &joined,
                &rules.allows,
                None,
            ));
            // Bash is a command string with no path — entropy always runs (no exclude applies).
            f.extend(scan_entropy(&rules.entropy, &joined, &rules.allows, None));
            f.extend(detect_path_mutation(&joined, &rules.path_mutation));
            emit_decision(&f)
        }
        "Write" => {
            let Some(fp) = event.tool_input.file_path.clone() else {
                eprintln!("gatekeeper scan --hook: Write event missing 'file_path'");
                return 2;
            };
            if hook_path_protected(
                &rules.protected,
                &rules.protected_artifacts,
                &fp,
                root,
                artifacts_root,
                target_base,
            ) {
                return emit_ask(&fp);
            }
            let Some(content) = event.tool_input.content else {
                eprintln!("gatekeeper scan --hook: Write event missing 'content'");
                return 2;
            };
            let mut f = scan_with(
                &rules.content_set,
                &rules.content,
                content.as_bytes(),
                &rules.allows,
                None,
            );
            // Path-bearing lane: entropy is suppressed when the target matches an exclude glob.
            if !rules.exclude_paths.iter().any(|g| glob_match(&fp, g)) {
                f.extend(scan_entropy(
                    &rules.entropy,
                    content.as_bytes(),
                    &rules.allows,
                    None,
                ));
            }
            emit_decision(&f)
        }
        "Edit" | "MultiEdit" => {
            let Some(fp) = event.tool_input.file_path.clone() else {
                eprintln!("gatekeeper scan --hook: Edit event missing 'file_path'");
                return 2;
            };
            if hook_path_protected(
                &rules.protected,
                &rules.protected_artifacts,
                &fp,
                root,
                artifacts_root,
                target_base,
            ) {
                return emit_ask(&fp);
            }
            // A recognized Edit/MultiEdit with no MEANINGFUL edit operation is malformed:
            // reconstruct would return the file unchanged and we'd "verify" content we never
            // edited. Require every op to carry a non-empty old or new string (an `edits:[{}]`
            // entry is a no-op) — otherwise fail closed.
            let ti = &event.tool_input;
            let payload_ok = match &ti.edits {
                Some(edits) => {
                    !edits.is_empty()
                        && edits
                            .iter()
                            .all(|e| !e.old_string.is_empty() || !e.new_string.is_empty())
                }
                None => match (&ti.old_string, &ti.new_string) {
                    (Some(o), Some(n)) => !o.is_empty() || !n.is_empty(),
                    _ => false,
                },
            };
            if !payload_ok {
                eprintln!("gatekeeper scan --hook: Edit event missing/empty edit payload");
                return 2;
            }
            let Some(text) = reconstruct(&fp, &event.tool_input, HOOK_INPUT_CAP) else {
                // Cannot read or oversize: we can't prove the post-edit file is secret-free, so
                // fail closed (ask) rather than scan only the added text.
                return emit_ask_reason(&format!(
                    "Topology: cannot read or oversize edit target '{fp}' — unable to verify it is \
                     secret-free; human approval required."
                ));
            };
            let mut f = scan_with(
                &rules.content_set,
                &rules.content,
                text.as_bytes(),
                &rules.allows,
                None,
            );
            // Path-bearing lane: entropy is suppressed when the target matches an exclude glob.
            if !rules.exclude_paths.iter().any(|g| glob_match(&fp, g)) {
                f.extend(scan_entropy(
                    &rules.entropy,
                    text.as_bytes(),
                    &rules.allows,
                    None,
                ));
            }
            emit_decision(&f)
        }
        _ => 0, // out of scope (MCP / other tools we do not gate): silent allow
    }
}

#[cfg(test)]
mod staged_unit {
    use super::*;

    // Over-cap is only testable with a small cap, so this calls scan_staged directly (the CLI
    // always passes the STAGED_BLOB_CAP const). Covers over-cap-block AND allow_blob-pass.
    #[test]
    fn over_cap_blocks_then_allowlisted_passes() {
        let root = std::env::temp_dir().join(format!("topo_staged_unit_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        git_raw(&root, &["init", "-q", "-b", "main"]).unwrap();
        git_raw(&root, &["config", "user.email", "t@t.t"]).unwrap();
        git_raw(&root, &["config", "user.name", "t"]).unwrap();
        std::fs::write(root.join("big.txt"), "0123456789ABCDEFGHIJ").unwrap(); // 20 bytes
        git_raw(&root, &["add", "big.txt"]).unwrap();
        // artifacts_root is irrelevant for this test (no protected_artifact_paths), so reuse root.
        let artifacts_root = root.clone();
        let rules = parse_rules("schema_version = 1").unwrap();
        assert_eq!(
            scan_staged(&rules, &root, &root, &artifacts_root, 8),
            1,
            "20-byte blob over an 8-byte cap blocks"
        );
        // Allowlist it by its git object id -> passes (the OID is read content-free).
        let oid = git_blob_oid(&root, "big.txt").unwrap();
        let toml = format!(
            r#"schema_version = 1
[[allow_blob]]
path = "big.txt"
blob_oid = "{oid}"
"#
        );
        assert_eq!(
            scan_staged(
                &parse_rules(&toml).unwrap(),
                &root,
                &root,
                &artifacts_root,
                8
            ),
            0,
            "allowlisted by blob_oid passes"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    // ── Staged-lane target-base regression tests ─────────────────────────────
    //
    // These reproduce the double-prefix bug found by the fresh-context critic (HEAD 1f9265b).
    //
    // Setup: a scratch git repo with `protected_artifact_paths = ["memory"]` and
    // `artifacts_root = <repo>/docs`.  Git emits paths relative to the REPO ROOT (e.g.
    // `docs/memory/x.handoff.md`).  The old code resolved that target against `artifacts_root`
    // (`<repo>/docs`) yielding `<repo>/docs/docs/memory/x.handoff.md` — no match.  The fix
    // resolves the target against `root` (the repo root = framework root) and only the entries
    // against their respective anchor roots.

    /// Build a scratch git repo whose rules.toml has `protected_artifact_paths = ["memory"]`.
    /// `artifacts_root` = `<root>/docs` (framework-repo layout).
    fn staged_artifact_root(tag: &str) -> std::path::PathBuf {
        let root =
            std::env::temp_dir().join(format!("topo_staged_art_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("security")).unwrap();
        // Minimal rules with only the artifact-protected set; no content rules to avoid spurious
        // blocks from the content-scan pass.
        let rules_toml = "schema_version = 1\n\
            [integrity]\nprotected_artifact_paths = [\"memory\"]\n";
        std::fs::write(root.join("security").join("rules.toml"), rules_toml).unwrap();
        std::fs::create_dir_all(root.join("skills")).unwrap(); // required marker
        std::fs::write(root.join("AGENTS.md"), "").unwrap(); // required marker
        git_raw(&root, &["init", "-q", "-b", "main"]).unwrap();
        git_raw(&root, &["config", "user.email", "t@t.t"]).unwrap();
        git_raw(&root, &["config", "user.name", "t"]).unwrap();
        // Initial commit so that HEAD exists (the integrity pass reads HEAD:security/rules.toml).
        git_raw(&root, &["add", "."]).unwrap();
        git_raw(&root, &["commit", "-q", "-m", "init"]).unwrap();
        root
    }

    #[test]
    fn staged_handoff_in_docs_memory_is_blocked() {
        // Regression: staging docs/memory/x.handoff.md must be BLOCKED (was allowed by the
        // double-prefix bug: target resolved against artifacts_root → <root>/docs/docs/memory/…).
        let root = staged_artifact_root("handoff_block");
        let artifacts_root = root.join("docs");
        std::fs::create_dir_all(root.join("docs").join("memory")).unwrap();
        std::fs::write(
            root.join("docs").join("memory").join("x.handoff.md"),
            "body",
        )
        .unwrap();
        git_raw(&root, &["add", "docs/memory/x.handoff.md"]).unwrap();
        let rules = parse_rules(
            &std::fs::read_to_string(root.join("security").join("rules.toml")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            scan_staged(&rules, &root, &root, &artifacts_root, STAGED_BLOB_CAP),
            1,
            "staging docs/memory/x.handoff.md must be blocked (protected_artifact_paths)"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn staged_memory_readme_in_framework_root_not_blocked() {
        // Spurious-block regression: staging memory/README.md (at the FRAMEWORK root, not under
        // docs/memory/) must NOT be blocked.  The old code resolved `memory/README.md` against
        // `artifacts_root` (`<root>/docs`) → `<root>/docs/memory/README.md` which starts_with
        // `<root>/docs/memory` — a false positive.  The fix resolves against `root` (the repo
        // root) → `<root>/memory/README.md`, which does NOT start_with `<root>/docs/memory`.
        let root = staged_artifact_root("mem_readme");
        let artifacts_root = root.join("docs");
        std::fs::create_dir_all(root.join("memory")).unwrap();
        std::fs::write(root.join("memory").join("README.md"), "template\n").unwrap();
        git_raw(&root, &["add", "memory/README.md"]).unwrap();
        let rules = parse_rules(
            &std::fs::read_to_string(root.join("security").join("rules.toml")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            scan_staged(&rules, &root, &root, &artifacts_root, STAGED_BLOB_CAP),
            0,
            "staging memory/README.md at the framework root must NOT be blocked"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    // ── Governed-project staged lane ─────────────────────────────────────────
    //
    // Live-test S5.1/S5.2 regression: in a governed project the pre-commit scan must target
    // the PROJECT repo's index (git_root), not the framework clone's. Before the fw/git root
    // split, `scan --staged` ran its git ops against the framework root — the (clean)
    // vendored clone — so a staged AWS key in the project committed without a single BLOCK.

    /// Scratch governed pair: a framework dir (rules only, not a repo) and a separate
    /// project git repo. Returns (fw_root, project_root, rules).
    fn governed_pair(tag: &str) -> (std::path::PathBuf, std::path::PathBuf, Rules) {
        let base =
            std::env::temp_dir().join(format!("topo_staged_gov_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let fw = base.join("fw");
        std::fs::create_dir_all(fw.join("security")).unwrap();
        let rules_toml = "schema_version = 1\n\
            [[rule]]\nid = \"aws-access-key-id\"\nkind = \"content\"\nseverity = \"block\"\n\
            description = \"AWS access key id\"\npattern = '\\b(AKIA|ASIA)[0-9A-Z]{16}\\b'\n\
            [integrity]\nprotected_artifact_paths = [\"memory\"]\n";
        std::fs::write(fw.join("security").join("rules.toml"), rules_toml).unwrap();
        let project = base.join("project");
        std::fs::create_dir_all(&project).unwrap();
        git_raw(&project, &["init", "-q", "-b", "main"]).unwrap();
        git_raw(&project, &["config", "user.email", "t@t.t"]).unwrap();
        git_raw(&project, &["config", "user.name", "t"]).unwrap();
        std::fs::write(project.join("README.md"), "notes app\n").unwrap();
        git_raw(&project, &["add", "."]).unwrap();
        git_raw(&project, &["commit", "-q", "-m", "init"]).unwrap();
        let rules = parse_rules(rules_toml).unwrap();
        (fw, project, rules)
    }

    #[test]
    fn staged_governed_secret_in_project_repo_is_blocked() {
        let (fw, project, rules) = governed_pair("secret");
        let artifacts_root = project.join(".claude").join("topology");
        let key = format!("AKIA{}", "XYZ123ABCDEF4567"); // built by concat; 20 chars total
        std::fs::write(
            project.join("config.js"),
            format!("const KEY = \"{key}\";\n"),
        )
        .unwrap();
        git_raw(&project, &["add", "config.js"]).unwrap();
        assert_eq!(
            scan_staged(&rules, &fw, &project, &artifacts_root, STAGED_BLOB_CAP),
            1,
            "a staged AWS key in the governed project repo must be blocked"
        );
        let _ = std::fs::remove_dir_all(project.parent().unwrap());
    }

    #[test]
    fn staged_governed_handoff_in_project_is_blocked_and_clean_file_passes() {
        let (fw, project, rules) = governed_pair("handoff");
        let artifacts_root = project.join(".claude").join("topology");
        // A clean source file alone passes.
        std::fs::write(project.join("app.js"), "console.log(1);\n").unwrap();
        git_raw(&project, &["add", "app.js"]).unwrap();
        assert_eq!(
            scan_staged(&rules, &fw, &project, &artifacts_root, STAGED_BLOB_CAP),
            0,
            "a clean staged source file in the governed project must pass"
        );
        // Staging the governed handoff trips the artifacts-anchored protected set.
        let mem = artifacts_root.join("memory");
        std::fs::create_dir_all(&mem).unwrap();
        std::fs::write(mem.join("x.handoff.md"), "body\n").unwrap();
        git_raw(&project, &["add", ".claude/topology/memory/x.handoff.md"]).unwrap();
        assert_eq!(
            scan_staged(&rules, &fw, &project, &artifacts_root, STAGED_BLOB_CAP),
            1,
            "a staged governed handoff must be blocked (artifacts-anchored protection)"
        );
        let _ = std::fs::remove_dir_all(project.parent().unwrap());
    }
}

#[cfg(test)]
mod perf_report {
    // EVIDENCE, not gates: wall-clock varies by machine, so these are #[ignore]'d and run
    // explicitly (`cargo test scan::perf_report -- --ignored --nocapture`); their numbers are
    // recorded in docs/verify/ against the 150/250 ms targets. The default-run gates are the
    // generous-ceiling smoke tests in match_tests.
    use super::*;
    use std::time::Instant;

    #[test]
    #[ignore]
    fn scan_latency_percentiles() {
        let r = parse_rules(include_str!("../../security/rules.toml")).unwrap();
        let input = "export URL=postgres://u:p@h/db\nlet x = 1;\n# comment\n".repeat(64); // ~few KB
        let mut us: Vec<u128> = (0..500)
            .map(|_| {
                let t = Instant::now();
                let _ = scan_with(
                    &r.content_set,
                    &r.content,
                    input.as_bytes(),
                    &r.allows,
                    None,
                );
                t.elapsed().as_micros()
            })
            .collect();
        us.sort_unstable();
        let q = |p: f64| us[((us.len() as f64 - 1.0) * p) as usize];
        println!(
            "scan latency us: p50={} p95={} p99={}",
            q(0.50),
            q(0.95),
            q(0.99)
        );
    }

    #[test]
    #[ignore]
    fn staged_scales_linearly() {
        for n in [1usize, 10, 100] {
            let root = std::env::temp_dir().join(format!("topo_perf_{n}_{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(&root).unwrap();
            git_raw(&root, &["init", "-q", "-b", "main"]).unwrap();
            git_raw(&root, &["config", "user.email", "t@t.t"]).unwrap();
            git_raw(&root, &["config", "user.name", "t"]).unwrap();
            for i in 0..n {
                std::fs::write(root.join(format!("f{i}.txt")), "benign content line\n").unwrap();
            }
            git_raw(&root, &["add", "."]).unwrap();
            let r = parse_rules("schema_version = 1").unwrap();
            let t = Instant::now();
            // artifacts_root is irrelevant for this test (no protected_artifact_paths in the
            // minimal rules); reuse root as a valid dummy path.
            let _ = scan_staged(&r, &root, &root, &root, STAGED_BLOB_CAP);
            println!("staged N={n}: {} ms", t.elapsed().as_millis());
            let _ = std::fs::remove_dir_all(&root);
        }
        // Eyeball linearity; the architecture guarantees it (independent per-blob, no shared state).
    }
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
    fn allow_pattern_requires_full_span() {
        // A pattern allow must exempt only a span it matches in FULL, not one it merely appears in.
        let allows = vec![CompiledAllow {
            rule: "*".to_string(),
            matcher: AllowMatch::Pattern(Regex::new("ABC").unwrap()),
        }];
        assert!(
            !is_allowed(&allows, "r", b"ABCDEFGHIJ"),
            "a substring pattern must not exempt a larger span"
        );
        assert!(
            is_allowed(&allows, "r", b"ABC"),
            "a full-span pattern match is exempted"
        );
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

// ── AWS secret access key rule tests ──────────────────────────────────────────
#[cfg(test)]
mod aws_secret_key_tests {
    use super::*;

    /// Rules including the aws-secret-access-key content rule + its allowlist entry.
    fn rules() -> Rules {
        let toml = concat!(
            "schema_version = 1\n",
            "\n",
            "[[rule]]\n",
            "id = \"aws-secret-access-key\"\n",
            "kind = \"content\"\n",
            "severity = \"block\"\n",
            "description = \"AWS secret access key assignment\"\n",
            // single-quoted TOML literal string passes backslashes through to the regex engine.
            "pattern = '(?i)\\b(?:aws[_-])?secret[_-](?:access[_-])?key\\s*[=:]\\s*[\\x22\\x27]?[A-Za-z0-9/+=]{40,}'\n",
            "\n",
            "[[allow]]\n",
            "rule = \"aws-secret-access-key\"\n",
            "pattern = '(?i)(?:aws[_-])?secret[_-](?:access[_-])?key\\s*[=:]\\s*[\\x22\\x27]?wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY[\\x22\\x27]?'\n",
        );
        parse_rules(toml).unwrap()
    }

    // Helper: run the content scanner and return findings.
    fn scan(r: &Rules, input: &[u8]) -> Vec<Finding> {
        scan_with(&r.content_set, &r.content, input, &r.allows, None)
    }

    // ── Positive cases (must block) ────────────────────────────────────────────

    #[test]
    fn blocks_aws_secret_access_key_uppercase() {
        // The canonical env-var spelling.  Value is 40 base64-ish chars (realistic shape).
        let r = rules();
        // Build key by concat to avoid the scanner flagging this source file itself.
        // "9v" (2) + "Kp2mXqL8wNzR4tYbE7uJ3hAgF6dScVnM1oPxIa" (38) = 40 chars total.
        let val = format!("{}Kp2mXqL8wNzR4tYbE7uJ3hAgF6dScVnM1oPxIa", "9v");
        assert_eq!(val.len(), 40, "test value must be exactly 40 chars");
        let input = format!("AWS_SECRET_ACCESS_KEY={val}\n");
        let f = scan(&r, input.as_bytes());
        assert_eq!(
            f.len(),
            1,
            "AWS_SECRET_ACCESS_KEY with 40-char value must block"
        );
        assert_eq!(report(&f), 1);
        // Redacted hint must not contain the raw secret.
        assert!(
            !f[0].redacted.contains(&val),
            "redacted hint must not expose the value"
        );
    }

    #[test]
    fn blocks_aws_secret_access_key_lowercase() {
        // Lowercase / .env file spelling.
        let r = rules();
        let val = format!("{}vKp2mXqL8wNzR4tYbE7uJ3hAgF6dScVnM1oPxIa", "9");
        let input = format!("aws_secret_access_key={val}\n");
        let f = scan(&r, input.as_bytes());
        assert_eq!(f.len(), 1, "aws_secret_access_key (lowercase) must block");
    }

    #[test]
    fn blocks_secret_key_colon_separator() {
        // YAML-style colon separator.
        let r = rules();
        let val = format!("{}vKp2mXqL8wNzR4tYbE7uJ3hAgF6dScVnM1oPxIa", "9");
        let input = format!("aws_secret_access_key: {val}\n");
        let f = scan(&r, input.as_bytes());
        assert_eq!(f.len(), 1, "colon separator must block");
    }

    #[test]
    fn blocks_secret_key_quoted_value() {
        // Quoted value (shell export / .env style).
        let r = rules();
        let val = format!("{}vKp2mXqL8wNzR4tYbE7uJ3hAgF6dScVnM1oPxIa", "9");
        let input = format!("AWS_SECRET_ACCESS_KEY=\"{val}\"\n");
        let f = scan(&r, input.as_bytes());
        assert_eq!(f.len(), 1, "double-quoted value must block");
    }

    #[test]
    fn blocks_aws_secret_key_short_form() {
        // No "access" component: aws_secret_key.
        let r = rules();
        let val = format!("{}vKp2mXqL8wNzR4tYbE7uJ3hAgF6dScVnM1oPxIa", "9");
        let input = format!("aws_secret_key={val}\n");
        let f = scan(&r, input.as_bytes());
        assert_eq!(f.len(), 1, "aws_secret_key (no 'access') must block");
    }

    #[test]
    fn blocks_secret_access_key_no_aws_prefix() {
        // No "aws" prefix: secret-access-key (hyphen separated, YAML config style).
        let r = rules();
        let val = format!("{}vKp2mXqL8wNzR4tYbE7uJ3hAgF6dScVnM1oPxIa", "9");
        let input = format!("secret-access-key: {val}\n");
        let f = scan(&r, input.as_bytes());
        assert_eq!(f.len(), 1, "secret-access-key (no aws prefix) must block");
    }

    // ── Negative cases (must NOT block) ────────────────────────────────────────

    #[test]
    fn allows_aws_docs_example_secret() {
        // The canonical AWS documentation example key is allowlisted.
        let r = rules();
        let input = b"AWS_SECRET_ACCESS_KEY=wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY\n";
        let f = scan(&r, input);
        assert!(
            f.is_empty(),
            "AWS docs example secret must be allowlisted; got: {:#?}",
            f.iter().map(|x| &x.redacted).collect::<Vec<_>>()
        );
    }

    #[test]
    fn allows_git_sha_without_secret_key_context() {
        // A 40-hex git SHA must NOT match — no "secret" in the key name.
        let r = rules();
        let input = b"commit = 4f80200a4f4f20627a4519c1f4eddf8d6e1c5a99\n";
        let f = scan(&r, input);
        assert!(
            f.is_empty(),
            "git SHA without secret-key context must not block"
        );
    }

    #[test]
    fn allows_deploy_key_without_secret() {
        // "deploy_key" contains "key" but not "secret" — must NOT match.
        let r = rules();
        let val = "4f80200a4f4f20627a4519c1f4eddf8d6e1c5a99"; // 40 hex chars
        let input = format!("deploy_key = {val}\n");
        let f = scan(&r, input.as_bytes());
        assert!(
            f.is_empty(),
            "deploy_key without 'secret' in name must not block"
        );
    }

    #[test]
    fn allows_base64_blob_without_secret_key_context() {
        // A base64-encoded blob in a lockfile line (no key-name context) must NOT block.
        let r = rules();
        // 44-char base64 string (without 'secret' context)
        let input = b"integrity = sha512-ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmno=\n";
        let f = scan(&r, input);
        assert!(
            f.is_empty(),
            "base64 blob without secret-key context must not block"
        );
    }

    #[test]
    fn value_shorter_than_40_chars_not_blocked() {
        // A 39-char value must not trigger (too short to be a valid AWS secret).
        let r = rules();
        let val = "9vKp2mXqL8wNzR4tYbE7uJ3hAgF6dScVnM1oPxI"; // 39 chars
        assert_eq!(val.len(), 39);
        let input = format!("AWS_SECRET_ACCESS_KEY={val}\n");
        let f = scan(&r, input.as_bytes());
        assert!(
            f.is_empty(),
            "39-char value must not block (below 40-char minimum)"
        );
    }

    #[test]
    fn redaction_does_not_expose_raw_value() {
        // Regression guard: the redacted hint must only show a short prefix + length.
        let r = rules();
        let val = format!("{}vKp2mXqL8wNzR4tYbE7uJ3hAgF6dScVnM1oPxIa", "9");
        let input = format!("AWS_SECRET_ACCESS_KEY={val}\n");
        let f = scan(&r, input.as_bytes());
        assert_eq!(f.len(), 1);
        // redact() shows at most 4 leading graphic bytes then "…<len=N>".
        assert!(
            f[0].redacted.contains("…<len="),
            "redacted hint must contain length marker"
        );
        assert!(
            !f[0].redacted.contains(&val),
            "redacted hint must not contain the raw value"
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

// ── Schema v2 acceptance (back-compat) — Task 1 ───────────────────────────────
//
// schema_version = 2 must parse (the new entropy schema), schema_version = 1 must STILL
// parse (back-compat with every committed rules.toml), and an out-of-range version (3) must
// be rejected with a message that names the accepted range ("expected 1 or 2").
//
// These compile against the EXISTING parser; they go red at RUNTIME today because the parser
// rejects anything != SCHEMA_VERSION (== 1). That is the intended Task-1 red.
#[cfg(test)]
mod schema_v2_tests {
    use super::*;

    /// A minimal v-N rules doc carrying one existing `kind="content"` rule. `{ver}` is substituted.
    fn doc(ver: u32) -> String {
        format!(
            "schema_version = {ver}\n\n\
             [[rule]]\n\
             id = \"k\"\n\
             kind = \"content\"\n\
             severity = \"block\"\n\
             description = \"d\"\n\
             pattern = '\\bAKIA[0-9A-Z]{{16}}\\b'\n"
        )
    }

    #[test]
    fn schema_version_2_accepted() {
        let r = parse_rules(&doc(2));
        assert!(
            r.is_ok(),
            "schema_version = 2 must parse (entropy schema); got: {:?}",
            r.err()
        );
    }

    #[test]
    fn schema_version_1_still_accepted() {
        // Back-compat: every committed rules.toml is v1 and must keep parsing.
        assert!(
            parse_rules(&doc(1)).is_ok(),
            "schema_version = 1 must still parse (back-compat)"
        );
    }

    #[test]
    fn schema_version_3_rejected() {
        let err = parse_rules(&doc(3))
            .expect_err("schema_version = 3 is out of the accepted range and must be rejected");
        assert!(
            err.contains("1 or 2"),
            "rejection message must name the accepted range (\"expected 1 or 2\"); got: {err}"
        );
    }
}

// ── Shannon entropy helper — Task 2 ───────────────────────────────────────────
//
// `shannon_entropy(token: &str) -> f64` returns bits per character:
//   H = -Σ p_i · log2 p_i   over the distinct chars of `token`.
// These reference a not-yet-existing free function, so the crate will not compile until it is
// added — that compile error naming `shannon_entropy` is the intended Task-2 red.
#[cfg(test)]
mod shannon_entropy_tests {
    use super::*;

    #[test]
    fn shannon_entropy_uniform_hex_near_4() {
        // 64 chars cycling through all 16 hex digits uniformly (each appears 4×).
        // A uniform distribution over 16 symbols has entropy log2(16) = 4.0 bits/char.
        let token: String = "0123456789abcdef".chars().cycle().take(64).collect();
        assert_eq!(token.len(), 64);
        let h = shannon_entropy(&token);
        assert!(
            (h - 4.0).abs() < 0.2,
            "uniform-hex token entropy must be ≈ 4.0 bits/char, got {h}"
        );
    }

    #[test]
    fn shannon_entropy_repetitive_is_low() {
        // A single repeated char carries no information → entropy near 0.
        let h = shannon_entropy("aaaaaaaaaaaaaaaa");
        assert!(
            h < 0.1,
            "a repeated single character must have near-zero entropy, got {h}"
        );
    }

    #[test]
    fn shannon_entropy_empty_is_zero() {
        // Empty input must be 0.0 — no panic, no NaN (the Σ over zero symbols).
        let h = shannon_entropy("");
        assert!(
            h.is_finite(),
            "empty-token entropy must not be NaN/inf, got {h}"
        );
        assert_eq!(h, 0.0, "empty token must have entropy 0.0");
    }
}

// ── kind = "entropy" parsing — Task 3 ─────────────────────────────────────────
//
// A `kind = "entropy"` rule carries `charset` / `min_length` / `threshold_bits_per_char`
// instead of a regex `pattern`, and compiles into an entropy-rule vector on `Rules` (the plan's
// `CompiledEntropyRule { id, severity, description, charset, min_length, threshold }`).
// References to `Kind::Entropy`, the new `RawRule` fields, and `Rules.entropy` do not exist yet,
// so the crate will not compile — that is the intended Task-3 red.
#[cfg(test)]
mod entropy_rule_parse_tests {
    use super::*;

    /// A v2 doc with a single hex entropy rule (no regex `pattern`).
    const ENTROPY_DOC: &str = "schema_version = 2\n\n\
        [[rule]]\n\
        id = \"hex-he\"\n\
        kind = \"entropy\"\n\
        severity = \"warn\"\n\
        description = \"high-entropy hex run\"\n\
        charset = \"hex\"\n\
        min_length = 32\n\
        threshold_bits_per_char = 3.0\n";

    #[test]
    fn entropy_rule_parses() {
        let r =
            parse_rules(ENTROPY_DOC).expect("a v2 doc with one kind=\"entropy\" rule must parse");
        // The entropy rule is routed into its own compiled vector (NOT the content RegexSet).
        assert_eq!(
            r.entropy.len(),
            1,
            "exactly one compiled entropy rule must be produced"
        );
        assert_eq!(
            r.content.len(),
            0,
            "an entropy rule must not land in the content lane"
        );
        let er = &r.entropy[0];
        assert_eq!(er.id, "hex-he");
        assert_eq!(er.charset, "hex", "compiled rule must carry charset = hex");
        assert_eq!(
            er.min_length, 32,
            "compiled rule must carry min_length = 32"
        );
        assert!(
            (er.threshold - 3.0).abs() < 1e-9,
            "compiled rule must carry threshold ≈ 3.0, got {}",
            er.threshold
        );
    }
}

// ── [scan] exclude_paths glob matcher — Task 5 ────────────────────────────────
//
// `glob_match(path: &str, glob: &str) -> bool` is the dep-free matcher backing
// `[scan] exclude_paths`.  Supported syntax (ADR-0007 / plan §Conventions):
//   - a leading/embedded `*` wildcard (e.g. `*.lock`, `*.min.js`)
//   - a trailing-slash directory prefix (e.g. `tests/fixtures/` matches any path under it).
// `glob_match` does not exist yet — this module references it, so the crate will not compile
// until Task 5 adds it.  That compile error naming `glob_match` is the intended Task-5 red.
#[cfg(test)]
mod glob_match_tests {
    use super::*;

    #[test]
    fn star_lock_matches_cargo_lock() {
        assert!(
            glob_match("Cargo.lock", "*.lock"),
            "`*.lock` must match `Cargo.lock`"
        );
    }

    #[test]
    fn star_lock_does_not_match_rust_file() {
        assert!(
            !glob_match("a.rs", "*.lock"),
            "`*.lock` must NOT match `a.rs`"
        );
    }

    #[test]
    fn trailing_slash_dir_prefix_matches_file_under_it() {
        assert!(
            glob_match("tests/fixtures/x.txt", "tests/fixtures/"),
            "a trailing-slash dir glob must match any path beneath it"
        );
    }

    #[test]
    fn star_min_js_matches_app_min_js() {
        assert!(
            glob_match("app.min.js", "*.min.js"),
            "`*.min.js` must match `app.min.js`"
        );
    }
}

#[cfg(test)]
mod is_protected_tests {
    use super::*;

    // ── Helpers ───────────────────────────────────────────────────────────────

    /// Stable fake framework root (no filesystem access needed — all lexical).
    fn fw_root() -> std::path::PathBuf {
        std::path::PathBuf::from("/framework/root")
    }

    /// Framework-anchored protected set (exact-match entries; no artifact paths).
    fn fw_protected() -> Vec<String> {
        vec![
            "security/rules.toml".to_string(),
            "gatekeeper/src/scan.rs".to_string(),
        ]
    }

    /// Artifact-anchored protected set: single entry "memory", resolved at runtime against the
    /// artifacts root.  This covers both layouts without hard-coding either full path.
    fn artifact_protected() -> Vec<String> {
        vec!["memory".to_string()]
    }

    // ── Framework-repo case (equal roots): artifacts_root = fw_root/docs ─────

    /// In the framework repo the artifacts root is <framework_root>/docs, so the artifact entry
    /// "memory" resolves to <framework_root>/docs/memory, which is the docs/memory/ path.
    fn fw_artifacts_root() -> std::path::PathBuf {
        fw_root().join("docs")
    }

    // ── Governed-project case: distinct project and framework roots ───────────

    /// A project root that is clearly separate from the framework root.
    fn project_root() -> std::path::PathBuf {
        std::path::PathBuf::from("/some/project")
    }

    /// In a governed project the artifacts root is <project>/.claude/topology, so the artifact
    /// entry "memory" resolves to <project>/.claude/topology/memory.
    fn project_artifacts_root() -> std::path::PathBuf {
        project_root().join(".claude").join("topology")
    }

    // ── PROTECTED — framework-repo layout (docs/memory) ──────────────────────
    //
    // In the framework repo: artifacts_root = <fw_root>/docs, entry "memory" → docs/memory/.
    // All paths here use ABSOLUTE forms (as the hook always delivers), resolved under fw_root.

    #[test]
    fn file_inside_docs_memory_is_protected() {
        // Absolute path under fw_root/docs/memory — resolved against artifacts_root it matches the
        // "memory" entry exactly by Path::starts_with.
        let target = format!("{}/docs/memory/x.md", fw_root().display());
        assert!(
            is_protected(&fw_artifacts_root(), &artifact_protected(), &target),
            "a file inside docs/memory/ must be protected (framework-repo path)"
        );
    }

    #[test]
    fn absolute_in_repo_path_to_docs_memory_file_is_protected() {
        let abs = format!("{}/docs/memory/some.handoff.md", fw_root().display());
        assert!(
            is_protected(&fw_artifacts_root(), &artifact_protected(), &abs),
            "an absolute in-repo path into docs/memory/ must be protected"
        );
    }

    #[test]
    fn dotdot_alias_into_docs_memory_is_protected() {
        // Absolute path with a .. component that resolves back to docs/memory/x.md.
        // /framework/root/docs/memory/../memory/x.md → /framework/root/docs/memory/x.md
        let target = format!("{}/docs/memory/../memory/x.md", fw_root().display());
        assert!(
            is_protected(&fw_artifacts_root(), &artifact_protected(), &target),
            "a .. alias that resolves into docs/memory/ must be protected"
        );
    }

    #[test]
    fn trailing_slash_docs_memory_is_protected() {
        // Absolute path with trailing slash; Path strips it, so it resolves to docs/memory.
        let target = format!("{}/docs/memory/", fw_root().display());
        assert!(
            is_protected(&fw_artifacts_root(), &artifact_protected(), &target),
            "docs/memory/ (trailing slash) must be protected"
        );
    }

    // ── PROTECTED — governed-project layout (.claude/topology/memory) ─────────
    //
    // These three tests use DISTINCT framework and artifacts roots (governed scenario) and assert
    // protection via the artifact-anchored set.  Previously they were masked because both the
    // target path and the protected entry were resolved against the SAME (framework) root — fixing
    // the protected-path bypass bug (ADR-0013).

    #[test]
    fn file_inside_claude_topology_memory_is_protected() {
        // Governed-project case: artifacts_root = <project>/.claude/topology, entry "memory".
        // The target path is absolute under the PROJECT root, not the framework root.
        let target = format!("{}/.claude/topology/memory/x.md", project_root().display());
        assert!(
            is_protected(&project_artifacts_root(), &artifact_protected(), &target),
            "a file inside <project>/.claude/topology/memory/ must be protected (governed-project path)"
        );
    }

    #[test]
    fn absolute_in_repo_path_to_claude_topology_memory_is_protected() {
        let abs = format!(
            "{}/.claude/topology/memory/some.handoff.md",
            project_root().display()
        );
        assert!(
            is_protected(&project_artifacts_root(), &artifact_protected(), &abs),
            "an absolute in-project path into .claude/topology/memory/ must be protected"
        );
    }

    #[test]
    fn trailing_slash_claude_topology_memory_is_protected() {
        let target = format!("{}/.claude/topology/memory/", project_root().display());
        assert!(
            is_protected(&project_artifacts_root(), &artifact_protected(), &target),
            "<project>/.claude/topology/memory/ (trailing slash) must be protected"
        );
    }

    // ── REGRESSION: the old bypass bug ───────────────────────────────────────
    //
    // Before the fix, `is_protected` resolved both the target and the entry against a SINGLE root
    // (the framework root).  A governed-project absolute path such as
    // `<project>/.claude/topology/memory/x.md` was NOT matched when root = framework_root (a
    // different directory), so the protection was silently bypassed.
    //
    // This test pins that exact scenario:
    //   - framework root:  /framework/root
    //   - project root:    /some/project
    //   - target:          /some/project/.claude/topology/memory/x.md
    //
    // The artifact-anchored call IS protected; the framework-anchored call (old bug) is NOT.

    #[test]
    fn governed_project_memory_bypasses_framework_anchored_check() {
        // Pin the old bug: resolving a governed-project path against the framework root alone
        // must NOT match — this is the bypass that is_protected_any fixes.
        let target = format!("{}/.claude/topology/memory/x.md", project_root().display());
        // Framework-anchored with the old entry style: does NOT cover governed project.
        let old_fw_entries = vec![".claude/topology/memory".to_string()];
        assert!(
            !is_protected(&fw_root(), &old_fw_entries, &target),
            "framework-root-anchored check alone must NOT match a path under the project root \
             (this is the bypass; is_protected_any fixes it)"
        );
        // Artifact-anchored DOES cover it.
        assert!(
            is_protected(&project_artifacts_root(), &artifact_protected(), &target),
            "artifact-root-anchored check MUST match the governed-project path"
        );
        // And is_protected_any combines both correctly.
        // The target is already absolute, so target_base does not affect resolution — use fw_root()
        // as the base (this matches the hook lane where absolute paths are the norm).
        assert!(
            is_protected_any(
                &fw_root(),
                &fw_root(),
                &[],
                &project_artifacts_root(),
                &artifact_protected(),
                &target,
            ),
            "is_protected_any must return true when the artifact-anchored check matches"
        );
    }

    // ── PROTECTED — exact-match framework entries ─────────────────────────────

    #[test]
    fn exact_match_entry_still_protected() {
        // Regression: exact-match framework entries must keep working after the split.
        assert!(
            is_protected(&fw_root(), &fw_protected(), "security/rules.toml"),
            "existing exact-match protected paths must still be protected"
        );
    }

    // ── NOT PROTECTED ─────────────────────────────────────────────────────────

    #[test]
    fn template_file_in_memory_root_not_protected() {
        // <fw_root>/memory/TEMPLATE.handoff.md is NOT under <fw_artifacts_root>/memory/
        // (i.e. not under <fw_root>/docs/memory/), so it must not be protected.
        let target = format!("{}/memory/TEMPLATE.handoff.md", fw_root().display());
        assert!(
            !is_protected(&fw_artifacts_root(), &artifact_protected(), &target),
            "memory/TEMPLATE.handoff.md must NOT be protected (not inside docs/memory/ or .claude/topology/memory/)"
        );
    }

    #[test]
    fn docs_memory_evil_sibling_not_protected() {
        // docs/memory-evil/ shares a string prefix with docs/memory but
        // Path::starts_with is component-wise and must reject it.
        let target = format!("{}/docs/memory-evil/x.md", fw_root().display());
        assert!(
            !is_protected(&fw_artifacts_root(), &artifact_protected(), &target),
            "docs/memory-evil/x.md must NOT be protected — Path::starts_with is component-wise"
        );
    }

    #[test]
    fn old_memory_artifacts_path_no_longer_protected() {
        // Regression guard: after ADR-0013 the old path must not be in the artifact-protected set.
        // <fw_root>/memory/artifacts/x.md is NOT under <fw_artifacts_root>/memory/ (docs/memory/).
        let target = format!("{}/memory/artifacts/x.md", fw_root().display());
        assert!(
            !is_protected(&fw_artifacts_root(), &artifact_protected(), &target),
            "memory/artifacts/x.md must NOT be protected under the new protected set"
        );
    }

    // ── target_base / entry-anchor separation (double-prefix regression) ──────
    //
    // These pin the specific staged-lane bug found by the fresh-context critic:
    //
    // (a) DOUBLE-PREFIX MISS: in the staged lane git emits repo-root-relative paths.
    //     The path "docs/memory/x.handoff.md" must be resolved against the REPO ROOT
    //     (`fw_root()`), NOT against `artifacts_root` (`fw_root()/docs`).  The old code
    //     resolved against `artifacts_root`, yielding `fw_root/docs/docs/memory/x.handoff.md`,
    //     which does NOT start_with `fw_root/docs/memory` — a miss (tamper ALLOWED).
    //
    // (b) SPURIOUS BLOCK: "memory/README.md" resolved against `fw_root/docs` gives
    //     `fw_root/docs/memory/README.md`, which starts_with `fw_root/docs/memory` — a false
    //     positive (unprotected seed file spuriously BLOCKED).
    //
    // The fix: is_protected_any takes an explicit `target_base` that is separate from each set's
    // anchor root.  In the staged lane target_base = fw_root (the repo root).

    #[test]
    fn staged_relative_docs_memory_file_is_protected_with_fw_root_as_base() {
        // (a) "docs/memory/x.handoff.md" from git diff --cached, target_base = fw_root().
        // The artifact entry "memory" resolves to fw_artifacts_root()/memory = fw_root/docs/memory.
        // Resolved target = fw_root/docs/memory/x.handoff.md → starts_with fw_root/docs/memory → BLOCKED.
        assert!(
            is_protected_any(
                &fw_root(),            // target_base: repo-root-relative git path
                &fw_root(),            // framework anchor (no framework entries in this check)
                &[],
                &fw_artifacts_root(),  // artifact anchor: <fw_root>/docs
                &artifact_protected(), // ["memory"] → <fw_root>/docs/memory
                "docs/memory/x.handoff.md",
            ),
            "repo-root-relative 'docs/memory/x.handoff.md' with target_base=fw_root must be protected"
        );
    }

    #[test]
    fn staged_relative_memory_readme_not_spuriously_blocked() {
        // (b) "memory/README.md" from git diff --cached, target_base = fw_root().
        // Resolved target = fw_root/memory/README.md.
        // Artifact entry "memory" resolves to fw_artifacts_root()/memory = fw_root/docs/memory.
        // fw_root/memory/README.md does NOT start_with fw_root/docs/memory → NOT protected.
        assert!(
            !is_protected_any(
                &fw_root(), // target_base: repo-root-relative git path
                &fw_root(), // framework anchor
                &[],
                &fw_artifacts_root(),  // artifact anchor: <fw_root>/docs
                &artifact_protected(), // ["memory"] → <fw_root>/docs/memory
                "memory/README.md",
            ),
            "repo-root-relative 'memory/README.md' with target_base=fw_root must NOT be protected"
        );
    }

    #[test]
    fn absolute_and_relative_spellings_agree_via_is_protected_any() {
        // An absolute path and its repo-root-relative spelling must produce the same result when
        // target_base = fw_root().  This pins the check-path consistency guarantee.
        let rel = "security/rules.toml";
        let abs = format!("{}/{}", fw_root().display(), rel);
        let fw_protected = fw_protected();
        let no_artifacts: Vec<String> = vec![];

        let result_rel = is_protected_any(
            &fw_root(),
            &fw_root(),
            &fw_protected,
            &fw_artifacts_root(),
            &no_artifacts,
            rel,
        );
        let result_abs = is_protected_any(
            &fw_root(),
            &fw_root(),
            &fw_protected,
            &fw_artifacts_root(),
            &no_artifacts,
            &abs,
        );
        assert_eq!(
            result_rel, result_abs,
            "relative and absolute spellings of the same protected path must agree: \
             rel={result_rel}, abs={result_abs}"
        );
        assert!(
            result_rel,
            "security/rules.toml must be protected in both spellings"
        );
    }
}
