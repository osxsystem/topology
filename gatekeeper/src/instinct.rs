//! Instincts engine — the weakest operator: tiny, always-on, reasoning-based guardrails.
//!
//! An instinct is a hyper-lean Markdown file (`instincts/<id>.md`): YAML-ish frontmatter (`id`,
//! `priority`, optional `schema`/`source`) + a 1–2 sentence *why* body. Instincts carry NO scope —
//! they are always-on; `activate` injects the whole set. The frontmatter is parsed by hand (std only;
//! no YAML crate). See docs/specs/2026-06-07-instincts-engine.md.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Priority {
    High,
    Medium,
    Low,
}

impl Priority {
    fn rank(self) -> u8 {
        match self {
            Priority::High => 0,
            Priority::Medium => 1,
            Priority::Low => 2,
        }
    }
    fn as_str(self) -> &'static str {
        match self {
            Priority::High => "high",
            Priority::Medium => "medium",
            Priority::Low => "low",
        }
    }
    fn parse(s: &str) -> Result<Priority, String> {
        match s {
            "high" => Ok(Priority::High),
            "medium" => Ok(Priority::Medium),
            "low" => Ok(Priority::Low),
            other => Err(format!(
                "invalid priority '{other}' (expected high|medium|low)"
            )),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Instinct {
    id: String,
    priority: Priority,
    #[allow(dead_code)] // accepted + validated; surfaced in Phase 3, not Phase 2
    schema: u32,
    #[allow(dead_code)] // accept-but-ignore provenance; read by Phase-3 promote
    source: Option<String>,
    body: String,
}

impl Instinct {
    /// The body collapsed to a single whitespace-normalized line (for preamble rendering).
    fn body_oneline(&self) -> String {
        self.body.split_whitespace().collect::<Vec<_>>().join(" ")
    }
    /// Word count of the body — the unit `--budget` truncates on.
    fn word_count(&self) -> usize {
        self.body.split_whitespace().count()
    }
}

/// Strip one layer of matching surrounding quotes, if present.
fn unquote(v: &str) -> &str {
    let v = v.trim();
    if (v.starts_with('"') && v.ends_with('"') && v.len() >= 2)
        || (v.starts_with('\'') && v.ends_with('\'') && v.len() >= 2)
    {
        &v[1..v.len() - 1]
    } else {
        v
    }
}

/// kebab-case, 1..=64 chars, no leading/trailing/double hyphen, no reserved word.
/// `pub` so Phase-3 `learn` validates ledger ids against the same rule an instinct id obeys.
pub fn validate_id(id: &str) -> Result<(), String> {
    if id.is_empty() || id.len() > 64 {
        return Err(format!("id '{id}': must be 1..=64 chars"));
    }
    let lc = id.to_lowercase();
    if lc.contains("claude") || lc.contains("anthropic") {
        return Err(format!(
            "id '{id}': must not contain a reserved word (claude/anthropic)"
        ));
    }
    let charset_ok = id
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if !charset_ok || id.starts_with('-') || id.ends_with('-') || id.contains("--") {
        return Err(format!(
            "id '{id}': must be kebab-case [a-z0-9-] with no leading/trailing/double hyphen"
        ));
    }
    Ok(())
}

/// Parse one instinct file: `---`-fenced frontmatter then a Markdown body. Any defect is an Err
/// (the caller maps it to skip+warn or exit 2 per the fail-mode matrix).
fn parse_instinct(raw: &str) -> Result<Instinct, String> {
    let text = raw.replace("\r\n", "\n");
    let after_open = text
        .strip_prefix("---\n")
        .ok_or("missing opening '---' frontmatter fence")?;

    // Walk frontmatter lines until a line that is exactly "---".
    let mut id: Option<String> = None;
    let mut priority = Priority::Medium;
    let mut schema = SCHEMA_VERSION;
    let mut source: Option<String> = None;
    let mut body_offset: Option<usize> = None;
    let mut offset = 0usize;
    for line in after_open.split_inclusive('\n') {
        let content = line.trim_end_matches('\n');
        if content.trim() == "---" {
            body_offset = Some(offset + line.len());
            break;
        }
        offset += line.len();
        let trimmed = content.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let (key, value) = trimmed
            .split_once(':')
            .ok_or_else(|| format!("frontmatter line is not 'key: value': {trimmed}"))?;
        let key = key.trim();
        let value = unquote(value);
        match key {
            "id" => id = Some(value.to_string()),
            "priority" => priority = Priority::parse(value)?,
            "schema" => {
                schema = value
                    .parse::<u32>()
                    .map_err(|_| format!("schema '{value}': expected a non-negative integer"))?;
            }
            "source" => source = Some(value.to_string()),
            other => return Err(format!("unknown frontmatter field '{other}'")),
        }
    }

    let body_offset = body_offset.ok_or("missing closing '---' frontmatter fence")?;
    let id = id.ok_or("missing required field 'id'")?;
    validate_id(&id)?;
    if schema != SCHEMA_VERSION {
        return Err(format!(
            "unsupported schema {schema} (expected {SCHEMA_VERSION})"
        ));
    }
    let body = after_open[body_offset..].trim().to_string();
    if body.is_empty() {
        return Err(format!("instinct '{id}': body (the why) is empty"));
    }
    Ok(Instinct {
        id,
        priority,
        schema,
        source,
        body,
    })
}

/// Validate that `raw` is a well-formed instinct file (the same contract `parse_instinct` enforces).
/// `pub` so Phase-3 `learn promote` proves a scaffolded instinct is valid before writing it.
pub fn validate_instinct_str(raw: &str) -> Result<(), String> {
    parse_instinct(raw).map(|_| ())
}

/// Load every `*.md` instinct under `dir`, sorted by (priority high→low, then id).
///
/// Fail-mode (design decision E): a missing dir yields an empty set in both modes. On a per-file
/// parse error or a duplicate id, `strict` mode returns Err (the `list`/`render` path → exit 2);
/// non-strict mode skips the offender, pushes a warning, and continues (the `activate` path → exit 0,
/// never breaking the turn).
fn load_instincts(
    dir: &Path,
    strict: bool,
    warnings: &mut Vec<String>,
) -> Result<Vec<Instinct>, String> {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(ref e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => {
            let msg = format!("cannot read {}: {e}", dir.display());
            return if strict {
                Err(msg)
            } else {
                warnings.push(msg);
                Ok(Vec::new())
            };
        }
    };

    let mut paths: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("md"))
        .collect();
    paths.sort(); // deterministic processing order

    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for p in paths {
        let raw = match fs::read_to_string(&p) {
            Ok(r) => r,
            Err(e) => {
                let msg = format!("{}: read error: {e}", p.display());
                if strict {
                    return Err(msg);
                }
                warnings.push(msg);
                continue;
            }
        };
        let inst = match parse_instinct(&raw) {
            Ok(i) => i,
            Err(e) => {
                let msg = format!("{}: {e}", p.display());
                if strict {
                    return Err(msg);
                }
                warnings.push(msg);
                continue;
            }
        };
        if !seen.insert(inst.id.clone()) {
            let msg = format!("duplicate instinct id '{}' ({})", inst.id, p.display());
            if strict {
                return Err(msg);
            }
            warnings.push(msg);
            continue;
        }
        out.push(inst);
    }

    out.sort_by(|a, b| {
        a.priority
            .rank()
            .cmp(&b.priority.rank())
            .then_with(|| a.id.cmp(&b.id))
    });
    Ok(out)
}

/// The fixed header that demarcates the instincts section from routed skills in the preamble.
const PREAMBLE_HEADER: &str =
    "Always-on instincts — how to reason here (framing you may reason past only with cause):";

/// Render the preamble section: header + one `- [id] why` line per instinct. Ends with a newline.
/// Note the deliberate absence of skills' `[enforcement]` tag — instincts are soft framing.
fn render_preamble(items: &[&Instinct]) -> String {
    let mut s = String::from(PREAMBLE_HEADER);
    s.push('\n');
    for i in items {
        s.push_str(&format!("  - [{}] {}\n", i.id, i.body_oneline()));
    }
    s
}

/// Keep the highest-priority prefix whose total body word count fits `budget` (design decision D:
/// drop lowest-priority-first, whole instincts only — never split a body). `None` keeps all.
fn budget_filter(items: &[Instinct], budget: Option<usize>) -> Vec<&Instinct> {
    match budget {
        None => items.iter().collect(),
        Some(max) => {
            let mut used = 0usize;
            let mut kept = Vec::new();
            for i in items {
                let w = i.word_count();
                if used + w > max {
                    break;
                }
                used += w;
                kept.push(i);
            }
            kept
        }
    }
}

/// Soft load + render for `cmd_activate`. Warns to stderr; returns "" when there are no instincts
/// (so a missing `instincts/` dir adds nothing and never breaks the turn).
pub fn activate_section(root: &Path) -> String {
    let mut warnings = Vec::new();
    let instincts =
        load_instincts(&root.join("instincts"), false, &mut warnings).unwrap_or_default();
    for w in &warnings {
        eprintln!("gatekeeper: instinct {w}");
    }
    if instincts.is_empty() {
        String::new()
    } else {
        let refs: Vec<&Instinct> = instincts.iter().collect();
        render_preamble(&refs)
    }
}

/// Adapter accessor (Phase 4): `(id, one-line body)` for every always-on instinct, sorted by
/// (priority high→low, then id). Strict load — a malformed `instincts/` dir is an `Err` the caller
/// surfaces (the `adapt` path → exit 2). See `crate::adapt`.
pub fn instincts_for_adapt(root: &Path) -> Result<Vec<(String, String)>, String> {
    let mut warnings = Vec::new();
    let list = load_instincts(&root.join("instincts"), true, &mut warnings)?;
    Ok(list
        .into_iter()
        .map(|i| {
            let why = i.body_oneline();
            (i.id, why)
        })
        .collect())
}

/// Entry point for `gatekeeper instinct ...`. Returns the process exit code (0 / 2).
pub fn cmd_instinct(args: &[String], root: &Path) -> i32 {
    match args.first().map(String::as_str) {
        Some("--help") | Some("-h") => {
            println!("{}", crate::USAGE_INSTINCT);
            0
        }
        Some("list") => {
            if let Some(code) = crate::check_help_or_unknown(
                "instinct list",
                &args[1..],
                &[],
                crate::USAGE_INSTINCT,
            ) {
                return code;
            }
            cmd_list_instincts(root)
        }
        Some("render") => cmd_render(&args[1..], root),
        _ => {
            eprintln!(
                "gatekeeper instinct: expected `list` or `render [--harness <h>] [--budget <n>]`"
            );
            2
        }
    }
}

fn cmd_list_instincts(root: &Path) -> i32 {
    let mut warnings = Vec::new();
    match load_instincts(&root.join("instincts"), true, &mut warnings) {
        Ok(list) => {
            for i in &list {
                println!("{}\t{}", i.id, i.priority.as_str());
            }
            0
        }
        Err(e) => {
            eprintln!("gatekeeper instinct list: {e}");
            2
        }
    }
}

fn cmd_render(args: &[String], root: &Path) -> i32 {
    // --help / -h handled via the generic helper so the render unknown-flag path is consistent.
    if let Some(code) = crate::check_help_or_unknown(
        "instinct render",
        args,
        &["--harness", "--budget"],
        crate::USAGE_INSTINCT,
    ) {
        return code;
    }
    let mut harness = "claude".to_string();
    let mut budget: Option<usize> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--harness" => match args.get(i + 1) {
                Some(h) => {
                    harness = h.clone();
                    i += 2;
                }
                None => {
                    eprintln!("gatekeeper instinct render: --harness needs a value");
                    return 2;
                }
            },
            "--budget" => match args.get(i + 1).and_then(|n| n.parse::<usize>().ok()) {
                Some(n) => {
                    budget = Some(n);
                    i += 2;
                }
                None => {
                    eprintln!("gatekeeper instinct render: --budget needs a non-negative integer");
                    return 2;
                }
            },
            // Unknown flags already rejected by check_help_or_unknown above; this arm is
            // unreachable but satisfies the exhaustiveness check.
            _ => {
                i += 1;
            }
        }
    }
    if harness != "claude" {
        eprintln!("gatekeeper instinct render: harness '{harness}' not supported in Phase 2 (only 'claude')");
        return 2;
    }
    let mut warnings = Vec::new();
    let list = match load_instincts(&root.join("instincts"), true, &mut warnings) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("gatekeeper instinct render: {e}");
            return 2;
        }
    };
    let kept = budget_filter(&list, budget);
    print!("{}", render_preamble(&kept));
    0
}

