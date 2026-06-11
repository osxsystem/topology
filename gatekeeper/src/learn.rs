//! Continuous learning — capture gotchas into an append-only ledger, then promote recurring ones into
//! standing operators (an instinct, a skill, or a security rule), with a human approving every promotion.
//!
//! The ledger lives at `<artifacts_root>/learn/ledger.md`.  In the framework repo the artifacts root is
//! `docs/`, so the path is `docs/learn/ledger.md` (unchanged from the pre-ADR-0013 value).  In a governed
//! project the artifacts root is `.claude/topology/`, so the ledger lands at
//! `.claude/topology/learn/ledger.md` — inside the project, not inside the payload.  See ADR-0013.
//!
//! `capture` only ever APPENDS a `## <id>` block; it never rewrites one, so a recurrence is just the same
//! id captured again — surfaced as an occurrence count by `learn list`. `promote` is the gated half: it
//! scaffolds the operator, validates it against that operator's OWN loader (the instinct parser /
//! `scan::load_rules` / the frontmatter `gatekeeper list` reads), prints a diff, and writes only on an
//! explicit `y` (or `--yes`). `promote` is only available in the framework repo; in a governed project it
//! refuses with a pointer to the fork story (ADR-0013 §3). Promotion never edits the ledger; provenance
//! lives on the operator as `source: ledger:<id>`. No new deps: the parser is hand-rolled on `std`, dates
//! arrive via `--date` (no clock), and promotions are add-only (no diff lib).
//! See docs/specs/2026-06-08-continuous-learning.md.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::instinct::{validate_id, validate_instinct_str};
use crate::scan;

/// The ledger's path, relative to the artifacts root.
/// Resolves to `docs/learn/ledger.md` in the framework repo (artifacts_root = docs/) and to
/// `.claude/topology/learn/ledger.md` in a governed project — see ADR-0013.
const LEDGER_REL: &str = "learn/ledger.md";

// ---------- model ----------

/// What surfaced the gotcha. `gate-failure` and `stop` are the hook-driven paths; `human-correction`
/// and `manual` are agent-driven.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Trigger {
    GateFailure,
    Stop,
    HumanCorrection,
    Manual,
}

impl Trigger {
    fn as_str(self) -> &'static str {
        match self {
            Trigger::GateFailure => "gate-failure",
            Trigger::Stop => "stop",
            Trigger::HumanCorrection => "human-correction",
            Trigger::Manual => "manual",
        }
    }
    fn parse(s: &str) -> Result<Trigger, String> {
        match s {
            "gate-failure" => Ok(Trigger::GateFailure),
            "stop" => Ok(Trigger::Stop),
            "human-correction" => Ok(Trigger::HumanCorrection),
            "manual" => Ok(Trigger::Manual),
            other => Err(format!(
                "invalid trigger '{other}' (expected gate-failure|stop|human-correction|manual)"
            )),
        }
    }
}

/// The promotion target an entry proposes (or that `--kind` selects).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Instinct,
    Skill,
    Rule,
}

impl Kind {
    fn as_str(self) -> &'static str {
        match self {
            Kind::Instinct => "instinct",
            Kind::Skill => "skill",
            Kind::Rule => "rule",
        }
    }
    fn parse(s: &str) -> Result<Kind, String> {
        match s {
            "instinct" => Ok(Kind::Instinct),
            "skill" => Ok(Kind::Skill),
            "rule" => Ok(Kind::Rule),
            other => Err(format!(
                "invalid kind '{other}' (expected instinct|skill|rule)"
            )),
        }
    }
}

/// One parsed ledger entry. The non-id, non-summary fields are provenance, surfaced in the `promote`
/// preview so the approving human sees where the gotcha came from.
#[derive(Debug, Clone)]
struct Entry {
    id: String,
    trigger: Trigger,
    gate: Option<String>,
    kind: Option<Kind>,
    date: Option<String>,
    source: Option<String>,
    summary: String,
}

// ---------- ledger parsing ----------

/// A record under construction while `parse_ledger` walks lines.
struct Partial {
    id: String,
    trigger: Trigger,
    gate: Option<String>,
    kind: Option<Kind>,
    date: Option<String>,
    source: Option<String>,
    summary_parts: Vec<String>,
}

