//! Topology gatekeeper — routes skills and enforces methodology gates.
//!
//! Subcommands:
//!   gatekeeper list                         List skills + descriptions.
//!   gatekeeper activate                     Read a prompt on stdin, print routed skills.
//!   gatekeeper check research --feature S   Research gate: a research note exists.
//!   gatekeeper check design  --feature S    Design gate: research note exists, then a spec doc exists.
//!   gatekeeper check plan    --feature S    Plan gate: a placeholder-free plan exists.
//!   gatekeeper check verify  --feature S    Verify gate: a verification note exists.
//!   gatekeeper check tdd     --feature S [--base R]  TDD gate: failing-test-first history heuristic.
//!   gatekeeper check review  --feature S [--base R]  Review gate: a fresh critic's artifact passes.
//!   gatekeeper check finish  -- <cmd...>    Finish gate: <cmd> exits 0.
//!   gatekeeper scan --hook                  Security-scan a PreToolUse event (stdin); emit the decision.
//!   gatekeeper scan --cmd | --content       Security-scan a command / file image on stdin.
//!   gatekeeper scan --staged                Pre-commit: scan staged blobs + enforce integrity.
//!   gatekeeper scan --check-path <path>     Exit 1 iff <path> is a protected safety file.
//!   gatekeeper instinct list                List always-on instincts (id + priority).
//!   gatekeeper instinct render [--harness H] [--budget N]   Render the always-on preamble subset.
//!   gatekeeper adapt --harness <h> [--check]   Generate harness <h>'s native config from the source.
//!   gatekeeper learn capture --summary <s>  Append a structured gotcha to <artifacts_root>/learn/ledger.md.
//!   gatekeeper learn list                   List ledger entries (id + occurrences + proposed kind).
//!   gatekeeper learn promote --id <id>      Scaffold an operator from a gotcha; diff + confirm to write.
//!   gatekeeper memory write --feature <slug> --date <YYYY-MM-DD>   Write a handoff artifact (body on stdin).
//!   gatekeeper memory read  --feature <slug>                       Print a handoff artifact to stdout.
//!   gatekeeper memory list                                         List all handoff artifacts (slug · created · status).
//!   gatekeeper check docs                   Docs-coverage lint: skills frontmatter, ADR index, ROADMAP evidence paths.
//!   gatekeeper doctor                       Read-only health check + binary-resolution transparency.
//!
//! Built offline from a small, vetted dependency set (regex, serde, serde_json, toml); ships as
//! a single std-only macOS-arm64 executable (dynamically links libSystem). See
//! docs/adr/0007-security-scanner-dependencies.md.
//! See docs/adr/0007-security-scanner-dependencies.md.

use std::env;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{exit, Command};

use regex::Regex;

mod adapt;
mod config;
mod doctor;
mod instinct;
mod learn;
mod memory;
mod review;
mod scan;
mod tdd;
mod verify;
mod version;

const PLACEHOLDERS: &[&str] = &[
    "tbd",
    "implement later",
    "similar to task",
    "appropriate validation",
    "to be determined",
    "fill in later",
];

// ── dispatch table ────────────────────────────────────────────────────────────

/// A single entry in the static dispatch table.
///
/// `name` may be a two-word key (e.g. `"check verify"`) for per-gate entries, or a single
/// word for top-level commands.  Dispatch uses longest-prefix match: a two-word key wins
/// over a one-word prefix.
pub(crate) struct SubcommandSpec {
    pub(crate) name: &'static str,
    pub(crate) usage: &'static str,
    #[allow(dead_code)]
    pub(crate) synopsis: &'static str,
    #[allow(dead_code)]
    pub(crate) known_flags: &'static [&'static str],
    pub(crate) handler: fn(&[String]) -> i32,
}

pub(crate) static SUBCOMMANDS: &[SubcommandSpec] = &[
    SubcommandSpec {
        name: "list",
        usage: "USAGE:\n  gatekeeper list",
        synopsis: "List skills + descriptions.",
        known_flags: &[],
        handler: |args| cmd_list(args),
    },
    SubcommandSpec {
        name: "activate",
        usage: "USAGE:\n  gatekeeper activate            (reads prompt on stdin)",
        synopsis: "Read a prompt on stdin, print routed skills.",
        known_flags: &[],
        handler: |args| cmd_activate(args),
    },
    SubcommandSpec {
        name: "check research",
        usage: "USAGE:\n  gatekeeper check research --feature <slug>",
        synopsis: "Research gate: a research note exists.",
        known_flags: &["--feature"],
        handler: |args| handle_check_research(args),
    },
    SubcommandSpec {
        name: "check design",
        usage: "USAGE:\n  gatekeeper check design --feature <slug>",
        synopsis: "Design gate: research note exists, then a spec doc exists.",
        known_flags: &["--feature"],
        handler: |args| handle_check_design(args),
    },
    SubcommandSpec {
        name: "check plan",
        usage: "USAGE:\n  gatekeeper check plan --feature <slug>",
        synopsis: "Plan gate: a placeholder-free plan exists.",
        known_flags: &["--feature"],
        handler: |args| handle_check_plan(args),
    },
    SubcommandSpec {
        name: "check verify",
        usage: "USAGE:\n  gatekeeper check verify --feature <slug>",
        synopsis: "Verify gate: a verification note exists.",
        known_flags: &["--feature"],
        handler: |args| handle_check_verify(args),
    },
    SubcommandSpec {
        name: "check tdd",
        usage: "USAGE:\n  gatekeeper check tdd --feature <slug> [--base <ref>]",
        synopsis: "TDD gate: failing-test-first history heuristic.",
        known_flags: &["--feature", "--base"],
        handler: |args| handle_check_tdd(args),
    },
    SubcommandSpec {
        name: "check review",
        usage: "USAGE:\n  gatekeeper check review --feature <slug> [--base <ref>]",
        synopsis: "Review gate: a fresh critic's artifact passes.",
        known_flags: &["--feature", "--base"],
        handler: |args| handle_check_review(args),
    },
    SubcommandSpec {
        name: "check finish",
        usage: "USAGE:\n  gatekeeper check finish -- <command...>",
        synopsis: "Finish gate: <cmd> exits 0.",
        known_flags: &["--"],
        handler: |args| handle_check_finish(args),
    },
    SubcommandSpec {
        name: "check docs",
        usage: "USAGE:\n  gatekeeper check docs",
        synopsis: "Docs-coverage lint: skills frontmatter, ADR index, ROADMAP evidence paths.",
        known_flags: &[],
        handler: |args| handle_check_docs(args),
    },
    SubcommandSpec {
        name: "scan",
        usage: "USAGE:\n  gatekeeper scan --hook | --cmd | --content       (reads stdin)\n  gatekeeper scan --staged | --check-path <path>",
        synopsis: "Security-scan a hook event, command, file, or staged blobs.",
        known_flags: &["--hook", "--cmd", "--content", "--staged", "--check-path"],
        handler: |args| {
            scan::cmd_scan(args, &framework_root(), &artifacts_root(), &project_root())
        },
    },
    SubcommandSpec {
        name: "instinct",
        usage: "USAGE:\n  gatekeeper instinct list\n  gatekeeper instinct render [--harness <h>] [--budget <n>]",
        synopsis: "List or render always-on instincts.",
        known_flags: &["--harness", "--budget"],
        handler: |args| instinct::cmd_instinct(args, &framework_root()),
    },
    SubcommandSpec {
        name: "adapt",
        usage: "USAGE:\n  gatekeeper adapt --harness <codex|cursor|opencode|claude> [--check]",
        synopsis: "Generate harness native config from the source.",
        known_flags: &["--harness", "--check"],
        handler: |args| adapt::cmd_adapt(args, &framework_root(), &project_root()),
    },
    SubcommandSpec {
        name: "learn",
        usage: "USAGE:\n  gatekeeper learn capture --summary <text> [--trigger <t>] [--gate <g>] [--kind <k>]\n  gatekeeper learn list\n  gatekeeper learn promote --id <id> [--kind <k>] [--yes]",
        synopsis: "Capture, list, or promote structured gotchas.",
        known_flags: &["--summary", "--trigger", "--gate", "--kind", "--id", "--yes"],
        handler: |args| learn::cmd_learn(args, &artifacts_root(), &framework_root()),
    },
    SubcommandSpec {
        name: "memory",
        usage: "USAGE:\n  gatekeeper memory write --feature <slug> --date <YYYY-MM-DD>  (reads body on stdin)\n  gatekeeper memory read  --feature <slug>\n  gatekeeper memory list",
        synopsis: "Write, read, or list handoff artifacts.",
        known_flags: &["--feature", "--date", "--status", "--verified-by"],
        handler: |args| {
            memory::cmd_memory(args, &artifacts_root(), &framework_root(), &project_root())
        },
    },
    SubcommandSpec {
        name: "doctor",
        usage: "USAGE:\n  gatekeeper doctor",
        synopsis: "Read-only health check + binary-resolution transparency.",
        known_flags: &[],
        handler: |args| handle_doctor(args),
    },
];

/// Look up a subcommand's usage string by name.  Returns an empty string if not found
/// (should never happen for the hard-coded names used by the modules below).
pub(crate) fn lookup_usage(name: &str) -> &'static str {
    SUBCOMMANDS
        .iter()
        .find(|s| s.name == name)
        .map(|s| s.usage)
        .unwrap_or("")
}

/// Build the group-level usage block for `check` (all check/* rows).
fn check_group_usage() -> String {
    let mut lines = String::from("USAGE:");
    for s in SUBCOMMANDS {
        if s.name.starts_with("check ") {
            // strip the leading "USAGE:\n  " from each entry's usage
            let body = s.usage.strip_prefix("USAGE:\n  ").unwrap_or(s.usage);
            lines.push_str("\n  ");
            lines.push_str(body);
        }
    }
    lines
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let code = dispatch(&args);
    exit(code);
}

fn dispatch(args: &[String]) -> i32 {
    // Special top-level flags handled before the table.
    match args.first().map(String::as_str) {
        Some("--version") | Some("-V") => {
            println!(
                "gatekeeper {} (rules schema v{})",
                version::tool(),
                version::rules_schema()
            );
            return 0;
        }
        Some("--help") | Some("-h") | None => {
            print_help();
            return 0;
        }
        _ => {}
    }

    // Longest-prefix match: try two-word key first, then one-word.
    let two_word: Option<&str> = args.first().zip(args.get(1)).and_then(|(a, b)| {
        let key = format!("{a} {b}");
        SUBCOMMANDS
            .iter()
            .find(|s| s.name == key.as_str())
            .map(|s| s.name)
    });

    if let Some(two) = two_word {
        // args[0] is the first word, args[1] is the second word.
        // Pass args[2..] to the handler.
        let spec = SUBCOMMANDS.iter().find(|s| s.name == two).unwrap();
        return (spec.handler)(&args[2..]);
    }

    // One-word match.
    let first = args[0].as_str();

    // Special group: bare `check`, `check --help/-h`, `check <unknown>`
    if first == "check" {
        return dispatch_check(&args[1..]);
    }

    if let Some(spec) = SUBCOMMANDS.iter().find(|s| s.name == first) {
        return (spec.handler)(&args[1..]);
    }

    // Unknown top-level command.
    eprintln!("gatekeeper: unknown command '{first}'\n");
    print_help();
    2
}

