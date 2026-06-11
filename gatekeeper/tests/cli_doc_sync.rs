//! Integration test: README / USER-GUIDE ↔ `gatekeeper --help` sync (FM3).
//!
//! Parsing scope (tight, per spec §6):
//!   (a) USER-GUIDE.md §"## Command reference" — heading-delimited (from the heading to the
//!       next `## ` heading); fenced code blocks excluded.
//!   (b) README.md gate table — the markdown table whose header row contains `Gate`; only
//!       that table, no surrounding prose.
//!
//! Normalisation: `<placeholder>` and `[optional]` tokens are removed before comparison;
//! a literal `--` separator token is kept (spec §6) but treated as compatible whether
//! it appears as required or optional `[--]`; comparison is on subcommand words +
//! required flag spellings (tokens starting with `--` that are NOT wrapped in `[…]`).
//!
//! Assertions:
//!   1. Every help command appears in USER-GUIDE §"## Command reference".
//!   2. Every in-scope documented command parses against the help table (no ghost commands).
//!   3. Required flag spellings match between docs and help.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::Command;

// ── path helpers ─────────────────────────────────────────────────────────────

fn repo_root() -> PathBuf {
    // CARGO_MANIFEST_DIR is gatekeeper/ — one level up is the repo root.
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .expect("CARGO_MANIFEST_DIR has a parent")
        .to_path_buf()
}

fn readme_path() -> PathBuf {
    repo_root().join("README.md")
}

fn user_guide_path() -> PathBuf {
    repo_root().join("docs").join("USER-GUIDE.md")
}

// ── help output parsing ───────────────────────────────────────────────────────

