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

mod adapt;
mod doctor;
mod instinct;
mod learn;
mod memory;
mod review;
mod scan;
mod tdd;
mod version;

const PLACEHOLDERS: &[&str] = &[
    "tbd",
    "implement later",
    "similar to task",
    "appropriate validation",
    "to be determined",
    "fill in later",
];

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let code = match args.first().map(String::as_str) {
        Some("list") => cmd_list(),
        Some("activate") => cmd_activate(),
        Some("check") => cmd_check(&args[1..]),
        Some("scan") => scan::cmd_scan(
            &args[1..],
            &framework_root(),
            &artifacts_root(),
            &project_root(),
        ),
        Some("instinct") => instinct::cmd_instinct(&args[1..], &framework_root()),
        Some("adapt") => adapt::cmd_adapt(&args[1..], &framework_root(), &project_root()),
        Some("learn") => learn::cmd_learn(&args[1..], &artifacts_root(), &framework_root()),
        Some("memory") => memory::cmd_memory(
            &args[1..],
            &artifacts_root(),
            &framework_root(),
            &project_root(),
        ),
        Some("doctor") => doctor::cmd_doctor(&framework_root()),
        Some("--version") | Some("-V") => {
            println!(
                "gatekeeper {} (rules schema v{})",
                version::tool(),
                version::rules_schema()
            );
            0
        }
        Some("--help") | Some("-h") | None => {
            print_help();
            0
        }
        Some(other) => {
            eprintln!("gatekeeper: unknown command '{other}'\n");
            print_help();
            2
        }
    };
    exit(code);
}

fn print_help() {
    println!(
        "topology gatekeeper {} (rules schema v{})\n\n\
         USAGE:\n  \
         gatekeeper list\n  \
         gatekeeper activate            (reads prompt on stdin)\n  \
         gatekeeper check research --feature <slug>\n  \
         gatekeeper check design --feature <slug>\n  \
         gatekeeper check plan   --feature <slug>\n  \
         gatekeeper check verify --feature <slug>\n  \
         gatekeeper check tdd    --feature <slug> [--base <ref>]\n  \
         gatekeeper check review --feature <slug> [--base <ref>]\n  \
         gatekeeper check finish -- <command...>\n  \
         gatekeeper check docs\n  \
         gatekeeper scan --hook | --cmd | --content       (reads stdin)\n  \
         gatekeeper scan --staged | --check-path <path>\n  \
         gatekeeper instinct list\n  \
         gatekeeper instinct render [--harness <h>] [--budget <n>]\n  \
         gatekeeper adapt --harness <codex|cursor|opencode|claude> [--check]\n  \
         gatekeeper learn capture --summary <text> [--trigger <t>] [--gate <g>] [--kind <k>]\n  \
         gatekeeper learn list\n  \
         gatekeeper learn promote --id <id> [--kind <k>] [--yes]\n  \
         gatekeeper memory write --feature <slug> --date <YYYY-MM-DD>  (reads body on stdin)\n  \
         gatekeeper memory read  --feature <slug>\n  \
         gatekeeper memory list\n  \
         gatekeeper doctor\n",
        version::tool(),
        version::rules_schema()
    );
}

const ROOT_MARKERS: &[&str] = &["AGENTS.md", "gatekeeper", ".claude-plugin"];

fn is_marked_root(dir: &Path) -> bool {
    dir.join("skills").is_dir() && ROOT_MARKERS.iter().any(|m| dir.join(m).exists())
}

fn resolve_root(start: &Path, env_override: Option<&Path>) -> PathBuf {
    if let Some(o) = env_override {
        if o.is_dir() {
            return o.to_path_buf();
        }
    }
    let mut dir = start.to_path_buf();
    loop {
        if is_marked_root(&dir) {
            return dir;
        }
        // Vendored install: `install.sh --project <path>` places the framework at
        // <project>/.topology. Recognize it during the walk-up so a plain `gatekeeper <cmd>`
        // from anywhere inside the project finds the framework without TOPOLOGY_ROOT. A dir
        // that is itself a marked root wins over its own .topology (checked above first).
        let vendored = dir.join(".topology");
        if is_marked_root(&vendored) {
            return vendored;
        }
        if !dir.pop() {
            return start.to_path_buf();
        }
    }
}