fn print_help() {
    print!(
        "topology gatekeeper {} (rules schema v{})\n\nUSAGE:\n",
        version::tool(),
        version::rules_schema()
    );
    for spec in SUBCOMMANDS {
        // Each spec.usage is "USAGE:\n  <lines...>"; strip the header and indent each line.
        let body = spec.usage.strip_prefix("USAGE:\n").unwrap_or(spec.usage);
        for line in body.lines() {
            println!("{line}");
        }
    }
    println!();
}

const ROOT_MARKERS: &[&str] = &["AGENTS.md", "gatekeeper", ".claude-plugin"];

pub(crate) fn is_marked_root(dir: &Path) -> bool {
    dir.join("skills").is_dir() && ROOT_MARKERS.iter().any(|m| dir.join(m).exists())
}

/// Which resolution step produced the `ResolvedRoot`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum RootSource {
    /// Step 1: `$TOPOLOGY_ROOT` was set and points at an existing directory.
    EnvOverride,
    /// Step 2: the project root (`.git` walk) is itself a marked root.
    SelfGoverned,
    /// Step 3: walking up from `current_exe()` found a marked ancestor.
    BinaryAdjacent,
    /// Step 4: `<project_root>/.topology` is a marked root.
    ProjectVendored,
    /// Step 5: `<home>/.topology` is a marked root.
    GlobalHome,
    /// Step 6: no deterministic source found; `start` (cwd) returned unchanged.
    Fallback,
}

/// A resolved framework root with provenance.
#[derive(Debug, Clone)]
pub(crate) struct ResolvedRoot {
    pub(crate) path: PathBuf,
    pub(crate) source: RootSource,
}

/// Pure resolution function — all inputs are arguments, no process state is read.
///
/// Precedence (see spec §"Resolution algorithm"):
///   1. `env_override` — if set and an existing directory, return verbatim.
///   2. `project_root` is itself a marked root (`SelfGoverned`).
///   3. Walk up from `exe_path`; first marked ancestor wins (`BinaryAdjacent`).
///   4. `<project_root>/.topology` is a marked root (`ProjectVendored`).
///   5. `<home>/.topology` is a marked root (`GlobalHome`).
///   6. Fallback: return `start` unchanged.
///
/// The bare cwd marker walk from the old algorithm is **removed**. The only cwd
/// influence is `project_root` (the nearest-`.git` walk, passed in as an argument)
/// and the identity fallback.
pub(crate) fn resolve_root(
    start: &Path,
    env_override: Option<&Path>,
    exe_path: Option<&Path>,
    home: Option<&Path>,
) -> ResolvedRoot {
    // Step 1: explicit env pin.
    if let Some(o) = env_override {
        if o.is_dir() {
            return ResolvedRoot {
                path: o.to_path_buf(),
                source: RootSource::EnvOverride,
            };
        }
    }

    // Step 2: self-governed — the project root is itself a marked root.
    let proj = resolve_project_root(start);
    if is_marked_root(&proj) {
        return ResolvedRoot {
            path: proj,
            source: RootSource::SelfGoverned,
        };
    }

    // Step 3: binary-adjacent — walk up from the exe's directory.
    if let Some(exe) = exe_path {
        let mut dir = exe.parent().map(|p| p.to_path_buf());
        while let Some(d) = dir {
            if is_marked_root(&d) {
                return ResolvedRoot {
                    path: d,
                    source: RootSource::BinaryAdjacent,
                };
            }
            dir = d.parent().map(|p| p.to_path_buf());
        }
    }

    // Step 4: <project_root>/.topology vendored install.
    let vendored = proj.join(".topology");
    if is_marked_root(&vendored) {
        return ResolvedRoot {
            path: vendored,
            source: RootSource::ProjectVendored,
        };
    }

    // Step 5: global ~/.topology install.
    if let Some(h) = home {
        let global = h.join(".topology");
        if is_marked_root(&global) {
            return ResolvedRoot {
                path: global,
                source: RootSource::GlobalHome,
            };
        }
    }

    // Step 6: fallback.
    ResolvedRoot {
        path: start.to_path_buf(),
        source: RootSource::Fallback,
    }
}

/// Locate the framework root. Emits a single stderr warning when resolution falls
/// back to cwd (i.e. no deterministic root was found). All other callers that need
/// provenance should use `resolved_root()` instead.
pub(crate) fn framework_root() -> PathBuf {
    let r = resolved_root();
    if r.source == RootSource::Fallback {
        // Most commands resolve the framework root more than once per process
        // (e.g. directly and again via artifacts_root()); warn only on the first.
        static FALLBACK_WARNING: std::sync::Once = std::sync::Once::new();
        FALLBACK_WARNING.call_once(|| {
            eprintln!(
                "gatekeeper: no framework root found; falling back to {} (run 'gatekeeper doctor')",
                r.path.display()
            );
        });
    }
    r.path
}

/// Resolve and return the full `ResolvedRoot` (path + provenance). Used by `doctor`
/// and unit tests that need to inspect the source step.
pub(crate) fn resolved_root() -> ResolvedRoot {
    let start = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let env_override = env::var_os("TOPOLOGY_ROOT").map(PathBuf::from);
    let exe_path = env::current_exe().ok();
    let home = env::var_os("HOME").map(PathBuf::from);
    resolve_root(
        &start,
        env_override.as_deref(),
        exe_path.as_deref(),
        home.as_deref(),
    )
}

/// Walk up from `start` to the nearest directory that contains `.git` (as a dir or file, so
/// worktrees are handled). Falls back to `start` when no `.git` is found.
pub(crate) fn resolve_project_root(start: &Path) -> PathBuf {
    let mut dir = start.to_path_buf();
    loop {
        let git_entry = dir.join(".git");
        if git_entry.is_dir() || git_entry.is_file() {
            return dir;
        }
        if !dir.pop() {
            return start.to_path_buf();
        }
    }
}

/// Locate the project root (nearest `.git` ancestor of cwd, or cwd).
pub(crate) fn project_root() -> PathBuf {
    let start = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    resolve_project_root(&start)
}

/// Compute the artifacts root given the project and framework roots.
///
/// Rule: when project == framework (the framework repo governs itself), artifacts live at
/// `project/docs`; otherwise they live at `project/.claude/topology`. Comparison uses
/// `canonicalize` when available, falling back to plain equality when the paths are not yet
/// on disk.
pub(crate) fn resolve_artifacts_root(project: &Path, framework: &Path) -> PathBuf {
    let same = match (fs::canonicalize(project), fs::canonicalize(framework)) {
        (Ok(p), Ok(f)) => p == f,
        _ => project == framework,
    };
    if same {
        project.join("docs")
    } else {
        project.join(".claude").join("topology")
    }
}

/// The artifacts root for the current process: docs/ when project == framework, else
/// .claude/topology/ relative to the project root.
pub(crate) fn artifacts_root() -> PathBuf {
    resolve_artifacts_root(&project_root(), &framework_root())
}

// ── per-subcommand usage strings are sourced from SUBCOMMANDS (see dispatch table above) ──

/// Check for `--help` / `-h` OR an unrecognized flag in `args`.
///
/// * `--help` / `-h` → print usage to stdout, return `Some(0)`.
/// * An arg that starts with `-` but is not in `known_flags` → print error + usage to stderr,
///   return `Some(2)`.
/// * Otherwise → return `None` (proceed normally).
///
/// `args` is the slice AFTER the subcommand name.  Flags consumed by the known set (the caller
/// iterates those separately) are listed in `known_flags`.  Anything after `--` is ignored so
/// that `check finish -- cmd --any-flag` passes through unmolested.
pub(crate) fn check_help_or_unknown(
    sub: &str,
    args: &[String],
    known_flags: &[&str],
    usage: &str,
) -> Option<i32> {
    // Stop scanning at `--` — everything after it is a passthrough (e.g. finish gate).
    let scan_args: &[String] = match args.iter().position(|a| a == "--") {
        Some(pos) => &args[..pos],
        None => args,
    };
    for arg in scan_args {
        if arg == "--help" || arg == "-h" {
            println!("{usage}");
            return Some(0);
        }
        if arg.starts_with('-') && !known_flags.contains(&arg.as_str()) {
            eprintln!("gatekeeper {sub}: unknown flag '{arg}'\n{usage}");
            return Some(2);
        }
    }
    None
}

fn cmd_list(args: &[String]) -> i32 {
    if let Some(code) = check_help_or_unknown("list", args, &[], lookup_usage("list")) {
        return code;
    }
    let skills_dir = framework_root().join("skills");
    let mut entries: Vec<PathBuf> = match fs::read_dir(&skills_dir) {
        Ok(rd) => rd.filter_map(|e| e.ok().map(|e| e.path())).collect(),
        Err(_) => {
            eprintln!("gatekeeper: no skills/ directory found");
            return 1;
        }
    };
    entries.sort();
    for path in entries {
        if !path.is_dir() {
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let desc = read_description(&path.join("SKILL.md")).unwrap_or_default();
        println!("  {name:<22} {desc}");
    }
    0
}

/// Pull the `description:` line out of a SKILL.md YAML frontmatter block.
fn read_description(skill_md: &Path) -> Option<String> {
    let text = fs::read_to_string(skill_md).ok()?;
    let mut in_front = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed == "---" {
            if in_front {
                break;
            }
            in_front = true;
            continue;
        }
        if in_front {
            if let Some(rest) = trimmed.strip_prefix("description:") {
                return Some(rest.trim().to_string());
            }
        }
    }
    None
}

/// Extract the user-facing prompt text from stdin.
///
/// Claude Code's `UserPromptSubmit` hook delivers a JSON object such as
/// `{"prompt":"..."}` on stdin.  When we receive a JSON object that contains
/// a top-level `"prompt"` string field we extract just that string so that
/// keyword matching runs only against the user's words — not the envelope keys
/// (which would cause the word "prompt" inside `{"prompt":"…"}` to route
/// `finish-branch` on every single hook invocation).
///
/// Non-JSON input (plain text) is returned as `None` so the caller keeps the
/// original without an extra allocation.  A JSON envelope returns `Some(text)`.
fn extract_prompt_owned(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    // Fast bail-out: only attempt JSON parsing when the input starts with '{'
    if !trimmed.starts_with('{') {
        return None;
    }
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed) {
        if let Some(s) = val.get("prompt").and_then(|v| v.as_str()) {
            return Some(s.to_owned());
        }
    }
    None
}