impl Partial {
    fn new(id: String) -> Self {
        Partial {
            id,
            trigger: Trigger::Manual,
            gate: None,
            kind: None,
            date: None,
            source: None,
            summary_parts: Vec::new(),
        }
    }
    fn finish(self) -> Result<Entry, String> {
        let summary = self
            .summary_parts
            .join(" ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if summary.is_empty() {
            return Err(format!(
                "ledger entry '{}': summary (the '>' line) is empty",
                self.id
            ));
        }
        Ok(Entry {
            id: self.id,
            trigger: self.trigger,
            gate: self.gate,
            kind: self.kind,
            date: self.date,
            source: self.source,
            summary,
        })
    }
}

/// Parse the whole ledger. A line `## <id>` opens an entry (id validated like an instinct id); within
/// it, `- key: value` sets a known field (unknown key ⇒ Err naming id+key), `> …` lines are the summary
/// (joined, whitespace-normalized), blank lines are skipped. Anything before the first `## ` (a title,
/// intro prose) is preamble and ignored. Strict: any defect is an Err (the caller maps it to exit 2).
fn parse_ledger(raw: &str) -> Result<Vec<Entry>, String> {
    let text = raw.replace("\r\n", "\n");
    let mut entries = Vec::new();
    let mut cur: Option<Partial> = None;

    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("## ") {
            if let Some(p) = cur.take() {
                entries.push(p.finish()?);
            }
            let id = rest.trim().to_string();
            validate_id(&id).map_err(|e| format!("ledger entry: {e}"))?;
            cur = Some(Partial::new(id));
            continue;
        }
        let Some(p) = cur.as_mut() else {
            continue; // preamble before the first entry
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some(field) = trimmed.strip_prefix("- ") {
            let (key, value) = field.split_once(':').ok_or_else(|| {
                format!(
                    "ledger entry '{}': field line is not '- key: value': {trimmed}",
                    p.id
                )
            })?;
            let value = value.trim();
            let value = if value.is_empty() {
                None
            } else {
                Some(value.to_string())
            };
            match key.trim() {
                "trigger" => {
                    if let Some(v) = value {
                        p.trigger = Trigger::parse(&v)
                            .map_err(|e| format!("ledger entry '{}': {e}", p.id))?;
                    }
                }
                "gate" => p.gate = value,
                "kind" => {
                    p.kind = match value {
                        Some(v) => Some(
                            Kind::parse(&v).map_err(|e| format!("ledger entry '{}': {e}", p.id))?,
                        ),
                        None => None,
                    };
                }
                "date" => p.date = value,
                "source" => p.source = value,
                other => {
                    return Err(format!("ledger entry '{}': unknown field '{other}'", p.id));
                }
            }
            continue;
        }
        if trimmed == ">" {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("> ") {
            p.summary_parts.push(rest.to_string());
            continue;
        }
        return Err(format!(
            "ledger entry '{}': unexpected line (want '- key: value', '> summary', or blank): {trimmed}",
            p.id
        ));
    }
    if let Some(p) = cur.take() {
        entries.push(p.finish()?);
    }
    Ok(entries)
}

/// Derive a valid kebab id from free text: lowercase, non-`[a-z0-9]` runs → single `-`, trimmed and
/// capped at 64. `None` if the result is empty or fails the shared id validator (e.g. a reserved word).
fn slugify(s: &str) -> Option<String> {
    let mut out = String::new();
    let mut prev_hyphen = false;
    for c in s.chars() {
        let lc = c.to_ascii_lowercase();
        if lc.is_ascii_lowercase() || lc.is_ascii_digit() {
            out.push(lc);
            prev_hyphen = false;
        } else if !out.is_empty() && !prev_hyphen {
            out.push('-');
            prev_hyphen = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.len() > 64 {
        out.truncate(64);
        while out.ends_with('-') {
            out.pop();
        }
    }
    if validate_id(&out).is_ok() {
        Some(out)
    } else {
        None
    }
}

// ---------- dispatch ----------

/// Entry point for `gatekeeper learn ...`. Returns the process exit code (0 / 2).
///
/// `artifacts_root` — the mutable-state root for this invocation (`docs/` in the framework repo,
/// `.claude/topology/` in a governed project); the ledger lives at
/// `<artifacts_root>/learn/ledger.md`.
///
/// `framework_root` — the payload root; used by `promote` to detect whether it is running inside
/// the framework repo (where promotion is allowed) or in a governed project (where it refuses).
pub fn cmd_learn(args: &[String], artifacts_root: &Path, framework_root: &Path) -> i32 {
    match args.first().map(String::as_str) {
        Some("--help") | Some("-h") => {
            println!("{}", crate::lookup_usage("learn"));
            0
        }
        Some("capture") => cmd_capture(&args[1..], artifacts_root),
        Some("list") => {
            if let Some(code) = crate::check_help_or_unknown(
                "learn list",
                &args[1..],
                &[],
                crate::lookup_usage("learn"),
            ) {
                return code;
            }
            cmd_list(artifacts_root)
        }
        Some("promote") => cmd_promote(&args[1..], artifacts_root, framework_root),
        _ => {
            eprintln!("gatekeeper learn: expected `capture`, `list`, or `promote`");
            2
        }
    }
}

// ---------- capture ----------

/// `YYYY-MM-DD`, checked without a date crate (shape only — the caller's `$(date +%F)` is the source).
fn is_iso_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b[..4].iter().all(u8::is_ascii_digit)
        && b[5..7].iter().all(u8::is_ascii_digit)
        && b[8..10].iter().all(u8::is_ascii_digit)
}

fn cmd_capture(args: &[String], root: &Path) -> i32 {
    if args.first().map(String::as_str) == Some("--help")
        || args.first().map(String::as_str) == Some("-h")
    {
        println!("{}", crate::lookup_usage("learn"));
        return 0;
    }
    let mut summary: Option<String> = None;
    let mut trigger = Trigger::Manual;
    let mut gate: Option<String> = None;
    let mut kind: Option<Kind> = None;
    let mut id: Option<String> = None;
    let mut date: Option<String> = None;
    let mut source: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--summary" => match args.get(i + 1) {
                Some(v) => {
                    summary = Some(v.clone());
                    i += 2;
                }
                None => return capture_usage("--summary needs a value"),
            },
            "--trigger" => match args.get(i + 1) {
                Some(v) => match Trigger::parse(v) {
                    Ok(t) => {
                        trigger = t;
                        i += 2;
                    }
                    Err(e) => return capture_usage(&e),
                },
                None => return capture_usage("--trigger needs a value"),
            },
            "--gate" => match args.get(i + 1) {
                Some(v) => {
                    gate = Some(v.clone());
                    i += 2;
                }
                None => return capture_usage("--gate needs a value"),
            },
            "--kind" => match args.get(i + 1) {
                Some(v) => match Kind::parse(v) {
                    Ok(k) => {
                        kind = Some(k);
                        i += 2;
                    }
                    Err(e) => return capture_usage(&e),
                },
                None => return capture_usage("--kind needs a value"),
            },
            "--id" => match args.get(i + 1) {
                Some(v) => {
                    id = Some(v.clone());
                    i += 2;
                }
                None => return capture_usage("--id needs a value"),
            },
            "--date" => match args.get(i + 1) {
                Some(v) => {
                    date = Some(v.clone());
                    i += 2;
                }
                None => return capture_usage("--date needs a value"),
            },
            "--source" => match args.get(i + 1) {
                Some(v) => {
                    source = Some(v.clone());
                    i += 2;
                }
                None => return capture_usage("--source needs a value"),
            },
            other => return capture_usage(&format!("unknown flag '{other}'")),
        }
    }

    let Some(summary) = summary else {
        return capture_usage("--summary <text> is required");
    };
    let summary = summary.split_whitespace().collect::<Vec<_>>().join(" ");
    if summary.is_empty() {
        return capture_usage("--summary must not be blank");
    }

    let id = match id {
        Some(v) => {
            if let Err(e) = validate_id(&v) {
                return capture_usage(&e);
            }
            v
        }
        None => match slugify(&summary) {
            Some(s) => s,
            None => {
                return capture_usage(
                    "could not derive a valid id from --summary; pass --id <kebab-slug>",
                )
            }
        },
    };

    if let Some(d) = &date {
        if !is_iso_date(d) {
            return capture_usage("--date must be YYYY-MM-DD");
        }
    }

    let block = format_entry(
        &id,
        trigger,
        gate.as_deref(),
        kind,
        date.as_deref(),
        source.as_deref(),
        &summary,
    );
    if let Err(e) = append_entry(&root.join(LEDGER_REL), &block) {
        eprintln!("gatekeeper learn capture: {e}");
        return 2;
    }
    println!("captured '{id}' → {LEDGER_REL}");
    0
}