#[cfg(test)]
mod parse_tests {
    use super::*;

    const VALID: &str = "---\nid: evidence-over-assertion\npriority: high\nsource: doc:ROADMAP\n---\n\"Done\" means a re-runnable command and its output, never a feeling.\n";

    #[test]
    fn valid_parses() {
        let i = parse_instinct(VALID).unwrap();
        assert_eq!(i.id, "evidence-over-assertion");
        assert_eq!(i.priority, Priority::High);
        assert_eq!(i.schema, 1);
        assert_eq!(i.source.as_deref(), Some("doc:ROADMAP"));
        assert!(i.body.starts_with("\"Done\""));
    }
    #[test]
    fn priority_defaults_to_medium() {
        let src = "---\nid: surgical-changes-only\n---\nChange only what the task needs.\n";
        assert_eq!(parse_instinct(src).unwrap().priority, Priority::Medium);
    }
    #[test]
    fn missing_id_rejected() {
        assert!(parse_instinct("---\npriority: high\n---\nbody\n").is_err());
    }
    #[test]
    fn unknown_field_rejected() {
        let bad = "---\nid: x\napplies: always\n---\nbody\n";
        let err = parse_instinct(bad).unwrap_err();
        assert!(err.contains("unknown frontmatter field 'applies'"), "{err}");
    }
    #[test]
    fn bad_priority_rejected() {
        assert!(parse_instinct("---\nid: x\npriority: urgent\n---\nbody\n").is_err());
    }
    #[test]
    fn bad_schema_rejected() {
        assert!(parse_instinct("---\nid: x\nschema: 9\n---\nbody\n").is_err());
    }
    #[test]
    fn reserved_word_in_id_rejected() {
        assert!(parse_instinct("---\nid: ask-claude\n---\nbody\n").is_err());
    }
    #[test]
    fn non_kebab_id_rejected() {
        assert!(parse_instinct("---\nid: Bad_Id\n---\nbody\n").is_err());
        assert!(parse_instinct("---\nid: -lead\n---\nbody\n").is_err());
        assert!(parse_instinct("---\nid: dou--ble\n---\nbody\n").is_err());
    }
    #[test]
    fn empty_body_rejected() {
        assert!(parse_instinct("---\nid: x\n---\n\n").is_err());
    }
    #[test]
    fn missing_closing_fence_rejected() {
        assert!(parse_instinct("---\nid: x\nbody with no fence\n").is_err());
    }
}