fn cmd_activate(args: &[String]) -> i32 {
    if let Some(code) = check_help_or_unknown("activate", args, &[], lookup_usage("activate")) {
        return code;
    }
    let mut raw = String::new();
    if std::io::stdin().read_to_string(&mut raw).is_err() {
        eprintln!("gatekeeper: failed to read stdin");
        return 1;
    }
    let extracted = extract_prompt_owned(&raw);
    let prompt = extracted.as_deref().unwrap_or(&raw);
    let prompt_lc = prompt.to_lowercase();

    let rules_path = framework_root().join("hooks").join("skill-rules.json");
    let matched = match fs::read_to_string(&rules_path) {
        Ok(raw) => match serde_json::from_str::<serde_json::Value>(&raw) {
            Ok(v) => route(&v, &prompt_lc),
            Err(e) => {
                eprintln!("gatekeeper: skill-rules.json parse error: {e}");
                return 1;
            }
        },
        Err(_) => Vec::new(),
    };

    println!("Topology: evaluate your skills before acting.");
    if matched.is_empty() {
        println!("No keyword-routed skills matched. Still run `getting-started` to pick the gate.");
    } else {
        println!("Routed skills for this prompt:");
        for (name, enforcement) in matched {
            println!("  - {name} [{enforcement}]");
        }
    }
    print!("{}", instinct::activate_section(&framework_root()));
    println!("You may not write production code before the design and plan gates pass.");
    0
}

/// Build a word-boundary regex for a keyword phrase (case-insensitive).
///
/// Each word in the phrase must appear as a whole word in the prompt.
/// For a single-word keyword like "pr" this prevents it from matching inside
/// "prompt", "approach", "print", etc.  Multi-word phrases are matched as a
/// contiguous whole-word sequence.
///
/// Returns `None` if the keyword is empty or the regex cannot be compiled
/// (should never happen for the simple ASCII keywords in skill-rules.json).
fn keyword_regex(keyword: &str) -> Option<Regex> {
    let kw = keyword.trim();
    if kw.is_empty() {
        return None;
    }
    // Escape each word individually and join with `\s+` to allow any whitespace.
    let inner: String = kw
        .split_whitespace()
        .map(regex::escape)
        .collect::<Vec<_>>()
        .join(r"\s+");
    // Wrap with word-boundary anchors.
    let pattern = format!(r"(?i)\b{inner}\b");
    Regex::new(&pattern).ok()
}

/// Given parsed skill-rules JSON and a lowercased prompt, return (skill, enforcement) matches.
fn route(rules: &serde_json::Value, prompt_lc: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let Some(skills) = rules.get("skills").and_then(|v| v.as_object()) else {
        return out;
    };
    for (name, cfg) in skills {
        let enforcement = cfg
            .get("enforcement")
            .and_then(|v| v.as_str())
            .unwrap_or("suggest")
            .to_string();
        let keywords = cfg
            .get("promptTriggers")
            .and_then(|t| t.get("keywords"))
            .and_then(|k| k.as_array());
        if let Some(kws) = keywords {
            let hit = kws
                .iter()
                .filter_map(|k| k.as_str())
                .filter_map(keyword_regex)
                .any(|re| re.is_match(prompt_lc));
            if hit {
                out.push((name.clone(), enforcement));
            }
        }
    }
    out.sort();
    out
}

/// Group-level dispatcher for `check`.
///
/// - bare `check`           → group usage + exit 2
/// - `check --help/-h`      → group usage + exit 0
/// - `check <known gate>`   → delegate to the per-gate handler (which prints its own one-line usage
///   on `--help`)
/// - `check <unknown>`      → error line + group usage + exit 2
fn dispatch_check(args: &[String]) -> i32 {
    let group_usage = check_group_usage();
    let Some(gate) = args.first().map(String::as_str) else {
        eprintln!("gatekeeper check: missing gate name\n{group_usage}");
        return 2;
    };
    if gate == "--help" || gate == "-h" {
        println!("{group_usage}");
        return 0;
    }
    // Look for "check <gate>" in the dispatch table.
    let key = format!("check {gate}");
    if let Some(spec) = SUBCOMMANDS.iter().find(|s| s.name == key.as_str()) {
        return (spec.handler)(if args.len() > 1 { &args[1..] } else { &[] });
    }
    // Unknown gate — only emit an error if it doesn't look like a --flag (flags are handled
    // above by check_help_or_unknown inside each handler).
    eprintln!("gatekeeper check: unknown gate '{gate}'\n{group_usage}");
    2
}

// ── thin handler wrappers (adapt gate logic to fn(&[String]) -> i32) ─────────

fn handle_check_research(args: &[String]) -> i32 {
    if let Some(code) = check_help_or_unknown(
        "check research",
        args,
        &["--feature"],
        lookup_usage("check research"),
    ) {
        return code;
    }
    gate_doc_exists("research", "research", &feature_arg_from(args))
}

fn handle_check_design(args: &[String]) -> i32 {
    if let Some(code) = check_help_or_unknown(
        "check design",
        args,
        &["--feature"],
        lookup_usage("check design"),
    ) {
        return code;
    }
    let f = feature_arg_from(args);
    if f.is_empty() {
        eprintln!("gatekeeper: --feature <slug> is required");
        return 2;
    }

    // Config strictness: parse failure exits 2 for the three hardened gates.
    let arts = artifacts_root();
    let load_result = config::ProjectConfig::load_result(&arts);
    if let config::LoadResult::ParseFailed(ref e) = load_result {
        eprintln!(
            "gatekeeper check design: config.toml parse error: {e} — cannot proceed (fix config.toml)",
        );
        return 2;
    }

    let cfg = match load_result {
        config::LoadResult::Ok(c) => c,
        config::LoadResult::Missing => config::ProjectConfig::default(),
        config::LoadResult::ParseFailed(_) => unreachable!(),
    };

    match find_doc("research", &f) {
        None => {
            let dir = arts.join("research");
            println!(
                "FAIL design gate: research-first — no {}/*{f}*.md",
                dir.display()
            );
            1
        }
        Some(_) => gate_design_approved(&f, &cfg),
    }
}

fn handle_check_plan(args: &[String]) -> i32 {
    if let Some(code) = check_help_or_unknown(
        "check plan",
        args,
        &["--feature"],
        lookup_usage("check plan"),
    ) {
        return code;
    }
    gate_plan(&feature_arg_from(args))
}

fn handle_check_verify(args: &[String]) -> i32 {
    if let Some(code) = check_help_or_unknown(
        "check verify",
        args,
        &["--feature"],
        lookup_usage("check verify"),
    ) {
        return code;
    }
    gate_verify(&feature_arg_from(args))
}

/// The verify gate with evidence-replay support (spec §3).
fn gate_verify(feature: &str) -> i32 {
    if feature.is_empty() {
        eprintln!("gatekeeper: --feature <slug> is required");
        return 2;
    }

    let arts = artifacts_root();

    // Config strictness: parse failure exits 2 for the three hardened gates.
    let load_result = config::ProjectConfig::load_result(&arts);
    if let config::LoadResult::ParseFailed(ref e) = load_result {
        eprintln!(
            "gatekeeper check verify: config.toml parse error: {e} — cannot proceed (fix config.toml)",
        );
        return 2;
    }
    let cfg = match load_result {
        config::LoadResult::Ok(c) => c,
        config::LoadResult::Missing => config::ProjectConfig::default(),
        config::LoadResult::ParseFailed(_) => unreachable!(),
    };

    // File-existence check (presence-mode baseline).
    let artifact_path = match find_doc("verify", feature) {
        Some(p) => p,
        None => {
            let dir = arts.join("verify");
            println!(
                "FAIL verify gate: no {}/*{feature}*.md found",
                dir.display()
            );
            return 1;
        }
    };

    let proj = project_root();
    verify::run_verify_gate(&artifact_path, &proj, &cfg)
}

fn handle_check_tdd(args: &[String]) -> i32 {
    if let Some(code) = check_help_or_unknown(
        "check tdd",
        args,
        &["--feature", "--base"],
        lookup_usage("check tdd"),
    ) {
        return code;
    }
    let cfg = config::ProjectConfig::load(&artifacts_root());
    let base = base_arg_from(args).or(cfg.base_branch);
    tdd::gate_tdd(&project_root(), &feature_arg_from(args), base.as_deref())
}

fn handle_check_review(args: &[String]) -> i32 {
    if let Some(code) = check_help_or_unknown(
        "check review",
        args,
        &["--feature", "--base"],
        lookup_usage("check review"),
    ) {
        return code;
    }
    let arts = artifacts_root();
    let cfg = config::ProjectConfig::load(&arts);
    review::gate_review(
        &project_root(),
        &arts,
        &feature_arg_from(args),
        base_arg_from(args).as_deref(),
        cfg.base_branch.as_deref(),
    )
}

fn handle_check_finish(args: &[String]) -> i32 {
    // `finish` passes everything after `--` through to a child process.
    if let Some(code) =
        check_help_or_unknown("check finish", args, &["--"], lookup_usage("check finish"))
    {
        return code;
    }

    let arts = artifacts_root();

    // Config strictness: parse failure exits 2 for the three hardened gates.
    let load_result = config::ProjectConfig::load_result(&arts);
    if let config::LoadResult::ParseFailed(ref e) = load_result {
        eprintln!(
            "gatekeeper check finish: config.toml parse error: {e} — cannot proceed (fix config.toml)",
        );
        return 2;
    }
    let cfg = match load_result {
        config::LoadResult::Ok(c) => c,
        config::LoadResult::Missing => config::ProjectConfig::default(),
        config::LoadResult::ParseFailed(_) => unreachable!(),
    };

    // Reconstruct the full args slice that gate_finish expects (args after "check", including
    // the "--" separator).  The handler receives args AFTER "finish", so we pass them directly
    // to gate_finish.
    gate_finish(args, &cfg)
}

fn handle_check_docs(args: &[String]) -> i32 {
    if let Some(code) = check_help_or_unknown("check docs", args, &[], lookup_usage("check docs")) {
        return code;
    }
    check_docs(&framework_root())
}

fn handle_doctor(args: &[String]) -> i32 {
    if let Some(code) = check_help_or_unknown("doctor", args, &[], lookup_usage("doctor")) {
        return code;
    }
    // No fallback eprintln here: doctor's own report carries `resolved by: fallback (cwd)`
    // and the F1 FAIL line, and cmd_doctor's internal artifacts_root() call already routes
    // through the Once-guarded warning in framework_root().
    let rr = resolved_root();
    doctor::cmd_doctor(&rr.path, &rr.source)
}