fn capture_usage(msg: &str) -> i32 {
    eprintln!("gatekeeper learn capture: {msg}");
    2
}

/// Render one entry block exactly as the parser reads it back.
fn format_entry(
    id: &str,
    trigger: Trigger,
    gate: Option<&str>,
    kind: Option<Kind>,
    date: Option<&str>,
    source: Option<&str>,
    summary: &str,
) -> String {
    let mut s = format!("## {id}\n\n- trigger: {}\n", trigger.as_str());
    if let Some(g) = gate {
        s.push_str(&format!("- gate: {g}\n"));
    }
    if let Some(k) = kind {
        s.push_str(&format!("- kind: {}\n", k.as_str()));
    }
    if let Some(d) = date {
        s.push_str(&format!("- date: {d}\n"));
    }
    if let Some(src) = source {
        s.push_str(&format!("- source: {src}\n"));
    }
    s.push_str(&format!("\n> {summary}\n"));
    s
}

/// Append a block to the ledger, creating it (with a title) if absent and separating entries with a
/// blank line. This is the only writer; it never rewrites existing content.
fn append_entry(path: &Path, block: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
    }
    let existed = path.exists();
    let mut f = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| format!("cannot open {}: {e}", path.display()))?;
    if existed {
        f.write_all(b"\n")
            .map_err(|e| format!("write error: {e}"))?;
    } else {
        f.write_all(b"# Gotcha ledger\n\n")
            .map_err(|e| format!("write error: {e}"))?;
    }
    f.write_all(block.as_bytes())
        .map_err(|e| format!("write error: {e}"))?;
    Ok(())
}

