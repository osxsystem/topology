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
use std::process::Command;

use regex::bytes::{Regex, RegexSet};
use serde::Deserialize;

const SCHEMA_VERSION: u32 = 1;
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

/// Entry point for `gatekeeper scan ...`. `root` is the framework root. Returns the process exit
/// code (0 clean / 1 veto / 2 usage or load error). Rules load first so a broken rules file
/// fails closed (exit 2) on every subcommand.
pub fn cmd_scan(args: &[String], root: &Path) -> i32 {
    let rules_path = root.join("security").join("rules.toml");
    let rules = match load_rules(&rules_path) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("gatekeeper scan: cannot load {}: {e}", rules_path.display());
            return 2;
        }
    };
    match args.first().map(String::as_str) {
        Some("--hook") => scan_hook(&rules, root),
        Some("--cmd") => scan_cmd_cmd(&rules),
        Some("--check-path") => scan_check_path(&rules, args.get(1).map(String::as_str)),
        Some("--staged") => scan_staged(&rules, root, STAGED_BLOB_CAP),
        Some("--content") => scan_content_cmd(&rules),
        _ => {
            eprintln!(
                "gatekeeper scan: expected --hook | --cmd | --content | --staged | --check-path <path>"
            );
            2
        }
    }
}

fn scan_content_cmd(rules: &Rules) -> i32 {
    let data = match read_stdin_bytes(HOOK_INPUT_CAP) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("BLOCK oversize-input: {e}");
            return 1; // fail closed
        }
    };
    report(&scan_with(&rules.content_set, &rules.content, &data, &rules.allows, None))
}

fn scan_cmd_cmd(rules: &Rules) -> i32 {
    let data = match read_stdin_bytes(HOOK_INPUT_CAP) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("BLOCK oversize-input: {e}");
            return 1;
        }
    };
    let mut findings = scan_with(&rules.content_set, &rules.content, &data, &rules.allows, None);
    findings.extend(scan_with(&rules.command_set, &rules.command, &data, &rules.allows, None));
    report(&findings)
}

/// Compare repo-relative paths with forward slashes, ignoring a leading "./".
fn normalize_path(p: &str) -> String {
    p.trim_start_matches("./").replace('\\', "/")
}

fn is_protected(protected: &[String], path: &str) -> bool {
    let norm = normalize_path(path);
    protected.iter().any(|p| normalize_path(p) == norm)
}