/// Docs-coverage lint (three rules, all satisfiable on the reconciled tree).
///
/// R1: every `skills/*/SKILL.md` passes `learn::validate_skill_file` (fence + non-empty name + description).
/// R2: every `docs/adr/00NN-*.md` (excluding README.md) is linked from `docs/adr/README.md`.
/// R3: every `docs/verify/<f>.md` token in `docs/ROADMAP.md` resolves on disk (forward-only; no regex dep).
///
/// Exit 0 clean, 1 listing specific gaps.
fn check_docs(root: &Path) -> i32 {
    let mut gaps: Vec<String> = Vec::new();

    // R1 — skills frontmatter
    let skills_dir = root.join("skills");
    if let Ok(rd) = fs::read_dir(&skills_dir) {
        let mut skill_dirs: Vec<_> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        skill_dirs.sort();
        for skill_dir in skill_dirs {
            let skill_md = skill_dir.join("SKILL.md");
            if !skill_md.exists() {
                continue;
            }
            if let Err(e) = learn::validate_skill_file(&skill_md) {
                gaps.push(format!(
                    "R1: skills/{}/SKILL.md: {e}",
                    skill_dir.file_name().unwrap_or_default().to_string_lossy()
                ));
            }
        }
    }

    // R2 — ADR index coverage
    let adr_dir = root.join("docs").join("adr");
    let adr_readme = adr_dir.join("README.md");
    let readme_text = fs::read_to_string(&adr_readme).unwrap_or_default();
    if let Ok(rd) = fs::read_dir(&adr_dir) {
        let mut adr_files: Vec<_> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                let name = p
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                p.is_file()
                    && name.ends_with(".md")
                    && name != "README.md"
                    && name.chars().take(4).all(|c| c.is_ascii_digit())
            })
            .collect();
        adr_files.sort();
        for adr_path in adr_files {
            let fname = adr_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            if !readme_text.contains(&fname) {
                gaps.push(format!(
                    "R2: docs/adr/{fname} not linked from docs/adr/README.md"
                ));
            }
        }
    }

    // R3 — ROADMAP verify-note pointers
    let roadmap = root.join("docs").join("ROADMAP.md");
    if let Ok(text) = fs::read_to_string(&roadmap) {
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        // Token-scan for "docs/verify/" followed by a valid filename (no regex dep).
        let prefix = "docs/verify/";
        let mut search = text.as_str();
        while let Some(pos) = search.find(prefix) {
            let after = &search[pos + prefix.len()..];
            // Collect chars of the filename: alphanumeric, '.', '-', '_'
            let fname: String = after
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '.' || *c == '-' || *c == '_')
                .collect();
            if fname.ends_with(".md") && !fname.is_empty() && seen.insert(fname.clone()) {
                let target = root.join("docs").join("verify").join(&fname);
                if !target.is_file() {
                    gaps.push(format!(
                        "R3: docs/verify/{fname} referenced in ROADMAP.md but file not found"
                    ));
                }
            }
            search = &search[pos + prefix.len()..];
        }
    }

    if gaps.is_empty() {
        println!("check docs: ok");
        0
    } else {
        for g in &gaps {
            println!("FAIL {g}");
        }
        1
    }
}

/// Extract the `--feature <slug>` value from a flag list.
pub(crate) fn feature_arg(args: &[String]) -> String {
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "--feature" {
            return it.next().cloned().unwrap_or_default();
        }
    }
    String::new()
}

/// Extract the `--feature <slug>` value from a flag list (alias used by handlers).
fn feature_arg_from(args: &[String]) -> String {
    feature_arg(args)
}

/// Extract the `--base <ref>` value from a flag list.
fn base_arg_from(args: &[String]) -> Option<String> {
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "--base" {
            return it.next().cloned();
        }
    }
    None
}

/// Find a markdown doc under <artifacts_root>/<sub>/ whose filename contains the feature slug.
pub(crate) fn find_doc(sub: &str, feature: &str) -> Option<PathBuf> {
    if feature.is_empty() {
        return None;
    }
    let dir = artifacts_root().join(sub);
    let rd = fs::read_dir(dir).ok()?;
    for e in rd.flatten() {
        let p = e.path();
        let fname = p.file_name()?.to_string_lossy().to_string();
        if fname.ends_with(".md") && fname.contains(feature) {
            return Some(p);
        }
    }
    None
}

/// `label` is the gate name as the user invoked it; `sub` is the artifact directory it reads.
/// They differ for the design gate (invoked as `design`, artifacts in `specs/`) — reporting
/// under the invoked name keeps the failure actionable without a name/directory mismatch.
fn gate_doc_exists(label: &str, sub: &str, feature: &str) -> i32 {
    if feature.is_empty() {
        eprintln!("gatekeeper: --feature <slug> is required");
        return 2;
    }
    match find_doc(sub, feature) {
        Some(p) => {
            println!("PASS {label} gate: {}", p.display());
            0
        }
        None => {
            let dir = artifacts_root().join(sub);
            println!(
                "FAIL {label} gate: no {}/*{feature}*.md found",
                dir.display()
            );
            1
        }
    }
}

/// Return true if the spec file contains an approval marker on any line.
///
/// Accepted forms (case-insensitive, flexible whitespace around `:` and value):
///   `Status: approved`                   — plain, with optional trailing text (dates, notes)
///   `**Status:** approved`               — Markdown bold field
///   `- **Status:** approved`             — bullet list item (house style)
///   `🟢 **Approved …`                    — emoji-prefixed bold approved (legacy form)
///
/// Algorithm per line:
///   1. Fold to lowercase and strip Markdown decoration (`*`, `-` bullets, common emoji).
///   2. Look for `status` followed by optional whitespace, a colon, optional whitespace, then
///      a value starting with `approved`.
pub(crate) fn spec_is_approved(text: &str) -> bool {
    for line in text.lines() {
        // Strip all `*` (Markdown bold/italic markers), leading `-` bullets, the 🟢 emoji, and
        // surrounding whitespace.  This normalises every accepted form into `status: approved …`
        // without needing separate branches for each decoration variant.
        let cleaned: String = line.chars().filter(|&c| c != '*').collect::<String>();
        let cleaned = cleaned.trim().trim_start_matches('-').trim();
        // Strip the 🟢 emoji if present (multi-byte: match the literal char).
        let cleaned = cleaned.trim_start_matches('🟢').trim();
        let lc = cleaned.to_lowercase();
        // Match `status` then optional whitespace then `:` then optional whitespace then `approved`.
        if let Some(after_status) = lc.strip_prefix("status") {
            let rest = after_status.trim_start();
            if let Some(after_colon) = rest.strip_prefix(':') {
                let value = after_colon.trim();
                if value.starts_with("approved") {
                    return true;
                }
            }
        }
    }
    false
}

/// Design gate: after the research-first sequence-lock and file-existence checks, additionally
/// require an explicit approval marker in the spec file, plus optional substance floor and
/// human-commit approval checks (spec §4).
fn gate_design_approved(feature: &str, cfg: &config::ProjectConfig) -> i32 {
    if feature.is_empty() {
        eprintln!("gatekeeper: --feature <slug> is required");
        return 2;
    }
    let Some(p) = find_doc("specs", feature) else {
        let dir = artifacts_root().join("specs");
        println!(
            "FAIL design gate: no {}/*{feature}*.md found",
            dir.display()
        );
        return 1;
    };
    let text = fs::read_to_string(&p).unwrap_or_default();
    if !spec_is_approved(&text) {
        println!(
            "FAIL design gate: {} exists but is not approved",
            p.display()
        );
        println!("  add a line 'Status: approved' once a human has signed off the design");
        println!("  accepted forms (anywhere in the file, case-insensitive):");
        println!("    Status: approved");
        println!("    **Status:** approved");
        println!("    - **Status:** approved");
        return 1;
    }

    // ── Substance floor check (§4) ────────────────────────────────────────────
    let substance_ok = design_check_substance(&text);
    let substance_configured = if cfg.design_substance_floor {
        verify::ShadowConfigured::On
    } else {
        verify::ShadowConfigured::Default
    };
    let art_str = p.to_string_lossy();
    if cfg.design_substance_floor {
        if !substance_ok {
            println!(
                "FAIL design gate: {} lacks substance (need ≥2 '## ' headings and ≥1 body line)",
                p.display()
            );
            println!("  add meaningful section headings and content to the spec");
            // Still emit SHADOW with fail result
            verify::emit_shadow(
                "design",
                "substance_floor",
                substance_configured,
                Some(&art_str),
                None,
                verify::ShadowResult::Fail,
                "spec has <2 ## headings or no body line outside Status",
            );
            return 1;
        }
        verify::emit_shadow(
            "design",
            "substance_floor",
            substance_configured,
            Some(&art_str),
            None,
            verify::ShadowResult::Pass,
            "≥2 ## headings and ≥1 body line found",
        );
    } else {
        // key is off — compute anyway, emit shadow
        verify::emit_shadow(
            "design",
            "substance_floor",
            substance_configured,
            Some(&art_str),
            None,
            if substance_ok {
                verify::ShadowResult::Pass
            } else {
                verify::ShadowResult::Fail
            },
            if substance_ok {
                "≥2 ## headings and ≥1 body line found"
            } else {
                "spec has <2 ## headings or no body line outside Status"
            },
        );
    }

    // ── Approval provenance check (§4) ────────────────────────────────────────
    let proj = project_root();
    let approval_configured = match cfg.design_approval {
        config::DesignApproval::HumanCommit => verify::ShadowConfigured::On,
        config::DesignApproval::StatusLine => verify::ShadowConfigured::Default,
    };

    match cfg.design_approval {
        config::DesignApproval::HumanCommit => {
            let result = design_check_human_commit(&p, &proj, &cfg.design_agent_trailer_patterns);
            match result {
                DesignApprovalResult::Pass => {
                    verify::emit_shadow(
                        "design",
                        "approval_provenance",
                        approval_configured,
                        Some(&art_str),
                        None,
                        verify::ShadowResult::Pass,
                        "approval commit has no agent trailer",
                    );
                }
                DesignApprovalResult::Fail(ref msg) => {
                    verify::emit_shadow(
                        "design",
                        "approval_provenance",
                        approval_configured,
                        Some(&art_str),
                        None,
                        verify::ShadowResult::Fail,
                        msg,
                    );
                    println!("FAIL design gate: {msg}");
                    return 1;
                }
                DesignApprovalResult::Skip(ref msg) => {
                    // Skip = obstacle: fail closed when enforced
                    verify::emit_shadow(
                        "design",
                        "approval_provenance",
                        approval_configured,
                        Some(&art_str),
                        None,
                        verify::ShadowResult::Skip,
                        msg,
                    );
                    println!("FAIL design gate: {msg}");
                    return 1;
                }
            }
        }
        config::DesignApproval::StatusLine => {
            // Compute anyway, emit SHADOW — do not affect exit code
            let result = design_check_human_commit(&p, &proj, &cfg.design_agent_trailer_patterns);
            let (shadow_result, detail) = match &result {
                DesignApprovalResult::Pass => (
                    verify::ShadowResult::Pass,
                    "approval commit has no agent trailer".to_string(),
                ),
                DesignApprovalResult::Fail(msg) => (verify::ShadowResult::Fail, msg.clone()),
                DesignApprovalResult::Skip(msg) => (verify::ShadowResult::Skip, msg.clone()),
            };
            verify::emit_shadow(
                "design",
                "approval_provenance",
                approval_configured,
                Some(&art_str),
                None,
                shadow_result,
                &detail,
            );
        }
    }

    println!("PASS design gate: {}", p.display());
    0
}

/// Result of the human-commit approval provenance check.
enum DesignApprovalResult {
    Pass,
    Fail(String),
    /// Skip = obstacle encountered; message describes the fix.
    Skip(String),
}