// ---------- list ----------

fn cmd_list(root: &Path) -> i32 {
    let path = root.join(LEDGER_REL);
    let raw = match fs::read_to_string(&path) {
        Ok(r) => r,
        Err(ref e) if e.kind() == std::io::ErrorKind::NotFound => return 0,
        Err(e) => {
            eprintln!("gatekeeper learn list: cannot read {}: {e}", path.display());
            return 2;
        }
    };
    let entries = match parse_ledger(&raw) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("gatekeeper learn list: {e}");
            return 2;
        }
    };
    // Aggregate by id (BTreeMap sorts by id); count occurrences, keep the first non-empty kind seen.
    let mut by_id: BTreeMap<String, (usize, Option<Kind>)> = BTreeMap::new();
    for e in &entries {
        let slot = by_id.entry(e.id.clone()).or_insert((0, e.kind));
        slot.0 += 1;
        if slot.1.is_none() {
            slot.1 = e.kind;
        }
    }
    for (id, (n, kind)) in &by_id {
        println!("{id}\t{n}\t{}", kind.map(Kind::as_str).unwrap_or("-"));
    }
    0
}

// ---------- promote ----------

/// A validated, ready-to-write promotion. `commit` performs the only filesystem write.
#[derive(Debug)]
struct Plan {
    rel: String,
    abs: PathBuf,
    diff_from: String,
    full_content: String,
    added: String,
    needs_parent: bool,
}

impl Plan {
    fn commit(&self) -> Result<(), String> {
        if self.needs_parent {
            if let Some(parent) = self.abs.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
            }
        }
        fs::write(&self.abs, &self.full_content)
            .map_err(|e| format!("cannot write {}: {e}", self.abs.display()))
    }
}

/// Returns `true` when the caller is running inside the framework repo — i.e. when
/// `artifacts_root` equals `framework_root/docs` (the value `resolve_artifacts_root` produces
/// when `project == framework`).  Uses `canonicalize` for a reliable comparison, falling back
/// to plain path equality when either path is not yet on disk.
fn is_framework_repo(artifacts_root: &Path, framework_root: &Path) -> bool {
    let expected = framework_root.join("docs");
    match (
        std::fs::canonicalize(artifacts_root),
        std::fs::canonicalize(&expected),
    ) {
        (Ok(a), Ok(e)) => a == e,
        _ => artifacts_root == expected,
    }
}