#[cfg(test)]
mod load_tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("topo_instload_{tag}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }
    fn write(dir: &Path, name: &str, body: &str) {
        fs::write(dir.join(name), body).unwrap();
    }

    #[test]
    fn missing_dir_is_empty_both_modes() {
        let dir = std::env::temp_dir().join("topo_instload_absent_does_not_exist");
        let mut w = Vec::new();
        assert!(load_instincts(&dir, true, &mut w).unwrap().is_empty());
        assert!(load_instincts(&dir, false, &mut w).unwrap().is_empty());
        assert!(w.is_empty());
    }
    #[test]
    fn sorts_priority_then_id() {
        let dir = scratch("sort");
        write(
            &dir,
            "b.md",
            "---\nid: b-medium\npriority: medium\n---\nwhy b\n",
        );
        write(
            &dir,
            "a.md",
            "---\nid: a-high\npriority: high\n---\nwhy a\n",
        );
        write(
            &dir,
            "c.md",
            "---\nid: c-high\npriority: high\n---\nwhy c\n",
        );
        let mut w = Vec::new();
        let list = load_instincts(&dir, true, &mut w).unwrap();
        let ids: Vec<&str> = list.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(
            ids,
            vec!["a-high", "c-high", "b-medium"],
            "high (by id) then medium"
        );
        let _ = fs::remove_dir_all(&dir);
    }
    #[test]
    fn malformed_file_strict_errs_soft_warns() {
        let dir = scratch("malformed");
        write(&dir, "ok.md", "---\nid: ok\n---\nfine\n");
        write(&dir, "bad.md", "---\nid: bad\nbogus: 1\n---\nnope\n");
        let mut w = Vec::new();
        assert!(load_instincts(&dir, true, &mut w).is_err());
        let mut w2 = Vec::new();
        let soft = load_instincts(&dir, false, &mut w2).unwrap();
        assert_eq!(soft.len(), 1, "soft mode keeps the good file");
        assert_eq!(w2.len(), 1, "soft mode warns about the bad one");
        let _ = fs::remove_dir_all(&dir);
    }
    #[test]
    fn duplicate_id_strict_errs() {
        let dir = scratch("dupe");
        write(&dir, "one.md", "---\nid: dup\n---\nfirst\n");
        write(&dir, "two.md", "---\nid: dup\n---\nsecond\n");
        let mut w = Vec::new();
        let err = load_instincts(&dir, true, &mut w).unwrap_err();
        assert!(err.contains("duplicate instinct id 'dup'"), "{err}");
        let _ = fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod render_tests {
    use super::*;

    fn inst(id: &str, prio: Priority, body: &str) -> Instinct {
        Instinct {
            id: id.to_string(),
            priority: prio,
            schema: 1,
            source: None,
            body: body.to_string(),
        }
    }

    #[test]
    fn preamble_has_header_and_id_lines_no_enforcement_tag() {
        let a = inst(
            "evidence-over-assertion",
            Priority::High,
            "Done means a re-runnable command.",
        );
        let refs = vec![&a];
        let out = render_preamble(&refs);
        assert!(out.starts_with(PREAMBLE_HEADER));
        assert!(out.contains("  - [evidence-over-assertion] Done means a re-runnable command."));
        assert!(
            !out.contains("[suggest]") && !out.contains("[block]"),
            "no enforcement tag"
        );
    }
    #[test]
    fn budget_drops_lowest_priority_whole() {
        // high=2 words, medium=5 words; sorted high first.
        let hi = inst("a-hi", Priority::High, "one two");
        let mid = inst("b-mid", Priority::Medium, "one two three four five");
        let all = vec![hi, mid];
        // budget 4: only the 2-word high fits; the 5-word medium is dropped whole.
        let kept = budget_filter(&all, Some(4));
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].id, "a-hi");
        // budget 7: both fit (2 + 5).
        assert_eq!(budget_filter(&all, Some(7)).len(), 2);
        // no budget: all.
        assert_eq!(budget_filter(&all, None).len(), 2);
    }
    #[test]
    fn body_oneline_collapses_whitespace() {
        let i = inst("x", Priority::Low, "line one\n  line   two\n");
        assert_eq!(i.body_oneline(), "line one line two");
        assert_eq!(i.word_count(), 4);
    }
}