/// Substance floor predicate (spec §4):
/// ≥2 `## ` headings AND ≥1 non-empty body line outside the Status line and not inside an
/// HTML comment.
pub(crate) fn design_check_substance(text: &str) -> bool {
    let mut heading_count = 0usize;
    let mut body_line_count = 0usize;
    let mut in_comment = false;

    for line in text.lines() {
        // Track HTML comments (strip_comments logic but line-granular)
        let mut rest = line;
        // Handle comment toggling on the line
        loop {
            if in_comment {
                if let Some(end) = rest.find("-->") {
                    in_comment = false;
                    rest = &rest[end + 3..];
                } else {
                    break; // whole line is inside comment
                }
            } else {
                if let Some(start) = rest.find("<!--") {
                    // Part before the comment is visible
                    let visible = &rest[..start];
                    // count visible part
                    if visible.contains("## ") {
                        heading_count += 1;
                    }
                    in_comment = true;
                    rest = &rest[start + 4..];
                } else {
                    // No comment opening — rest is fully visible
                    if rest.starts_with("## ") {
                        heading_count += 1;
                    } else {
                        let trimmed = rest.trim();
                        if !trimmed.is_empty() {
                            // Not a heading, not inside a comment — check if it's the Status line
                            let cleaned: String = trimmed.chars().filter(|&c| c != '*').collect();
                            let cleaned = cleaned.trim().trim_start_matches('-').trim();
                            let cleaned = cleaned.trim_start_matches('🟢').trim();
                            let lc = cleaned.to_lowercase();
                            let is_status = lc.starts_with("status");
                            if !is_status {
                                body_line_count += 1;
                            }
                        }
                    }
                    break;
                }
            }
        }
    }

    heading_count >= 2 && body_line_count >= 1
}

/// Check the approval provenance of a spec file via git history.
///
/// Returns `Pass`, `Fail(message)`, or `Skip(obstacle)`.
fn design_check_human_commit(
    spec_path: &std::path::Path,
    project_root: &std::path::Path,
    agent_trailer_patterns: &[String],
) -> DesignApprovalResult {
    // ── 1. Git floor: require git ≥ 2.15 ─────────────────────────────────────
    match probe_git_version() {
        GitVersionResult::TooOld(v) => {
            return DesignApprovalResult::Skip(format!(
                "approval_provenance: git {v} is too old (need ≥ 2.15); \
                 upgrade git to enable the human-commit check"
            ));
        }
        GitVersionResult::Unparsable(raw) => {
            return DesignApprovalResult::Skip(format!(
                "approval_provenance: cannot parse git version output {raw:?}; \
                 upgrade git or fix PATH"
            ));
        }
        GitVersionResult::Ok => {}
    }

    // ── 2. Shallow-clone check ────────────────────────────────────────────────
    match probe_git_shallow(project_root) {
        ShallowResult::Shallow => {
            return DesignApprovalResult::Skip(
                "approval_provenance: repository is a shallow clone; \
                 run 'git fetch --unshallow' to enable the human-commit check"
                    .to_string(),
            );
        }
        ShallowResult::Error(e) => {
            return DesignApprovalResult::Skip(format!(
                "approval_provenance: cannot check shallow status: {e}; \
                 ensure git is functional"
            ));
        }
        ShallowResult::NotShallow => {}
    }

    // ── 3. Untracked check ───────────────────────────────────────────────────
    // Is the spec untracked?
    let relpath = match spec_path.strip_prefix(project_root) {
        Ok(r) => r.to_string_lossy().to_string(),
        Err(_) => spec_path.to_string_lossy().to_string(),
    };

    let ls_out = std::process::Command::new("git")
        .args([
            "-C",
            &project_root.to_string_lossy(),
            "ls-files",
            "--error-unmatch",
            "--",
            &relpath,
        ])
        .output();
    match ls_out {
        Err(e) => {
            return DesignApprovalResult::Skip(format!(
                "approval_provenance: cannot run git ls-files: {e}"
            ));
        }
        Ok(out) if !out.status.success() => {
            return DesignApprovalResult::Skip(format!(
                "approval_provenance: spec {relpath} is untracked — commit the spec file \
                 to enable the human-commit check"
            ));
        }
        Ok(_) => {}
    }

    // ── 4. Dirty spec check ───────────────────────────────────────────────────
    // Unstaged changes?
    let diff_out = std::process::Command::new("git")
        .args([
            "-C",
            &project_root.to_string_lossy(),
            "diff",
            "--quiet",
            "--",
            &relpath,
        ])
        .status();
    match diff_out {
        Err(e) => {
            return DesignApprovalResult::Skip(format!(
                "approval_provenance: cannot run git diff: {e}"
            ));
        }
        Ok(s) if !s.success() => {
            return DesignApprovalResult::Skip(format!(
                "approval_provenance: spec {relpath} has unstaged changes — \
                 commit all edits before running the human-commit check"
            ));
        }
        Ok(_) => {}
    }
    // Staged changes?
    let diff_cached_out = std::process::Command::new("git")
        .args([
            "-C",
            &project_root.to_string_lossy(),
            "diff",
            "--cached",
            "--quiet",
            "--",
            &relpath,
        ])
        .status();
    match diff_cached_out {
        Err(e) => {
            return DesignApprovalResult::Skip(format!(
                "approval_provenance: cannot run git diff --cached: {e}"
            ));
        }
        Ok(s) if !s.success() => {
            return DesignApprovalResult::Skip(format!(
                "approval_provenance: spec {relpath} has staged (index) changes — \
                 commit or unstage all edits before running the human-commit check"
            ));
        }
        Ok(_) => {}
    }

    // ── 5. Read the committed spec to find the approval line number ──────────
    let committed_text_out = std::process::Command::new("git")
        .args([
            "-C",
            &project_root.to_string_lossy(),
            "show",
            &format!("HEAD:{relpath}"),
        ])
        .output();
    let committed_text = match committed_text_out {
        Err(e) => {
            return DesignApprovalResult::Skip(format!(
                "approval_provenance: cannot read committed spec via git show: {e}"
            ));
        }
        Ok(out) if !out.status.success() => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            return DesignApprovalResult::Skip(format!(
                "approval_provenance: git show HEAD:{relpath} failed: {stderr}"
            ));
        }
        Ok(out) => String::from_utf8_lossy(&out.stdout).to_string(),
    };

    // Find the line number of the first approval line (1-based, matching spec_is_approved logic)
    let approval_line_number = {
        let mut found = None;
        for (idx, line) in committed_text.lines().enumerate() {
            // Mirror spec_is_approved normalization
            let cleaned: String = line.chars().filter(|&c| c != '*').collect();
            let cleaned = cleaned.trim().trim_start_matches('-').trim();
            let cleaned = cleaned.trim_start_matches('🟢').trim();
            let lc = cleaned.to_lowercase();
            if let Some(after_status) = lc.strip_prefix("status") {
                let rest = after_status.trim_start();
                if let Some(after_colon) = rest.strip_prefix(':') {
                    let value = after_colon.trim();
                    if value.starts_with("approved") {
                        found = Some(idx + 1); // 1-based
                        break;
                    }
                }
            }
        }
        match found {
            Some(n) => n,
            None => {
                // Should not happen since spec_is_approved already checked above
                return DesignApprovalResult::Skip(
                    "approval_provenance: cannot locate approval line in committed spec"
                        .to_string(),
                );
            }
        }
    };

    // ── 6. git log -L to find the commit that last touched the approval line ──
    // Format: `git log -L<n>,<n>:<path> --format=%H`
    let log_arg = format!("-L{approval_line_number},{approval_line_number}:{relpath}");
    let log_out = std::process::Command::new("git")
        .args([
            "-C",
            &project_root.to_string_lossy(),
            "log",
            &log_arg,
            "--format=%H",
        ])
        .output();
    let log_stdout = match log_out {
        Err(e) => {
            return DesignApprovalResult::Skip(format!(
                "approval_provenance: cannot run git log -L: {e}"
            ));
        }
        Ok(out) if !out.status.success() => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            return DesignApprovalResult::Skip(format!(
                "approval_provenance: git log -L failed: {stderr}"
            ));
        }
        Ok(out) => String::from_utf8_lossy(&out.stdout).to_string(),
    };

    // Extract the first non-empty 40-char hex SHA from the log output
    let approval_sha = log_stdout
        .lines()
        .map(str::trim)
        .find(|l| l.len() == 40 && l.chars().all(|c| c.is_ascii_hexdigit()))
        .map(str::to_owned);

    let sha = match approval_sha {
        Some(s) => s,
        None => {
            return DesignApprovalResult::Skip(format!(
                "approval_provenance: git log -L produced no commit SHA for line \
                 {approval_line_number} of {relpath}; output: {log_stdout:?}"
            ));
        }
    };

    // ── 7. Read trailers from the approval commit ────────────────────────────
    let trailer_out = std::process::Command::new("git")
        .args([
            "-C",
            &project_root.to_string_lossy(),
            "show",
            "-s",
            "--format=%(trailers)",
            &sha,
        ])
        .output();
    let trailers = match trailer_out {
        Err(e) => {
            return DesignApprovalResult::Skip(format!(
                "approval_provenance: cannot read trailers from commit {sha}: {e}"
            ));
        }
        Ok(out) if !out.status.success() => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            return DesignApprovalResult::Skip(format!(
                "approval_provenance: git show --format=%(trailers) failed for {sha}: {stderr}"
            ));
        }
        Ok(out) => String::from_utf8_lossy(&out.stdout).to_string(),
    };

    // ── 8. Check trailers against agent_trailer_patterns ─────────────────────
    // Compile patterns; check each Co-Authored-By value
    for line in trailers.lines() {
        // Trailer lines look like "Co-Authored-By: Name <email>" (case-insensitive key)
        let lc = line.to_lowercase();
        if !lc.starts_with("co-authored-by:") {
            continue;
        }
        let value = line["co-authored-by:".len()..].trim();
        for pattern in agent_trailer_patterns {
            match regex::Regex::new(pattern) {
                Ok(re) => {
                    if re.is_match(value) {
                        return DesignApprovalResult::Fail(format!(
                            "approval_provenance: commit {sha} carries agent trailer \
                             'Co-Authored-By: {value}' (matched pattern {pattern:?}); \
                             the spec must be approved by a human commit without an agent \
                             co-author trailer — this is a residual risk for sycophantic \
                             self-approval, not a claim about operator intent"
                        ));
                    }
                }
                Err(e) => {
                    return DesignApprovalResult::Skip(format!(
                        "approval_provenance: agent_trailer_patterns entry {pattern:?} \
                         is not a valid regex: {e}"
                    ));
                }
            }
        }
    }

    DesignApprovalResult::Pass
}

// ── git capability probes ─────────────────────────────────────────────────────

pub(crate) enum GitVersionResult {
    Ok,
    TooOld(String),
    Unparsable(String),
}

/// Probe `git --version` and check ≥ 2.15.
pub(crate) fn probe_git_version() -> GitVersionResult {
    let out = match std::process::Command::new("git").arg("--version").output() {
        Ok(o) => o,
        Err(e) => return GitVersionResult::Unparsable(format!("<error: {e}>")),
    };
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    // Expected: "git version 2.39.0" or "git version 2.39.0 (Apple Git-...)"
    parse_git_version_str(&s)
}