fn cmd_promote(args: &[String], artifacts_root: &Path, framework_root: &Path) -> i32 {
    if args.first().map(String::as_str) == Some("--help")
        || args.first().map(String::as_str) == Some("-h")
    {
        println!("{}", crate::lookup_usage("learn"));
        return 0;
    }
    // ── ADR-0013 §3: promote is framework-only ─────────────────────────────────
    // The promotion targets (instincts/, skills/, security/rules.toml) live inside
    // the payload.  In a governed project the payload is replaced wholesale on
    // upgrade, so any file written there would be silently deleted.  Refuse with an
    // actionable message instead of producing silent data loss.
    if !is_framework_repo(artifacts_root, framework_root) {
        let ledger_path = artifacts_root.join(LEDGER_REL);
        eprintln!(
            "gatekeeper learn promote: this is a governed project — promote is not available here.\n\
             The gotcha is safe in the ledger at {lp}.\n\
             To promote it into a standing operator, run `learn promote` from your framework fork.\n\
             See ADR-0013 for the rationale.",
            lp = ledger_path.display()
        );
        return 2;
    }

    let root = framework_root;
    let mut id: Option<String> = None;
    let mut kind_override: Option<Kind> = None;
    let mut priority = "medium".to_string();
    let mut pattern: Option<String> = None;
    let mut rule_kind = "content".to_string();
    let mut severity = "warn".to_string();
    let mut assume_yes = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--id" => match args.get(i + 1) {
                Some(v) => {
                    id = Some(v.clone());
                    i += 2;
                }
                None => return promote_usage("--id needs a value"),
            },
            "--kind" => match args.get(i + 1) {
                Some(v) => match Kind::parse(v) {
                    Ok(k) => {
                        kind_override = Some(k);
                        i += 2;
                    }
                    Err(e) => return promote_usage(&e),
                },
                None => return promote_usage("--kind needs a value"),
            },
            "--priority" => match args.get(i + 1) {
                Some(v) => {
                    priority = v.clone();
                    i += 2;
                }
                None => return promote_usage("--priority needs a value"),
            },
            "--pattern" => match args.get(i + 1) {
                Some(v) => {
                    pattern = Some(v.clone());
                    i += 2;
                }
                None => return promote_usage("--pattern needs a value"),
            },
            "--rule-kind" => match args.get(i + 1) {
                Some(v) => {
                    rule_kind = v.clone();
                    i += 2;
                }
                None => return promote_usage("--rule-kind needs a value"),
            },
            "--severity" => match args.get(i + 1) {
                Some(v) => {
                    severity = v.clone();
                    i += 2;
                }
                None => return promote_usage("--severity needs a value"),
            },
            "--yes" => {
                assume_yes = true;
                i += 1;
            }
            other => return promote_usage(&format!("unknown flag '{other}'")),
        }
    }

    let Some(id) = id else {
        return promote_usage("--id <ledger-entry-id> is required");
    };

    let ledger_path = artifacts_root.join(LEDGER_REL);
    let raw = match fs::read_to_string(&ledger_path) {
        Ok(r) => r,
        Err(e) => {
            eprintln!(
                "gatekeeper learn promote: cannot read {}: {e}",
                ledger_path.display()
            );
            return 2;
        }
    };
    let entries = match parse_ledger(&raw) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("gatekeeper learn promote: {e}");
            return 2;
        }
    };
    let matching: Vec<&Entry> = entries.iter().filter(|e| e.id == id).collect();
    let Some(entry) = matching.last().copied() else {
        eprintln!(
            "gatekeeper learn promote: no ledger entry '{id}' in {}",
            ledger_path.display()
        );
        return 2;
    };
    let occurrences = matching.len();

    let Some(kind) = kind_override.or(entry.kind) else {
        eprintln!(
            "gatekeeper learn promote: entry '{id}' has no 'kind'; pass --kind instinct|skill|rule"
        );
        return 2;
    };

    let opts = PromoteOpts {
        priority,
        pattern,
        rule_kind,
        severity,
    };
    let plan = match build_promotion(root, &id, entry, kind, &opts) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("gatekeeper learn promote: {e}");
            return 2;
        }
    };

    // Provenance line so the approving human sees where the gotcha came from.
    let mut prov = format!("trigger {}", entry.trigger.as_str());
    if let Some(g) = &entry.gate {
        prov.push_str(&format!(", gate {g}"));
    }
    if let Some(d) = &entry.date {
        prov.push_str(&format!(", {d}"));
    }
    if let Some(src) = &entry.source {
        prov.push_str(&format!(", source {src}"));
    }
    println!("# promotion preview: {} '{}'", kind.as_str(), id);
    println!("#   from ledger entry '{id}' — {prov}, {occurrences} occurrence(s)");
    print!(
        "{}",
        render_add_diff(&plan.diff_from, &plan.rel, &plan.added)
    );

    if !assume_yes && !confirm() {
        println!("promotion aborted: no confirmation, nothing written");
        return 0;
    }

    if let Err(e) = plan.commit() {
        eprintln!("gatekeeper learn promote: {e}");
        return 2;
    }
    println!("promoted: wrote {}", plan.rel);
    0
}

fn promote_usage(msg: &str) -> i32 {
    eprintln!("gatekeeper learn promote: {msg}");
    2
}