fn scan_check_path(rules: &Rules, path: Option<&str>) -> i32 {
    match path {
        Some(p) if is_protected(&rules.protected, p) => 1,
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
        return Err(format!("git {args:?} exited {}", out.status.code().unwrap_or(-1)));
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
        let n = if status.starts_with('R') || status.starts_with('C') { 2 } else { 1 };
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
    Ok(String::from_utf8_lossy(&git_raw(root, &["rev-parse", &format!(":{path}")])?)
        .trim()
        .to_string())
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
    String::from_utf8_lossy(&out).split_whitespace().next().map(str::to_string)
}

fn scan_staged(rules: &Rules, root: &Path, cap: usize) -> i32 {
    let mut blocked = false;

    // (1) Scan enumeration: ACMR — content of each added/copied/modified/renamed staged blob.
    match git_paths_z(
        root,
        &["diff", "--cached", "--name-only", "-z", "--diff-filter=ACMR"],
    ) {
        Ok(paths) => {
            for path in paths {
                // Submodule gitlinks (mode 160000) are commit pointers, not content — skip (not
                // recursed); the pointed-to commit may not even be in this repo's object store.
                if git_index_mode(root, &path).as_deref() == Some("160000") {
                    continue;
                }
                // Size FIRST (a cheap header read), so an oversize blob never streams into memory.
                let size = match git_blob_size(root, &path) {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("BLOCK staged-size: {e}");
                        blocked = true;
                        continue;
                    }
                };
                if size > cap {
                    // Oversize: never read content; the OID allowlist check is content-free too.
                    if !is_blob_allowlisted(root, &path, &rules.allow_blobs) {
                        eprintln!("BLOCK unscannable-blob: {path} (over {cap}-byte cap); allowlist via [[allow_blob]] path + blob_oid");
                        blocked = true;
                    }
                    continue;
                }
                // Size is within the cap, so reading the content is now bounded.
                match git_raw(root, &["show", &format!(":{path}")]) {
                    Ok(blob) => {
                        if blob.iter().take(8192).any(|&b| b == 0) {
                            // Binary/undecodable: block unless allowlisted by path + OID.
                            if !is_blob_allowlisted(root, &path, &rules.allow_blobs) {
                                eprintln!("BLOCK unscannable-blob: {path} (binary/undecodable); allowlist via [[allow_blob]] path + blob_oid");
                                blocked = true;
                            }
                            continue;
                        }
                        let f = scan_with(&rules.content_set, &rules.content, &blob, &rules.allows, Some(&path));
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

    // (2) Integrity enumeration: ACDMRT — broader; both rename sides vs protected_paths.
    match git_name_status_z(
        root,
        &["diff", "--cached", "--name-status", "-z", "-M", "--diff-filter=ACDMRT"],
    ) {
        Ok(entries) => {
            for (status, paths) in entries {
                for p in &paths {
                    if is_protected(&rules.protected, p) {
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
    let reason =
        format!("Topology: '{path}' is a protected safety file — human approval required to modify it.");
    println!("{}", decision_json("ask", &reason));
    0
}

fn apply_edit(text: &str, old: &str, new: &str, replace_all: bool) -> String {
    if old.is_empty() {
        return text.to_string();
    }
    if replace_all {
        text.replace(old, new)
    } else {
        text.replacen(old, new, 1)
    }
}

/// Read at most cap+1 bytes of a file. None if it is unreadable OR over the cap — the caller then
/// falls back to scanning the added text (the full-file secret is still caught at pre-commit).
fn read_file_capped(path: &str, cap: usize) -> Option<String> {
    let mut buf = Vec::new();
    fs::File::open(path).ok()?.take(cap as u64 + 1).read_to_end(&mut buf).ok()?;
    if buf.len() > cap {
        return None;
    }
    Some(String::from_utf8_lossy(&buf).into_owned())
}

/// Reconstruct the full post-edit file (bounded read). If the file is unreadable or over the cap,
/// fall back to the added text so a secret in new content is still caught.
fn reconstruct(file_path: &str, ti: &ToolInput, cap: usize) -> String {
    match read_file_capped(file_path, cap) {
        Some(mut text) => {
            if let Some(edits) = &ti.edits {
                for e in edits {
                    text = apply_edit(&text, &e.old_string, &e.new_string, e.replace_all.unwrap_or(false));
                }
            } else if let (Some(old), Some(new)) = (&ti.old_string, &ti.new_string) {
                text = apply_edit(&text, old, new, ti.replace_all.unwrap_or(false));
            }
            text
        }
        None => match &ti.edits {
            Some(edits) => edits.iter().map(|e| e.new_string.clone()).collect::<Vec<_>>().join("\n"),
            None => ti.new_string.clone().unwrap_or_default(),
        },
    }
}

fn hook_path_protected(protected: &[String], file_path: &str, root: &Path) -> bool {
    let rel = Path::new(file_path)
        .strip_prefix(root)
        .map(|r| r.to_string_lossy().into_owned())
        .unwrap_or_else(|_| file_path.to_string());
    is_protected(protected, &rel)
}

fn scan_hook(rules: &Rules, root: &Path) -> i32 {
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
    match event.tool_name.as_str() {
        "Bash" => {
            let cmd = event.tool_input.command.unwrap_or_default();
            let bytes = cmd.as_bytes();
            let mut f = scan_with(&rules.content_set, &rules.content, bytes, &rules.allows, None);
            f.extend(scan_with(&rules.command_set, &rules.command, bytes, &rules.allows, None));
            emit_decision(&f)
        }
        "Write" => {
            if let Some(fp) = &event.tool_input.file_path {
                if hook_path_protected(&rules.protected, fp, root) {
                    return emit_ask(fp);
                }
            }
            let content = event.tool_input.content.unwrap_or_default();
            emit_decision(&scan_with(&rules.content_set, &rules.content, content.as_bytes(), &rules.allows, None))
        }
        "Edit" | "MultiEdit" => {
            let Some(fp) = event.tool_input.file_path.clone() else {
                return 0; // no file_path -> nothing to scan
            };
            if hook_path_protected(&rules.protected, &fp, root) {
                return emit_ask(&fp);
            }
            let text = reconstruct(&fp, &event.tool_input, HOOK_INPUT_CAP);
            emit_decision(&scan_with(&rules.content_set, &rules.content, text.as_bytes(), &rules.allows, None))
        }
        _ => 0, // out of scope (MCP / other tools): silent allow
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
        let rules = parse_rules("schema_version = 1").unwrap();
        assert_eq!(scan_staged(&rules, &root, 8), 1, "20-byte blob over an 8-byte cap blocks");
        // Allowlist it by its git object id -> passes (the OID is read content-free).
        let oid = git_blob_oid(&root, "big.txt").unwrap();
        let toml = format!(
            r#"schema_version = 1
[[allow_blob]]
path = "big.txt"
blob_oid = "{oid}"
"#
        );
        assert_eq!(scan_staged(&parse_rules(&toml).unwrap(), &root, 8), 0, "allowlisted by blob_oid passes");
        let _ = std::fs::remove_dir_all(&root);
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
                let _ = scan_with(&r.content_set, &r.content, input.as_bytes(), &r.allows, None);
                t.elapsed().as_micros()
            })
            .collect();
        us.sort_unstable();
        let q = |p: f64| us[((us.len() as f64 - 1.0) * p) as usize];
        println!("scan latency us: p50={} p95={} p99={}", q(0.50), q(0.95), q(0.99));
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
            let _ = scan_staged(&r, &root, STAGED_BLOB_CAP);
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