pub(crate) fn parse_git_version_str(s: &str) -> GitVersionResult {
    // Find "git version X.Y.Z..."
    let raw = s.trim();
    let version_part = if let Some(rest) = raw.strip_prefix("git version ") {
        rest.trim()
    } else {
        return GitVersionResult::Unparsable(raw.to_string());
    };
    // Take the first space-separated token as the version number
    let ver_token = version_part.split_whitespace().next().unwrap_or("");
    let mut parts = ver_token.split('.');
    let major: u32 = match parts.next().and_then(|p| p.parse().ok()) {
        Some(n) => n,
        None => return GitVersionResult::Unparsable(raw.to_string()),
    };
    let minor: u32 = match parts.next().and_then(|p| p.parse().ok()) {
        Some(n) => n,
        None => return GitVersionResult::Unparsable(raw.to_string()),
    };
    if major > 2 || (major == 2 && minor >= 15) {
        GitVersionResult::Ok
    } else {
        GitVersionResult::TooOld(ver_token.to_string())
    }
}

pub(crate) enum ShallowResult {
    NotShallow,
    Shallow,
    Error(String),
}

/// Check whether the git repo at `project_root` is a shallow clone.
pub(crate) fn probe_git_shallow(project_root: &std::path::Path) -> ShallowResult {
    let out = std::process::Command::new("git")
        .args([
            "-C",
            &project_root.to_string_lossy(),
            "rev-parse",
            "--is-shallow-repository",
        ])
        .output();
    match out {
        Err(e) => ShallowResult::Error(e.to_string()),
        Ok(o) if !o.status.success() => {
            let stderr = String::from_utf8_lossy(&o.stderr).to_string();
            ShallowResult::Error(stderr.trim().to_string())
        }
        Ok(o) => {
            let stdout = String::from_utf8_lossy(&o.stdout);
            let val = stdout.trim();
            match val {
                "true" => ShallowResult::Shallow,
                "false" => ShallowResult::NotShallow,
                other => ShallowResult::Error(format!("unexpected output: {other:?}")),
            }
        }
    }
}

fn gate_plan(feature: &str) -> i32 {
    if feature.is_empty() {
        eprintln!("gatekeeper: --feature <slug> is required");
        return 2;
    }
    let Some(p) = find_doc("plans", feature) else {
        let dir = artifacts_root().join("plans");
        println!("FAIL plan gate: no {}/*{feature}*.md found", dir.display());
        return 1;
    };
    let text = fs::read_to_string(&p).unwrap_or_default();
    if let Some(found) = find_placeholder(&text) {
        println!(
            "FAIL plan gate: {} contains placeholder '{}'",
            p.display(),
            found
        );
        return 1;
    }
    println!("PASS plan gate: {} (no placeholders)", p.display());
    0
}

/// Return the first placeholder token found in plan text (ignoring HTML comments).
fn find_placeholder(text: &str) -> Option<String> {
    let lc = strip_comments(text).to_lowercase();
    PLACEHOLDERS
        .iter()
        .find(|p| lc.contains(*p))
        .map(|p| p.to_string())
}

fn strip_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("<!--") {
        out.push_str(&rest[..start]);
        if let Some(end) = rest[start..].find("-->") {
            rest = &rest[start + end + 3..];
        } else {
            rest = "";
            break;
        }
    }
    out.push_str(rest);
    out
}

fn gate_finish(args: &[String], cfg: &config::ProjectConfig) -> i32 {
    let cli_cmd: Vec<&String> = args.iter().skip_while(|a| *a != "--").skip(1).collect();

    if !cli_cmd.is_empty() {
        // Explicit CLI `-- cmd` wins unconditionally over config.test_command.
        // Spec §5: floor applies to both invocation paths.
        let cmd_str = cli_cmd
            .iter()
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        return run_finish_command_parts_floor(&cli_cmd, &cmd_str, cfg);
    }

    // No CLI command — try config.test_command.
    if let Some(ref tc) = cfg.test_command {
        let tc_owned = tc.clone();
        return run_finish_sh_floor(&tc_owned, cfg);
    }

    // Nothing supplied — emit the original usage error, extended to mention config.
    let arts = artifacts_root();
    eprintln!("gatekeeper check finish -- <command...>  (command required)");
    eprintln!("  The finish gate runs your full test command and passes when it exits 0:");
    eprintln!("    gatekeeper check finish -- npm test");
    eprintln!("    gatekeeper check finish -- cargo test");
    eprintln!(
        "  Or set test_command in {}/config.toml to avoid retyping it.",
        arts.display()
    );
    2
}

// ── finish gate — streaming tee + zero-test floor ────────────────────────────

/// Run a finish command expressed as a pre-split argument list (from the CLI `-- cmd...` form).
/// Captures output while streaming through; applies the zero-test floor when configured.
fn run_finish_command_parts_floor(
    cmd: &[&String],
    cmd_str: &str,
    cfg: &config::ProjectConfig,
) -> i32 {
    use std::process::Stdio;
    let mut child = match Command::new(cmd[0])
        .args(&cmd[1..])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            println!("FAIL finish gate: could not run command: {e}");
            return 1;
        }
    };
    let (transcript, truncated, exit_ok) = drain_finish_child(&mut child);
    apply_finish_floor(transcript, truncated, exit_ok, cmd_str, cfg)
}

/// Run a finish command expressed as a single shell string (from config.test_command).
/// Uses `sh -c` so shell syntax (pipes, &&, etc.) works as expected.
/// Captures output while streaming through; applies the zero-test floor when configured.
fn run_finish_sh_floor(cmd: &str, cfg: &config::ProjectConfig) -> i32 {
    use std::process::Stdio;
    let mut child = match Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            println!("FAIL finish gate: could not run command: {e}");
            return 1;
        }
    };
    let (transcript, truncated, exit_ok) = drain_finish_child(&mut child);
    apply_finish_floor(transcript, truncated, exit_ok, cmd, cfg)
}

/// Drain a child process's stdout+stderr while **streaming each line through** to the
/// real stdout/stderr respectively.  Returns `(merged_transcript, truncated, exit_ok)`.
///
/// Two reader threads tee each line: send to the channel (for capture) AND write it to
/// the real fd immediately.  Tail-capped at 1 MiB.
fn drain_finish_child(child: &mut std::process::Child) -> (String, bool, bool) {
    use std::io::{BufRead, BufReader};
    use std::sync::mpsc;
    use std::thread;

    const OUTPUT_CAP: usize = 1024 * 1024; // 1 MiB

    let stdout = child.stdout.take().map(BufReader::new);
    let stderr = child.stderr.take().map(BufReader::new);

    // Each message: (line_text, is_stderr)
    let (tx_a, rx) = mpsc::channel::<(String, bool)>();
    let tx_b = tx_a.clone();

    let mut handles = Vec::new();

    if let Some(reader) = stdout {
        handles.push(thread::spawn(move || {
            for line in reader.lines().map_while(Result::ok) {
                println!("{line}");
                let _ = tx_a.send((line, false));
            }
        }));
    } else {
        drop(tx_a);
    }

    if let Some(reader) = stderr {
        handles.push(thread::spawn(move || {
            for line in reader.lines().map_while(Result::ok) {
                eprintln!("{line}");
                let _ = tx_b.send((line, true));
            }
        }));
    } else {
        drop(tx_b);
    }

    // Collect all lines (approximate arrival order).
    let all_lines: Vec<String> = rx.into_iter().map(|(l, _)| l).collect();

    for h in handles {
        let _ = h.join();
    }

    let status = child.wait();
    let exit_ok = status.map(|s| s.success()).unwrap_or(false);

    // Tail-cap at 1 MiB.
    let total: usize = all_lines.iter().map(|l| l.len() + 1).sum();
    let truncated = total > OUTPUT_CAP;
    let kept = if truncated {
        let mut lines = all_lines;
        let mut cur = total;
        while cur > OUTPUT_CAP && !lines.is_empty() {
            cur -= lines.remove(0).len() + 1;
        }
        lines
    } else {
        all_lines
    };

    (kept.join("\n"), truncated, exit_ok)
}

/// The outcome of parsing runner summaries from a transcript.
#[derive(Debug)]
enum TestCountResult {
    /// Pattern matched; sum of counts across matching lines.
    Recognized(u64),
    /// No pattern matched at all.
    Unrecognized,
}

/// Parse runner summaries from a merged transcript.
///
/// Patterns tried in order; first-match-wins:
///   1. cargo:  `(?m)^test result: \w+\. (\d+) passed`
///   2. pytest: `(?m)(\d+) passed[^\n]* in [0-9.]+s`
///   3. extra_count_patterns (user-supplied, one capture group each)
pub(crate) fn parse_test_count(transcript: &str, extra_patterns: &[String]) -> TestCountResult {
    use regex::Regex;

    // 1. cargo
    {
        let re = Regex::new(r"(?m)^test result: \w+\. (\d+) passed").unwrap();
        let caps: Vec<_> = re.captures_iter(transcript).collect();
        if !caps.is_empty() {
            let sum: u64 = caps
                .iter()
                .filter_map(|c| c.get(1)?.as_str().parse::<u64>().ok())
                .sum();
            return TestCountResult::Recognized(sum);
        }
    }

    // 2. pytest (no === fence anchor; pytest -q compatible)
    {
        let re = Regex::new(r"(?m)(\d+) passed[^\n]* in [0-9.]+s").unwrap();
        let caps: Vec<_> = re.captures_iter(transcript).collect();
        if !caps.is_empty() {
            let sum: u64 = caps
                .iter()
                .filter_map(|c| c.get(1)?.as_str().parse::<u64>().ok())
                .sum();
            return TestCountResult::Recognized(sum);
        }
    }

    // 3. extra_count_patterns (user-supplied)
    for pat in extra_patterns {
        match Regex::new(pat) {
            Err(_) => continue, // invalid patterns are skipped at runtime
            Ok(re) => {
                let caps: Vec<_> = re.captures_iter(transcript).collect();
                if !caps.is_empty() {
                    let sum: u64 = caps
                        .iter()
                        .filter_map(|c| c.get(1)?.as_str().parse::<u64>().ok())
                        .sum();
                    return TestCountResult::Recognized(sum);
                }
            }
        }
    }

    TestCountResult::Unrecognized
}