/// Run `gatekeeper --help` and return its stdout.
fn help_output() -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_gatekeeper"))
        .arg("--help")
        .output()
        .expect("failed to spawn gatekeeper --help");
    assert_eq!(
        out.status.code(),
        Some(0),
        "gatekeeper --help must exit 0; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Parse the help output into a list of normalised command signatures.
///
/// Each USAGE line looks like `  gatekeeper <words...>`.  Multiple usage lines
/// for the same top-level subcommand (e.g. two `scan` lines) are each
/// represented as a separate NormCmd.
fn parse_help_commands(help: &str) -> Vec<NormCmd> {
    let mut cmds = Vec::new();
    for line in help.lines() {
        let trimmed = line.trim_start();
        if !trimmed.starts_with("gatekeeper ") {
            continue;
        }
        if trimmed == "gatekeeper" {
            continue;
        }
        if let Some(nc) = NormCmd::from_str(trimmed) {
            cmds.push(nc);
        }
    }
    cmds
}

// ── doc section parsing ───────────────────────────────────────────────────────

/// Extract the text of the `## Command reference` section of USER-GUIDE.md,
/// excluding fenced code blocks.
fn user_guide_command_reference(content: &str) -> String {
    let mut in_section = false;
    let mut in_fence = false;
    let mut out = String::new();

    for line in content.lines() {
        if line.starts_with("## ") {
            if line == "## Command reference" {
                in_section = true;
                in_fence = false;
                continue;
            } else if in_section {
                break;
            } else {
                continue;
            }
        }
        if !in_section {
            continue;
        }
        if line.starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// Extract the gate table rows from README.md — the table whose header row contains `Gate`.
/// Returns body rows (excludes header and separator).
fn readme_gate_table(content: &str) -> Vec<String> {
    let mut rows = Vec::new();
    let mut in_table = false;
    let mut header_seen = false;
    let mut sep_seen = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with('|') {
            if in_table {
                break;
            }
            continue;
        }
        if !in_table {
            if trimmed.contains("Gate") {
                in_table = true;
                header_seen = true;
                continue;
            }
            continue;
        }
        if header_seen && !sep_seen {
            sep_seen = true;
            continue;
        }
        rows.push(trimmed.to_string());
    }
    rows
}

/// Extract all backtick-quoted `gatekeeper …` spans from a text string.
/// The caller is responsible for excluding fenced code blocks before passing text in.
fn extract_backtick_commands(text: &str) -> Vec<String> {
    let mut cmds = Vec::new();
    let mut remaining = text;
    while let Some(start) = remaining.find('`') {
        remaining = &remaining[start + 1..];
        let end = match remaining.find('`') {
            Some(e) => e,
            None => break,
        };
        let span = &remaining[..end];
        remaining = &remaining[end + 1..];
        if span.starts_with("gatekeeper ") {
            cmds.push(span.to_string());
        }
    }
    cmds
}

// ── normalised command representation ────────────────────────────────────────

/// A normalised command: subcommand key (the non-flag leading words) and
/// required flags (tokens that start with `--` and are NOT wrapped in `[…]`).
///
/// The `has_separator` field records whether a bare `--` separator was present
/// (required or optional).  Subcommand words do NOT include `--` — the separator
/// is stored separately so that `check finish -- <cmd>` (help) and
/// `check finish [-- <cmd>]` (USER-GUIDE optional form) compare as the same command.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct NormCmd {
    /// Leading subcommand words (e.g. `["check", "design"]`).
    subcommand: Vec<String>,
    /// Required flag names (e.g. `["--feature"]`), sorted.
    required_flags: BTreeSet<String>,
    /// True if a `--` separator appeared (required or optional).
    has_separator: bool,
    /// Raw source text for error messages.
    raw: String,
}

impl NormCmd {
    /// Parse a raw `gatekeeper …` string into a NormCmd.
    ///
    /// Normalisation rules:
    /// - Strip the leading `gatekeeper` word.
    /// - Track `[…]` bracket depth; tokens inside brackets are optional.
    /// - `<placeholder>` tokens mark end of subcommand but are discarded.
    /// - A bare `--` separator is recorded in `has_separator` (NOT added to `subcommand`),
    ///   whether required or optional — so `check finish -- <cmd>` and
    ///   `check finish [-- <cmd>]` normalise to the same subcommand key.
    /// - `--flag` tokens OUTSIDE brackets are required flags.
    /// - `--flag` tokens INSIDE brackets are optional and discarded.
    /// - Words before any `--flag` or `<placeholder>` are subcommand words.
    /// - Prose `(text...)` terminates parsing.
    ///
    /// Returns None if the string doesn't start with `gatekeeper `.
    fn from_str(raw: &str) -> Option<Self> {
        let s = raw.trim();
        let rest = s.strip_prefix("gatekeeper ")?;

        let tokens: Vec<&str> = rest.split_whitespace().collect();

        let mut subcommand: Vec<String> = Vec::new();
        let mut required_flags: BTreeSet<String> = BTreeSet::new();
        let mut past_subcmd = false;
        let mut has_separator = false;
        let mut bracket_depth: i32 = 0;

        for token in &tokens {
            if token.starts_with('(') {
                break; // Prose annotation — stop.
            }

            // Count bracket characters to track optional scope.
            let opens: i32 = token.chars().filter(|&c| c == '[').count() as i32;
            let closes: i32 = token.chars().filter(|&c| c == ']').count() as i32;

            let depth_before = bracket_depth;
            let depth_after = bracket_depth + opens - closes;
            // Token is optional if it opens a bracket OR we were already inside one.
            let is_optional = depth_before > 0 || opens > 0;
            bracket_depth = depth_after.max(0);

            // Strip surrounding `[` / `]` to get the content.
            let core = token.trim_matches(|c| c == '[' || c == ']');
            if core.is_empty() {
                continue;
            }

            // Placeholder like `<slug>` or `<ref>`.
            if core.starts_with('<') && core.ends_with('>') {
                past_subcmd = true;
                continue;
            }

            if core == "--" {
                // Bare `--` separator — record but do NOT add to subcommand.
                has_separator = true;
                past_subcmd = true;
                continue;
            }

            if core.starts_with("--") {
                past_subcmd = true;
                // Strip trailing `|` or `,` from alternatives like `--hook |`.
                let flag = core.trim_end_matches(['|', ',']);
                if !is_optional {
                    required_flags.insert(flag.to_string());
                }
                continue;
            }

            // Regular subcommand word.
            if !past_subcmd && !is_optional {
                subcommand.push(token.to_string());
            }
        }

        if subcommand.is_empty() {
            return None;
        }
        Some(NormCmd {
            subcommand,
            required_flags,
            has_separator,
            raw: raw.to_string(),
        })
    }

    /// Human-readable signature for error messages.
    fn display(&self) -> String {
        let mut parts = self.subcommand.clone();
        if self.has_separator {
            parts.push("--".to_string());
        }
        let mut flags: Vec<&String> = self.required_flags.iter().collect();
        flags.sort();
        for f in flags {
            parts.push(f.clone());
        }
        format!("gatekeeper {}", parts.join(" "))
    }
}

/// Return true if `doc_cmd` is covered by at least one entry in `help_cmds`.
///
/// Matching semantics:
/// - Subcommand words must match exactly.
/// - Every required flag in `doc_cmd` must appear in the help entry.
/// - Doc entries with no required flags match if any help entry shares the
///   same subcommand (group-level / section-header mentions).
fn covered_by_help(doc_cmd: &NormCmd, help_cmds: &[NormCmd]) -> bool {
    if doc_cmd.required_flags.is_empty() {
        // Group mention: accept if any help entry starts with or equals this subcommand.
        return help_cmds.iter().any(|h| {
            h.subcommand == doc_cmd.subcommand || h.subcommand.starts_with(&doc_cmd.subcommand)
        });
    }
    help_cmds.iter().any(|h| {
        h.subcommand == doc_cmd.subcommand
            && doc_cmd
                .required_flags
                .iter()
                .all(|f| h.required_flags.contains(f))
    })
}

/// Return true if `help_cmd` is documented in `doc_cmds` (USER-GUIDE reference section).
///
/// Matching semantics:
/// - Exact match: same subcommand AND all help required flags appear in a doc entry.
/// - Split-flag match: each individual required flag of the help command has at least
///   one doc entry for the same subcommand (handles `scan` whose two help lines have
///   multiple flags each, while docs have one entry per flag variant).
/// - Group-level match: a doc entry with the same subcommand but no required flags
///   counts as documenting the command family.
fn documented_in_ug(help_cmd: &NormCmd, doc_cmds: &[NormCmd]) -> bool {
    // Exact full-match.
    if doc_cmds.iter().any(|d| {
        d.subcommand == help_cmd.subcommand
            && help_cmd
                .required_flags
                .iter()
                .all(|f| d.required_flags.contains(f))
    }) {
        return true;
    }

    // Split-flag match (e.g. scan): every required flag individually documented.
    if !help_cmd.required_flags.is_empty() {
        let all_covered = help_cmd.required_flags.iter().all(|f| {
            doc_cmds
                .iter()
                .any(|d| d.subcommand == help_cmd.subcommand && d.required_flags.contains(f))
        });
        if all_covered {
            return true;
        }
    }

    // Group-level doc entry.
    doc_cmds
        .iter()
        .any(|d| d.subcommand == help_cmd.subcommand && d.required_flags.is_empty())
}

// ── test ─────────────────────────────────────────────────────────────────────

#[test]
fn help_and_docs_are_in_sync() {
    let help = help_output();
    let help_cmds = parse_help_commands(&help);
    assert!(
        !help_cmds.is_empty(),
        "No commands parsed from --help output; raw output:\n{help}"
    );

    // Parse USER-GUIDE Command reference section (fenced blocks excluded).
    let ug_content =
        std::fs::read_to_string(user_guide_path()).expect("could not read docs/USER-GUIDE.md");
    let ug_section = user_guide_command_reference(&ug_content);
    let ug_raw: Vec<String> = extract_backtick_commands(&ug_section);
    let ug_cmds: Vec<NormCmd> = ug_raw.iter().filter_map(|s| NormCmd::from_str(s)).collect();

    // Parse README gate table.
    let readme_content = std::fs::read_to_string(readme_path()).expect("could not read README.md");
    let readme_rows = readme_gate_table(&readme_content);
    let readme_raw: Vec<String> = readme_rows
        .iter()
        .flat_map(|row| extract_backtick_commands(row))
        .collect();
    let readme_cmds: Vec<NormCmd> = readme_raw
        .iter()
        .filter_map(|s| NormCmd::from_str(s))
        .collect();

    // Combined doc commands from both sources.
    let all_doc_cmds: Vec<NormCmd> = {
        let mut v = ug_cmds.clone();
        v.extend(readme_cmds.clone());
        v
    };

    // ── Assertion 1: every help command is covered in USER-GUIDE Command reference ─
    let mut missing_in_ug: Vec<String> = Vec::new();
    for hc in &help_cmds {
        if !documented_in_ug(hc, &ug_cmds) {
            missing_in_ug.push(format!("  {} (from help line: {:?})", hc.display(), hc.raw));
        }
    }

    // ── Assertion 2: every in-scope doc command parses against the help table ──────
    let mut ghost_cmds: Vec<String> = Vec::new();
    for dc in &all_doc_cmds {
        if !covered_by_help(dc, &help_cmds) {
            ghost_cmds.push(format!("  {} (raw: {:?})", dc.display(), dc.raw));
        }
    }

    // ── Assertion 3: required flag spellings in docs must exist in help ───────────
    // This is already enforced by assertions 1 & 2, but we emit targeted diagnostics.
    let mut flag_mismatches: Vec<String> = Vec::new();
    for dc in &all_doc_cmds {
        for f in &dc.required_flags {
            let found = help_cmds
                .iter()
                .any(|h| h.subcommand == dc.subcommand && h.required_flags.contains(f));
            if !found {
                flag_mismatches.push(format!(
                    "  doc '{}' has flag '{}' not found in any help entry for subcommand {:?}",
                    dc.display(),
                    f,
                    dc.subcommand
                ));
            }
        }
    }

    // ── Collect failures and emit a single actionable message ─────────────────────
    let mut failures: Vec<String> = Vec::new();

    if !missing_in_ug.is_empty() {
        failures.push(format!(
            "Assertion 1 FAILED — {} help command(s) missing from USER-GUIDE \
             §'## Command reference':\n{}",
            missing_in_ug.len(),
            missing_in_ug.join("\n")
        ));
    }

    if !ghost_cmds.is_empty() {
        failures.push(format!(
            "Assertion 2 FAILED — {} in-scope doc command(s) not found in help \
             table (ghost commands):\n{}",
            ghost_cmds.len(),
            ghost_cmds.join("\n")
        ));
    }

    if !flag_mismatches.is_empty() {
        failures.push(format!(
            "Assertion 3 FAILED — flag spelling mismatches (doc flag not in help):\n{}",
            flag_mismatches.join("\n")
        ));
    }

    assert!(
        failures.is_empty(),
        "doc/binary sync failures detected:\n\n{}\n\n\
         --- Parsed help commands ---\n{}\n\n\
         --- USER-GUIDE in-scope commands ---\n{}\n\n\
         --- README gate-table commands ---\n{}",
        failures.join("\n\n"),
        help_cmds
            .iter()
            .map(|c| format!("  {}", c.display()))
            .collect::<Vec<_>>()
            .join("\n"),
        ug_cmds
            .iter()
            .map(|c| format!("  {}", c.display()))
            .collect::<Vec<_>>()
            .join("\n"),
        readme_cmds
            .iter()
            .map(|c| format!("  {}", c.display()))
            .collect::<Vec<_>>()
            .join("\n"),
    );
}