/// Locate the framework root by walking up from cwd looking for a marked Topology root.
fn framework_root() -> PathBuf {
    let start = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let env_override = env::var_os("TOPOLOGY_ROOT").map(PathBuf::from);
    resolve_root(&start, env_override.as_deref())
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

fn cmd_list() -> i32 {
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

fn cmd_activate() -> i32 {
    let mut prompt = String::new();
    if std::io::stdin().read_to_string(&mut prompt).is_err() {
        eprintln!("gatekeeper: failed to read stdin");
        return 1;
    }
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
                .any(|k| prompt_lc.contains(&k.to_lowercase()));
            if hit {
                out.push((name.clone(), enforcement));
            }
        }
    }
    out.sort();
    out
}

fn cmd_check(args: &[String]) -> i32 {
    let Some(gate) = args.first().map(String::as_str) else {
        eprintln!("gatekeeper check: missing gate name");
        return 2;
    };
    match gate {
        "research" => gate_doc_exists("research", "research", &feature_arg(args)),
        "design" => {
            let f = feature_arg(args);
            if f.is_empty() {
                // A missing --feature is a usage error (exit 2), like every other gate via
                // gate_doc_exists — not a research-first failure. Guard before find_doc, whose
                // empty-slug None would otherwise misroute this into the lock branch (exit 1).
                eprintln!("gatekeeper: --feature <slug> is required");
                return 2;
            }
            match find_doc("research", &f) {
                None => {
                    let dir = artifacts_root().join("research");
                    println!(
                        "FAIL design gate: research-first — no {}/*{f}*.md",
                        dir.display()
                    );
                    1
                }
                Some(_) => gate_doc_exists("design", "specs", &f),
            }
        }
        "plan" => gate_plan(&feature_arg(args)),
        "verify" => gate_doc_exists("verify", "verify", &feature_arg(args)),
        "tdd" => tdd::gate_tdd(
            &project_root(),
            &feature_arg(args),
            base_arg(args).as_deref(),
        ),
        "finish" => gate_finish(args),
        "review" => review::gate_review(
            &project_root(),
            &artifacts_root(),
            &feature_arg(args),
            base_arg(args).as_deref(),
        ),
        "docs" => check_docs(&framework_root()),
        other => {
            eprintln!("gatekeeper check: unknown gate '{other}'");
            2
        }
    }
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

pub(crate) fn feature_arg(args: &[String]) -> String {
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "--feature" {
            return it.next().cloned().unwrap_or_default();
        }
    }
    String::new()
}