/// Core logic after capture: apply zero-test floor, emit SHADOW, return exit code.
fn apply_finish_floor(
    transcript: String,
    truncated: bool,
    exit_ok: bool,
    cmd_str: &str,
    cfg: &config::ProjectConfig,
) -> i32 {
    let count_result = parse_test_count(&transcript, &cfg.finish_extra_count_patterns);

    // Determine shadow configured state.
    let shadow_configured = if cfg.finish_require_test_count {
        verify::ShadowConfigured::On
    } else {
        verify::ShadowConfigured::Default
    };

    // Build detail string.
    let (floor_pass, detail) = match &count_result {
        TestCountResult::Recognized(n) => {
            let trunc = if truncated {
                " (output truncated to last 1 MiB)"
            } else {
                ""
            };
            let pass = *n > 0;
            let detail = format!("recognized runner summary; count={n}{trunc}");
            (pass, detail)
        }
        TestCountResult::Unrecognized => {
            let trunc = if truncated {
                " (output truncated to last 1 MiB)"
            } else {
                ""
            };
            let detail = format!("no recognized runner summary{trunc}");
            (false, detail)
        }
    };

    // Emit SHADOW line always (whether key is on or off).
    verify::emit_shadow(
        "finish",
        "zero_test_floor",
        shadow_configured,
        None,
        Some(cmd_str),
        if floor_pass {
            verify::ShadowResult::Pass
        } else {
            verify::ShadowResult::Fail
        },
        &detail,
    );

    // Check exit code first.
    if !exit_ok {
        println!("FAIL finish gate: test command exited non-zero");
        return 1;
    }

    // Apply floor when configured.
    if cfg.finish_require_test_count && !floor_pass {
        match &count_result {
            TestCountResult::Unrecognized => {
                println!(
                    "FAIL finish gate: require_test_count=true but no recognized runner summary found; \
                     add a cargo/pytest/extra_count_patterns-matched summary line"
                );
            }
            TestCountResult::Recognized(0) => {
                println!(
                    "FAIL finish gate: require_test_count=true but recognized summary shows 0 tests; \
                     run a non-empty test suite"
                );
            }
            TestCountResult::Recognized(_) => unreachable!(),
        }
        return 1;
    }

    println!("PASS finish gate: test command exited 0");
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_placeholders() {
        assert_eq!(find_placeholder("step 1: TBD"), Some("tbd".into()));
        assert_eq!(
            find_placeholder("similar to Task 2"),
            Some("similar to task".into())
        );
        assert_eq!(find_placeholder("a complete, concrete plan"), None);
    }

    #[test]
    fn ignores_placeholders_in_comments() {
        let t = "real step\n<!-- no TBD or implement later here -->\nmore";
        assert_eq!(find_placeholder(t), None);
    }

    #[test]
    fn routes_on_keyword() {
        let raw = r#"{ "skills": { "write-plan": { "enforcement": "require",
            "promptTriggers": { "keywords": ["plan", "breakdown"] } } } }"#;
        let v: serde_json::Value = serde_json::from_str(raw).unwrap();
        let m = route(&v, "can you plan this feature");
        assert_eq!(m, vec![("write-plan".to_string(), "require".to_string())]);
        assert!(route(&v, "unrelated request").is_empty());
    }

    #[test]
    fn reads_description_frontmatter() {
        let dir = env::temp_dir().join("topology_test_skill");
        let _ = fs::create_dir_all(&dir);
        let md = dir.join("SKILL.md");
        fs::write(
            &md,
            "---\nname: x\ndescription: Do a thing. Use when needed.\n---\nbody",
        )
        .unwrap();
        assert_eq!(
            read_description(&md).as_deref(),
            Some("Do a thing. Use when needed.")
        );
    }

    // resolve_root tests (new 4-argument pure signature).
    // Each uses a distinct hard-coded tempdir subdir so reruns are clean.
    //
    // NOTE: tests that previously passed because of the cwd marker walk have been
    // rewritten to pin roots explicitly (via env_override or fixture layout) — the old
    // cwd walk is removed by design; weakening assertions would hide the spec change.

    #[test]
    fn resolve_root_fallback_when_no_candidate_matches() {
        // No env_override, no marked project_root, no exe nearby, no ~/.topology →
        // returns start with source Fallback.
        // (Previously "hijack regression" relied on the cwd walk returning start when a
        // stray skills/ had no marker — now the fallback is the guaranteed path when
        // nothing resolves, regardless of ancestry.)
        let base = env::temp_dir().join("topology_resolve_root_hijack");
        let _ = fs::remove_dir_all(&base);
        let start = base.join("project");
        fs::create_dir_all(&start).unwrap();
        // Empty home — no ~/.topology
        let fake_home = base.join("home");
        fs::create_dir_all(&fake_home).unwrap();

        let result = resolve_root(&start, None, None, Some(&fake_home));
        assert_eq!(
            result.source,
            RootSource::Fallback,
            "no candidate → Fallback"
        );
        assert_eq!(
            fs::canonicalize(&start).unwrap(),
            fs::canonicalize(&result.path).unwrap(),
            "Fallback must return start"
        );
    }

    #[test]
    fn resolve_root_self_governed_when_project_root_is_marked() {
        // The project root (nearest .git ancestor) is itself a marked root →
        // SelfGoverned, returns the project root.
        // (Previously "marked_direct" relied on the cwd walk finding a marked dir; now
        // the self-governed step covers this when the marked dir also has .git.)
        // Pin: we pass start = <base> and the project root resolves to <base> (no .git
        // above it), AND we make <base> a marked root; since project_root falls back to
        // start when no .git is found, and start IS the marked root → SelfGoverned.
        // Actually: project_root() uses resolve_project_root(start); with no .git it
        // falls back to start. So if start is a marked root, step 2 wins.
        let base = env::temp_dir().join("topology_resolve_root_marked");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("skills")).unwrap();
        fs::write(base.join("AGENTS.md"), "").unwrap();
        let fake_home = base.parent().unwrap().join("home_marked");
        fs::create_dir_all(&fake_home).unwrap();

        let result = resolve_root(&base, None, None, Some(&fake_home));
        assert_eq!(
            result.source,
            RootSource::SelfGoverned,
            "marked project root must produce SelfGoverned"
        );
        assert_eq!(
            fs::canonicalize(&base).unwrap(),
            fs::canonicalize(&result.path).unwrap(),
            "SelfGoverned must return the project root"
        );
    }

    #[test]
    fn resolve_root_nested_start_uses_env_override() {
        // Running from a subdir with TOPOLOGY_ROOT pinned to the marked root → EnvOverride.
        // Previously "nested_start" relied on the cwd walk walking up to the marked root;
        // that walk is removed. Explicit pin via env_override is the supported mechanism.
        let base = env::temp_dir().join("topology_resolve_root_nested");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("skills")).unwrap();
        fs::write(base.join("AGENTS.md"), "").unwrap();
        let nested = base.join("a").join("b").join("c");
        fs::create_dir_all(&nested).unwrap();
        let fake_home = base.parent().unwrap().join("home_nested");
        fs::create_dir_all(&fake_home).unwrap();

        // Pin via env_override (the supported way to point at a marked root from a sub-dir).
        let result = resolve_root(&nested, Some(&base), None, Some(&fake_home));
        assert_eq!(
            result.source,
            RootSource::EnvOverride,
            "env_override must win — use TOPOLOGY_ROOT when running from a sub-dir"
        );
        assert_eq!(
            fs::canonicalize(&base).unwrap(),
            fs::canonicalize(&result.path).unwrap(),
            "EnvOverride must return the pinned root"
        );
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn resolve_root_env_override_wins() {
        // A valid env_override is returned as EnvOverride regardless of other candidates.
        let base = env::temp_dir().join("topology_resolve_root_override");
        let _ = fs::remove_dir_all(&base);
        let start = base.join("start");
        fs::create_dir_all(&start).unwrap();
        // override dir: a different valid directory
        let override_dir = base.join("override_dir");
        fs::create_dir_all(&override_dir).unwrap();
        let fake_home = base.join("home_override");
        fs::create_dir_all(&fake_home).unwrap();

        let result = resolve_root(&start, Some(&override_dir), None, Some(&fake_home));
        assert_eq!(
            result.source,
            RootSource::EnvOverride,
            "valid env override must produce EnvOverride"
        );
        assert_eq!(
            fs::canonicalize(&override_dir).unwrap(),
            fs::canonicalize(&result.path).unwrap(),
            "EnvOverride must return the override directory"
        );
    }

    #[test]
    fn resolve_root_env_override_invalid_ignored() {
        // A non-existent env_override is ignored; fallback applies when nothing else resolves.
        let base = env::temp_dir().join("topology_resolve_root_override_invalid");
        let _ = fs::remove_dir_all(&base);
        let start = base.join("project");
        fs::create_dir_all(&start).unwrap();
        let nonexistent = base.join("does_not_exist");
        let fake_home = base.join("home_invalid");
        fs::create_dir_all(&fake_home).unwrap();

        let result = resolve_root(&start, Some(&nonexistent), None, Some(&fake_home));
        assert_eq!(
            result.source,
            RootSource::Fallback,
            "non-existent env override must be ignored; must fall back"
        );
        assert_eq!(
            fs::canonicalize(&start).unwrap(),
            fs::canonicalize(&result.path).unwrap(),
            "Fallback must return start"
        );
    }

    #[test]
    fn resolve_root_finds_project_vendored_topology() {
        // A governed project carries the framework at <project_root>/.topology.
        // The project_root is the nearest .git ancestor of start; since there is no
        // .git here, project_root falls back to start. So <start>/.topology is probed.
        // Pin: start == base (no .git), base/.topology is a marked root → ProjectVendored.
        let base = env::temp_dir().join("topology_vendored_root");
        let _ = fs::remove_dir_all(&base);
        let vendored = base.join(".topology");
        fs::create_dir_all(vendored.join("skills")).unwrap();
        fs::write(vendored.join("AGENTS.md"), "marker\n").unwrap();
        // NOTE: previously the test started from base/src/deep and relied on the cwd
        // walk finding <base>/.topology; after the rewrite only the project_root's
        // direct .topology child is probed (Q4 decision). We test from base itself so
        // project_root == base and step 4 finds base/.topology.
        let fake_home = base.parent().unwrap().join("home_vendored");
        fs::create_dir_all(&fake_home).unwrap();

        let result = resolve_root(&base, None, None, Some(&fake_home));
        assert_eq!(
            result.source,
            RootSource::ProjectVendored,
            "project_root/.topology must resolve as ProjectVendored"
        );
        assert_eq!(
            fs::canonicalize(&vendored).unwrap(),
            fs::canonicalize(&result.path).unwrap(),
            "ProjectVendored must return the .topology path"
        );
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn resolve_root_self_governed_beats_project_vendored_topology() {
        // A directory that is itself a marked root (SelfGoverned, step 2) wins over
        // a .topology inside it (ProjectVendored, step 4) — the framework repo must
        // keep resolving to itself even if a stray .topology clone appears in its tree.
        // Previously tested as "prefers_marked_dir_over_its_own_vendored_topology"
        // with the cwd-walk; now the SelfGoverned step provides the same guarantee.
        let base = env::temp_dir().join("topology_vendored_precedence");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("skills")).unwrap();
        fs::write(base.join("AGENTS.md"), "marker\n").unwrap();
        let vendored = base.join(".topology");
        fs::create_dir_all(vendored.join("skills")).unwrap();
        fs::write(vendored.join("AGENTS.md"), "marker\n").unwrap();
        let fake_home = base.parent().unwrap().join("home_prec");
        fs::create_dir_all(&fake_home).unwrap();

        let result = resolve_root(&base, None, None, Some(&fake_home));
        assert_eq!(
            result.source,
            RootSource::SelfGoverned,
            "marked project root must win (SelfGoverned beats ProjectVendored)"
        );
        assert_eq!(
            fs::canonicalize(&base).unwrap(),
            fs::canonicalize(&result.path).unwrap(),
            "SelfGoverned must return the project root, not its .topology child"
        );
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn resolve_root_global_home_topology_found() {
        // When no other step resolves, <home>/.topology that is a marked root → GlobalHome.
        let base = env::temp_dir().join("topology_global_home");
        let _ = fs::remove_dir_all(&base);
        let start = base.join("project");
        fs::create_dir_all(&start).unwrap();
        let fake_home = base.join("home");
        let global_topology = fake_home.join(".topology");
        fs::create_dir_all(global_topology.join("skills")).unwrap();
        fs::write(global_topology.join("AGENTS.md"), "marker\n").unwrap();

        let result = resolve_root(&start, None, None, Some(&fake_home));
        assert_eq!(
            result.source,
            RootSource::GlobalHome,
            "<home>/.topology must resolve as GlobalHome"
        );
        assert_eq!(
            fs::canonicalize(&global_topology).unwrap(),
            fs::canonicalize(&result.path).unwrap(),
            "GlobalHome must return <home>/.topology"
        );
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn resolve_root_binary_adjacent_from_bin_layout() {
        // Binary at <root>/bin/gatekeeper; walk up from the exe dir finds the marked root.
        let base = env::temp_dir().join("topology_bin_adjacent");
        let _ = fs::remove_dir_all(&base);
        let root = base.join("root");
        fs::create_dir_all(root.join("skills")).unwrap();
        fs::write(root.join("AGENTS.md"), "marker\n").unwrap();
        // Simulate exe at <root>/bin/gatekeeper
        let bin_dir = root.join("bin");
        fs::create_dir_all(&bin_dir).unwrap();
        let fake_exe = bin_dir.join("gatekeeper");
        fs::write(&fake_exe, "").unwrap();

        let start = base.join("cwd");
        fs::create_dir_all(&start).unwrap();
        let fake_home = base.join("home");
        fs::create_dir_all(&fake_home).unwrap();

        let result = resolve_root(&start, None, Some(&fake_exe), Some(&fake_home));
        assert_eq!(
            result.source,
            RootSource::BinaryAdjacent,
            "exe at <root>/bin/gatekeeper must produce BinaryAdjacent"
        );
        assert_eq!(
            fs::canonicalize(&root).unwrap(),
            fs::canonicalize(&result.path).unwrap(),
            "BinaryAdjacent must return the marked root above bin/"
        );
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn resolve_root_q4_nested_dir_gitless_project_only_probes_cwd_topology() {
        // Q4: with no .git, project_root falls back to start (cwd). Only
        // <start>/.topology is probed in step 4 — a .topology on a deeper unrelated
        // ancestor does NOT win.
        let base = env::temp_dir().join("topology_q4_gitless");
        let _ = fs::remove_dir_all(&base);

        // An ancestor has .topology (simulates an unrelated project above cwd).
        let ancestor_topology = base.join(".topology");
        fs::create_dir_all(ancestor_topology.join("skills")).unwrap();
        fs::write(ancestor_topology.join("AGENTS.md"), "marker\n").unwrap();

        // Cwd is a sub-directory; it has no .git and no .topology of its own.
        let start = base.join("sub").join("project");
        fs::create_dir_all(&start).unwrap();
        let fake_home = base.join("home");
        fs::create_dir_all(&fake_home).unwrap();

        let result = resolve_root(&start, None, None, Some(&fake_home));
        // The ancestor's .topology must NOT resolve — only <start>/.topology would be
        // probed (and it doesn't exist), so we fall back.
        assert_eq!(
            result.source,
            RootSource::Fallback,
            "Q4: ancestor .topology must not win; only project_root/.topology is probed"
        );
        let _ = fs::remove_dir_all(&base);
    }

    // ── resolve_project_root tests ────────────────────────────────────────────

    #[test]
    fn project_root_git_dir_found() {
        // A .git directory at 'base' → returns base.
        let base = env::temp_dir().join("topology_prj_root_dir");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join(".git")).unwrap();

        let result = resolve_project_root(&base);
        assert_eq!(
            fs::canonicalize(&base).unwrap(),
            fs::canonicalize(&result).unwrap(),
            ".git dir should be found"
        );
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn project_root_git_file_found() {
        // A .git FILE (worktree) at 'base' → returns base.
        let base = env::temp_dir().join("topology_prj_root_file");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        fs::write(base.join(".git"), "gitdir: /some/path\n").unwrap();

        let result = resolve_project_root(&base);
        assert_eq!(
            fs::canonicalize(&base).unwrap(),
            fs::canonicalize(&result).unwrap(),
            ".git file (worktree) should be found"
        );
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn project_root_no_git_returns_start() {
        // No .git anywhere in the chain → returns start.
        let base = env::temp_dir().join("topology_prj_root_none");
        let _ = fs::remove_dir_all(&base);
        let start = base.join("deeply").join("nested");
        fs::create_dir_all(&start).unwrap();

        let result = resolve_project_root(&start);
        assert_eq!(
            fs::canonicalize(&start).unwrap(),
            fs::canonicalize(&result).unwrap(),
            "no .git → fallback to start"
        );
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn project_root_walks_up_to_git() {
        // .git is at 'base', start is a nested subdir → walks up.
        let base = env::temp_dir().join("topology_prj_root_walk");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join(".git")).unwrap();
        let start = base.join("src").join("deeply").join("nested");
        fs::create_dir_all(&start).unwrap();

        let result = resolve_project_root(&start);
        assert_eq!(
            fs::canonicalize(&base).unwrap(),
            fs::canonicalize(&result).unwrap(),
            "nested start must walk up to the .git root"
        );
        let _ = fs::remove_dir_all(&base);
    }

    // ── resolve_artifacts_root tests ─────────────────────────────────────────

    #[test]
    fn artifacts_root_equal_roots_yields_docs() {
        // project == framework → project/docs
        let base = env::temp_dir().join("topology_artifacts_equal");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();

        let result = resolve_artifacts_root(&base, &base);
        assert_eq!(result, base.join("docs"), "equal roots → docs/");
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn artifacts_root_differing_roots_yields_claude_topology() {
        // project != framework → project/.claude/topology
        let base = env::temp_dir().join("topology_artifacts_diff");
        let _ = fs::remove_dir_all(&base);
        let project = base.join("project");
        let framework = base.join("framework");
        fs::create_dir_all(&project).unwrap();
        fs::create_dir_all(&framework).unwrap();

        let result = resolve_artifacts_root(&project, &framework);
        assert_eq!(
            result,
            project.join(".claude").join("topology"),
            "differing roots → .claude/topology/"
        );
        let _ = fs::remove_dir_all(&base);
    }

    // ── word-boundary keyword matching (issue #23, Part A) ───────────────────

    fn finish_branch_rules() -> serde_json::Value {
        serde_json::from_str(
            r#"{ "skills": { "finish-branch": { "enforcement": "suggest",
                "promptTriggers": { "keywords": ["merge", "pr", "pull request",
                    "wrap up", "ship", "cleanup", "close out"] } } } }"#,
        )
        .unwrap()
    }

    /// "approach" must NOT route finish-branch (substring of "approach" is "pr")
    #[test]
    fn word_boundary_approach_does_not_match_pr() {
        let rules = finish_branch_rules();
        assert!(
            route(&rules, "approach").is_empty(),
            "'approach' must not match keyword 'pr'"
        );
    }

    /// "print the report" must NOT route finish-branch
    #[test]
    fn word_boundary_print_does_not_match_pr() {
        let rules = finish_branch_rules();
        assert!(
            route(&rules, "print the report").is_empty(),
            "'print the report' must not match keyword 'pr'"
        );
    }

    /// The bare word "prompt" that appears in the JSON envelope must NOT route finish-branch
    #[test]
    fn word_boundary_prompt_does_not_match_pr() {
        let rules = finish_branch_rules();
        assert!(
            route(&rules, "prompt").is_empty(),
            "'prompt' must not match keyword 'pr'"
        );
    }

    /// "open a pr" MUST still route finish-branch
    #[test]
    fn word_boundary_open_a_pr_routes_finish_branch() {
        let rules = finish_branch_rules();
        assert_eq!(
            route(&rules, "open a pr"),
            vec![("finish-branch".to_string(), "suggest".to_string())],
            "'open a pr' must match keyword 'pr'"
        );
    }

    /// "raise a PR" (upper-case) MUST still route finish-branch
    #[test]
    fn word_boundary_raise_a_pr_routes_finish_branch() {
        let rules = finish_branch_rules();
        assert_eq!(
            route(&rules, "raise a pr"),
            vec![("finish-branch".to_string(), "suggest".to_string())],
            "'raise a PR' must match keyword 'pr'"
        );
    }

    // ── JSON envelope extraction (issue #23, Part B) ─────────────────────────

    /// Helper: extract and fall back to the original string (mirrors cmd_activate logic).
    fn do_extract(raw: &str) -> String {
        extract_prompt_owned(raw).unwrap_or_else(|| raw.to_owned())
    }

    /// Plain text is returned as-is (no JSON envelope detected).
    #[test]
    fn extract_prompt_plain_text_unchanged() {
        assert_eq!(do_extract("implement the input"), "implement the input");
    }

    /// A JSON envelope {"prompt":"..."} returns only the string value.
    #[test]
    fn extract_prompt_unwraps_json_envelope() {
        assert_eq!(do_extract(r#"{"prompt":"implement X"}"#), "implement X");
    }

    /// JSON envelope routes the same skills as bare text (no spurious finish-branch match).
    #[test]
    fn json_envelope_routes_same_as_plain_text() {
        let rules = finish_branch_rules();
        // Bare text: "implement the input" — no finish-branch keywords
        let plain = route(&rules, &do_extract("implement the input").to_lowercase());
        // JSON envelope wrapping the same text
        let enveloped = route(
            &rules,
            &do_extract(r#"{"prompt":"implement the input"}"#).to_lowercase(),
        );
        assert_eq!(
            plain, enveloped,
            "JSON envelope must route identically to bare text"
        );
        assert!(
            plain.is_empty(),
            "'implement the input' must not route finish-branch"
        );
    }

    /// JSON envelope with a PR prompt correctly routes finish-branch.
    #[test]
    fn json_envelope_with_pr_routes_finish_branch() {
        let rules = finish_branch_rules();
        let matched = route(
            &rules,
            &do_extract(r#"{"prompt":"open a pr for this change"}"#).to_lowercase(),
        );
        assert_eq!(
            matched,
            vec![("finish-branch".to_string(), "suggest".to_string())],
            "JSON envelope containing 'open a pr' must route finish-branch"
        );
    }

    /// Non-JSON that starts with something other than '{' is returned unchanged.
    #[test]
    fn extract_prompt_non_json_returned_unchanged() {
        let input = "just a plain string without braces";
        assert_eq!(do_extract(input), input);
    }

    /// JSON object without a "prompt" field is returned as the raw string.
    #[test]
    fn extract_prompt_json_without_prompt_key_returns_raw() {
        let input = r#"{"message":"hello"}"#;
        assert_eq!(do_extract(input), input);
    }
}