/// Read one line of confirmation from stdin. Any answer other than `y`/`yes` (including EOF) is "no".
fn confirm() -> bool {
    eprint!("Write this operator? Type 'y' to confirm: ");
    let _ = std::io::stderr().flush();
    let mut line = String::new();
    if std::io::stdin().read_line(&mut line).is_err() {
        return false;
    }
    matches!(line.trim().to_lowercase().as_str(), "y" | "yes")
}

/// A unified-diff-style preview. Promotions are add-only (a new file, or an appended rule block), so
/// every body line is an addition — no LCS, no diff crate.
fn render_add_diff(from: &str, rel: &str, added: &str) -> String {
    let mut s = format!("--- {from}\n+++ {rel}\n");
    for line in added.lines() {
        s.push('+');
        s.push_str(line);
        s.push('\n');
    }
    s
}

/// The promotion knobs grouped into one struct, so `build_promotion` stays within clippy's argument
/// limit and the kind-specific flags travel together.
struct PromoteOpts {
    priority: String,
    pattern: Option<String>,
    rule_kind: String,
    severity: String,
}

/// Build + validate the scaffold for `kind`. Pure: the only side effect is a temp file used to validate
/// a rule against `scan::load_rules`. Returns the `Plan` the caller commits after confirmation.
fn build_promotion(
    root: &Path,
    id: &str,
    entry: &Entry,
    kind: Kind,
    opts: &PromoteOpts,
) -> Result<Plan, String> {
    match kind {
        Kind::Instinct => {
            let rel = format!("instincts/{id}.md");
            let abs = root.join(&rel);
            if abs.exists() {
                return Err(format!("{rel} already exists; refusing to overwrite"));
            }
            let content = format!(
                "---\nid: {id}\npriority: {}\nsource: ledger:{id}\n---\n{}\n",
                opts.priority, entry.summary
            );
            validate_instinct_str(&content)
                .map_err(|e| format!("scaffolded instinct is invalid: {e}"))?;
            Ok(Plan {
                rel,
                abs,
                diff_from: "/dev/null".to_string(),
                full_content: content.clone(),
                added: content,
                needs_parent: true,
            })
        }
        Kind::Skill => {
            let rel = format!("skills/{id}/SKILL.md");
            let abs = root.join(&rel);
            if abs.exists() {
                return Err(format!("{rel} already exists; refusing to overwrite"));
            }
            let summary = &entry.summary;
            let content = format!(
                "---\nname: {id}\ndescription: {summary} Use when this recurring failure is about to repeat.\n---\n\n# {id}\n\n{summary}\n\n> Scaffolded by `gatekeeper learn promote` from ledger entry `{id}`. Replace this note with the\n> procedure: name the trigger, the action to take, and the bar for done.\n"
            );
            validate_skill_str(&content)
                .map_err(|e| format!("scaffolded skill is invalid: {e}"))?;
            Ok(Plan {
                rel,
                abs,
                diff_from: "/dev/null".to_string(),
                full_content: content.clone(),
                added: content,
                needs_parent: true,
            })
        }
        Kind::Rule => {
            let Some(pattern) = opts.pattern.as_deref() else {
                return Err("promoting to a rule requires --pattern <regex> (a detection pattern cannot be inferred from prose)".to_string());
            };
            let rule_kind = opts.rule_kind.as_str();
            if rule_kind != "content" && rule_kind != "command" {
                return Err(format!(
                    "--rule-kind '{rule_kind}' (expected content|command)"
                ));
            }
            let severity = opts.severity.as_str();
            if severity != "warn" && severity != "block" {
                return Err(format!("--severity '{severity}' (expected warn|block)"));
            }
            let rel = "security/rules.toml".to_string();
            let abs = root.join(&rel);
            let existing =
                fs::read_to_string(&abs).map_err(|e| format!("cannot read {rel}: {e}"))?;
            let desc = entry.summary.replace('\\', "\\\\").replace('"', "\\\"");
            let block = format!(
                "\n[[rule]]\nid = \"{id}\"\nkind = \"{rule_kind}\"\nseverity = \"{severity}\"\ndescription = \"{desc}\"\npattern = '{pattern}'\n"
            );
            let full = format!("{existing}{block}");
            validate_rules_str(&full)?;
            Ok(Plan {
                rel,
                abs,
                diff_from: "security/rules.toml".to_string(),
                full_content: full,
                added: block.trim_start_matches('\n').to_string(),
                needs_parent: false,
            })
        }
    }
}