fn base_arg(args: &[String]) -> Option<String> {
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

fn gate_finish(args: &[String]) -> i32 {
    let cmd: Vec<&String> = args.iter().skip_while(|a| *a != "--").skip(1).collect();
    if cmd.is_empty() {
        eprintln!("gatekeeper check finish -- <command...>  (command required)");
        eprintln!("  The finish gate runs your full test command and passes when it exits 0:");
        eprintln!("    gatekeeper check finish -- npm test");
        eprintln!("    gatekeeper check finish -- cargo test");
        return 2;
    }
    let status = Command::new(cmd[0]).args(&cmd[1..]).status();
    match status {
        Ok(s) if s.success() => {
            println!("PASS finish gate: test command exited 0");
            0
        }
        Ok(s) => {
            println!(
                "FAIL finish gate: test command exited {}",
                s.code().unwrap_or(-1)
            );
            1
        }
        Err(e) => {
            println!("FAIL finish gate: could not run command: {e}");
            1
        }
    }
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

    // resolve_root tests — each uses a distinct hard-coded tempdir subdir so reruns are clean.

    #[test]
    fn resolve_root_hijack_regression() {
        // A chain whose only skills/ dir has NO marker → returns start (not the stray dir).
        let base = env::temp_dir().join("topology_resolve_root_hijack");
        let _ = fs::remove_dir_all(&base);
        // stray parent: has skills/ but no marker
        let stray = base.join("stray");
        fs::create_dir_all(stray.join("skills")).unwrap();
        // start is a subdir inside stray
        let start = stray.join("project");
        fs::create_dir_all(&start).unwrap();

        let result = resolve_root(&start, None);
        assert_eq!(
            fs::canonicalize(&start).unwrap(),
            fs::canonicalize(&result).unwrap(),
            "stray skills/ without marker must not hijack"
        );
    }

    #[test]
    fn resolve_root_marked_direct() {
        // A dir containing both skills/ and AGENTS.md → returns that dir.
        let base = env::temp_dir().join("topology_resolve_root_marked");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("skills")).unwrap();
        fs::write(base.join("AGENTS.md"), "").unwrap();

        let result = resolve_root(&base, None);
        assert_eq!(
            fs::canonicalize(&base).unwrap(),
            fs::canonicalize(&result).unwrap(),
            "marked root must be returned directly"
        );
    }

    #[test]
    fn resolve_root_nested_start() {
        // Starting from a nested subdir of a marked root → walks up and returns the marked root.
        let base = env::temp_dir().join("topology_resolve_root_nested");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("skills")).unwrap();
        fs::write(base.join("AGENTS.md"), "").unwrap();
        let nested = base.join("a").join("b").join("c");
        fs::create_dir_all(&nested).unwrap();

        let result = resolve_root(&nested, None);
        assert_eq!(
            fs::canonicalize(&base).unwrap(),
            fs::canonicalize(&result).unwrap(),
            "nested start must walk up to the marked root"
        );
    }

    #[test]
    fn resolve_root_env_override_wins() {
        // A valid env_override is returned regardless of what the walk-up would find.
        let base = env::temp_dir().join("topology_resolve_root_override");
        let _ = fs::remove_dir_all(&base);
        // walk-up target: has a marked root
        let marked = base.join("marked");
        fs::create_dir_all(marked.join("skills")).unwrap();
        fs::write(marked.join("AGENTS.md"), "").unwrap();
        let start = marked.join("sub");
        fs::create_dir_all(&start).unwrap();
        // override dir: a different valid directory
        let override_dir = base.join("override_dir");
        fs::create_dir_all(&override_dir).unwrap();

        let result = resolve_root(&start, Some(&override_dir));
        assert_eq!(
            fs::canonicalize(&override_dir).unwrap(),
            fs::canonicalize(&result).unwrap(),
            "valid env override must win over walk-up"
        );
    }

    #[test]
    fn resolve_root_env_override_invalid_ignored() {
        // A non-existent env_override is ignored; walk-up/fallback applies.
        let base = env::temp_dir().join("topology_resolve_root_override_invalid");
        let _ = fs::remove_dir_all(&base);
        // No marked root anywhere — fallback to start
        let start = base.join("project");
        fs::create_dir_all(&start).unwrap();
        let nonexistent = base.join("does_not_exist");

        let result = resolve_root(&start, Some(&nonexistent));
        assert_eq!(
            fs::canonicalize(&start).unwrap(),
            fs::canonicalize(&result).unwrap(),
            "non-existent env override must be ignored; fallback to start"
        );
    }

    #[test]
    fn resolve_root_finds_vendored_topology() {
        // A governed project carries the framework at <project>/.topology (install.sh
        // --project). Walk-up from anywhere inside the project must find it without
        // TOPOLOGY_ROOT — this was the live-test S1.3 failure: doctor from the project
        // root probed the project itself and reported 3 failures.
        let base = env::temp_dir().join("topology_vendored_root");
        let _ = fs::remove_dir_all(&base);
        let vendored = base.join(".topology");
        fs::create_dir_all(vendored.join("skills")).unwrap();
        fs::write(vendored.join("AGENTS.md"), "marker\n").unwrap();
        let nested = base.join("src").join("deep");
        fs::create_dir_all(&nested).unwrap();

        for start in [&base, &nested] {
            let result = resolve_root(start, None);
            assert_eq!(
                fs::canonicalize(&vendored).unwrap(),
                fs::canonicalize(&result).unwrap(),
                "walk-up from {} must find the vendored .topology",
                start.display()
            );
        }
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn resolve_root_prefers_marked_dir_over_its_own_vendored_topology() {
        // A directory that is itself a marked root wins over a .topology inside it —
        // the framework repo must keep resolving to itself even if a stray .topology
        // clone appears in its tree.
        let base = env::temp_dir().join("topology_vendored_precedence");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("skills")).unwrap();
        fs::write(base.join("AGENTS.md"), "marker\n").unwrap();
        let vendored = base.join(".topology");
        fs::create_dir_all(vendored.join("skills")).unwrap();
        fs::write(vendored.join("AGENTS.md"), "marker\n").unwrap();

        let result = resolve_root(&base, None);
        assert_eq!(
            fs::canonicalize(&base).unwrap(),
            fs::canonicalize(&result).unwrap(),
            "a marked dir must win over its own vendored .topology"
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
}