/// Validate a scaffolded skill by the same two fields `gatekeeper list` reads back: a non-empty `name`
/// and `description` in the frontmatter.
pub(crate) fn validate_skill_str(raw: &str) -> Result<(), String> {
    let mut in_front = false;
    let mut name = false;
    let mut desc = false;
    for line in raw.lines() {
        let t = line.trim();
        if t == "---" {
            if in_front {
                break;
            }
            in_front = true;
            continue;
        }
        if in_front {
            if let Some(v) = t.strip_prefix("name:") {
                if !v.trim().is_empty() {
                    name = true;
                }
            } else if let Some(v) = t.strip_prefix("description:") {
                if !v.trim().is_empty() {
                    desc = true;
                }
            }
        }
    }
    if !in_front {
        return Err("missing frontmatter fence".to_string());
    }
    if !name {
        return Err("frontmatter missing a non-empty 'name'".to_string());
    }
    if !desc {
        return Err("frontmatter missing a non-empty 'description'".to_string());
    }
    Ok(())
}

/// Read `path` and validate it as a skill file via [`validate_skill_str`].
/// Called by both `doctor`'s skills probe and `check docs` R1 — the shared path wrapper.
pub(crate) fn validate_skill_file(path: &std::path::Path) -> Result<(), String> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    validate_skill_str(&raw)
}

/// Distinguishes concurrent rule-validation temp files (cargo runs unit tests on parallel threads).
static RULECHECK_SEQ: AtomicU64 = AtomicU64::new(0);

/// Validate a candidate `rules.toml` body by loading it through `scan::load_rules` — the exact loader
/// the scanner uses — via a temp file (so no `scan.rs` change is needed). Guarantees the promoted rule
/// "passes scan-load": id-unique, pattern compiles, schema valid.
fn validate_rules_str(raw: &str) -> Result<(), String> {
    let seq = RULECHECK_SEQ.fetch_add(1, Ordering::Relaxed);
    let tmp = std::env::temp_dir().join(format!(
        "topo_learn_rulecheck_{}_{seq}.toml",
        std::process::id()
    ));
    fs::write(&tmp, raw).map_err(|e| format!("cannot stage rule validation: {e}"))?;
    let res = scan::load_rules(&tmp).map(|_| ());
    let _ = fs::remove_file(&tmp);
    res.map_err(|e| format!("scaffolded rule does not load under scan: {e}"))
}

// ---------- tests ----------

#[cfg(test)]
mod parse_tests {
    use super::*;

    const LEDGER: &str = "# Gotcha ledger\n\n## verify-skipped\n\n- trigger: gate-failure\n- gate: verify\n- kind: instinct\n- date: 2026-06-08\n\n> Unit tests passing is not the verify gate.\n\n## rm-in-script\n\n- trigger: human-correction\n\n> A script deleted a path without a guard.\n";

    #[test]
    fn parses_two_entries_with_fields() {
        let e = parse_ledger(LEDGER).unwrap();
        assert_eq!(e.len(), 2);
        assert_eq!(e[0].id, "verify-skipped");
        assert_eq!(e[0].trigger, Trigger::GateFailure);
        assert_eq!(e[0].gate.as_deref(), Some("verify"));
        assert_eq!(e[0].kind, Some(Kind::Instinct));
        assert_eq!(e[0].date.as_deref(), Some("2026-06-08"));
        assert!(e[0].summary.starts_with("Unit tests"));
        assert_eq!(e[1].id, "rm-in-script");
        assert_eq!(e[1].trigger, Trigger::HumanCorrection);
        assert_eq!(e[1].kind, None);
    }
    #[test]
    fn recurrence_keeps_each_occurrence() {
        let dup = "## dup\n\n- trigger: stop\n\n> first time\n\n## dup\n\n- trigger: stop\n\n> second time\n";
        let e = parse_ledger(dup).unwrap();
        assert_eq!(e.len(), 2);
        assert!(e.iter().all(|x| x.id == "dup"));
    }
    #[test]
    fn unknown_field_rejected() {
        let err = parse_ledger("## x\n\n- bogus: 1\n\n> body\n").unwrap_err();
        assert!(err.contains("unknown field 'bogus'"), "{err}");
    }
    #[test]
    fn empty_summary_rejected() {
        assert!(parse_ledger("## x\n\n- trigger: stop\n").is_err());
    }
    #[test]
    fn preamble_before_first_entry_ignored() {
        let s = "# Gotcha ledger\n\nSome intro paragraph.\n\n## only\n\n> just one\n";
        let e = parse_ledger(s).unwrap();
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].id, "only");
    }
    #[test]
    fn bad_id_in_heading_rejected() {
        assert!(parse_ledger("## Bad_Id\n\n> body\n").is_err());
    }
}

#[cfg(test)]
mod slug_tests {
    use super::*;

    #[test]
    fn slugifies_a_sentence() {
        assert_eq!(
            slugify("Verify skipped on green tests").as_deref(),
            Some("verify-skipped-on-green-tests")
        );
    }
    #[test]
    fn collapses_punctuation_and_trims() {
        assert_eq!(
            slugify("  rm -rf!! in a script??").as_deref(),
            Some("rm-rf-in-a-script")
        );
    }
    #[test]
    fn all_symbols_is_none() {
        assert_eq!(slugify("@#$%"), None);
    }
    #[test]
    fn reserved_word_is_none() {
        assert_eq!(slugify("ask Claude first"), None);
    }
}

#[cfg(test)]
mod scaffold_tests {
    use super::*;

    fn entry(id: &str, summary: &str) -> Entry {
        Entry {
            id: id.to_string(),
            trigger: Trigger::GateFailure,
            gate: Some("verify".to_string()),
            kind: None,
            date: Some("2026-06-08".to_string()),
            source: None,
            summary: summary.to_string(),
        }
    }
    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("topo_learn_scaf_{tag}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }
    fn opts(priority: &str, pattern: Option<&str>) -> PromoteOpts {
        PromoteOpts {
            priority: priority.to_string(),
            pattern: pattern.map(str::to_string),
            rule_kind: "content".to_string(),
            severity: "warn".to_string(),
        }
    }

    #[test]
    fn instinct_scaffold_is_valid_and_backlinks() {
        let root = scratch("inst");
        let e = entry(
            "verify-skipped",
            "Unit tests are not the verify gate; record a re-runnable end-to-end command.",
        );
        let plan = build_promotion(
            &root,
            "verify-skipped",
            &e,
            Kind::Instinct,
            &opts("high", None),
        )
        .unwrap();
        assert_eq!(plan.rel, "instincts/verify-skipped.md");
        assert!(plan.added.contains("source: ledger:verify-skipped"));
        assert!(plan.added.contains("priority: high"));
        validate_instinct_str(&plan.full_content).unwrap();
        let _ = fs::remove_dir_all(&root);
    }
    #[test]
    fn rule_scaffold_loads_and_bad_pattern_rejected() {
        let root = scratch("rule");
        fs::create_dir_all(root.join("security")).unwrap();
        fs::write(root.join("security/rules.toml"), "schema_version = 1\n").unwrap();
        let e = entry("leaky-token", "A FIXME-SECRET marker keeps slipping in.");
        let ok = build_promotion(
            &root,
            "leaky-token",
            &e,
            Kind::Rule,
            &opts("medium", Some(r"\bFIXME-SECRET\b")),
        )
        .unwrap();
        assert!(ok.added.contains("id = \"leaky-token\""));
        assert!(ok.added.contains("severity = \"warn\""));
        let bad = build_promotion(
            &root,
            "leaky-token",
            &e,
            Kind::Rule,
            &opts("medium", Some("(unclosed")),
        );
        assert!(
            bad.is_err(),
            "an uncompilable pattern must be rejected by scan-load"
        );
        let _ = fs::remove_dir_all(&root);
    }
    #[test]
    fn rule_without_pattern_errors() {
        let root = scratch("nopat");
        fs::create_dir_all(root.join("security")).unwrap();
        fs::write(root.join("security/rules.toml"), "schema_version = 1\n").unwrap();
        let e = entry("x", "some lesson");
        let err = build_promotion(&root, "x", &e, Kind::Rule, &opts("medium", None)).unwrap_err();
        assert!(err.contains("requires --pattern"), "{err}");
        let _ = fs::remove_dir_all(&root);
    }
    #[test]
    fn skill_scaffold_has_name_and_description() {
        let root = scratch("skill");
        let e = entry("flaky-thing", "Some recurring flakiness lesson.");
        let plan =
            build_promotion(&root, "flaky-thing", &e, Kind::Skill, &opts("medium", None)).unwrap();
        assert_eq!(plan.rel, "skills/flaky-thing/SKILL.md");
        assert!(plan.added.contains("name: flaky-thing"));
        validate_skill_str(&plan.full_content).unwrap();
        let _ = fs::remove_dir_all(&root);
    }
}
