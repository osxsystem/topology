//! Topology gatekeeper — routes skills and enforces methodology gates.
//!
//! Subcommands:
//!   gatekeeper list                         List skills + descriptions.
//!   gatekeeper activate                     Read a prompt on stdin, print routed skills.
//!   gatekeeper check design  --feature S    Design gate: a spec doc exists.
//!   gatekeeper check plan    --feature S    Plan gate: a placeholder-free plan exists.
//!   gatekeeper check verify  --feature S    Verify gate: a verification note exists.
//!   gatekeeper check review  --feature S    Review gate: a fresh critic's artifact passes.
//!   gatekeeper check finish  -- <cmd...>    Finish gate: <cmd> exits 0.
//!   gatekeeper scan --hook                  Security-scan a PreToolUse event (stdin); emit the decision.
//!   gatekeeper scan --cmd | --content       Security-scan a command / file image on stdin.
//!   gatekeeper scan --staged                Pre-commit: scan staged blobs + enforce integrity.
//!   gatekeeper scan --check-path <path>     Exit 1 iff <path> is a protected safety file.
//!   gatekeeper instinct list                List always-on instincts (id + priority).
//!   gatekeeper instinct render [--harness H] [--budget N]   Render the always-on preamble subset.
//!
//! Built offline from a small, vetted dependency set (regex, serde, serde_json, toml); ships as
//! one static binary. See docs/adr/0007-security-scanner-dependencies.md.

use std::env;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{exit, Command};

mod instinct;
mod review;
mod scan;

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
        Some("scan") => scan::cmd_scan(&args[1..], &framework_root()),
        Some("instinct") => instinct::cmd_instinct(&args[1..], &framework_root()),
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
        "topology gatekeeper\n\n\
         USAGE:\n  \
         gatekeeper list\n  \
         gatekeeper activate            (reads prompt on stdin)\n  \
         gatekeeper check design --feature <slug>\n  \
         gatekeeper check plan   --feature <slug>\n  \
         gatekeeper check verify --feature <slug>\n  \
         gatekeeper check review --feature <slug> [--base <ref>]\n  \
         gatekeeper check finish -- <command...>\n  \
         gatekeeper scan --hook | --cmd | --content       (reads stdin)\n  \
         gatekeeper scan --staged | --check-path <path>\n  \
         gatekeeper instinct list\n  \
         gatekeeper instinct render [--harness <h>] [--budget <n>]\n"
    );
}

/// Locate the framework root by walking up from cwd looking for a `skills/` dir.
fn framework_root() -> PathBuf {
    let mut dir = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    loop {
        if dir.join("skills").is_dir() {
            return dir;
        }
        if !dir.pop() {
            return env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        }
    }
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
        "design" => gate_doc_exists("specs", &feature_arg(args)),
        "plan" => gate_plan(&feature_arg(args)),
        "verify" => gate_doc_exists("verify", &feature_arg(args)),
        "finish" => gate_finish(args),
        "review" => review::gate_review(
            &framework_root(),
            &feature_arg(args),
            base_arg(args).as_deref(),
        ),
        other => {
            eprintln!("gatekeeper check: unknown gate '{other}'");
            2
        }
    }
}

fn feature_arg(args: &[String]) -> String {
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

/// Find a markdown doc under docs/<sub>/ whose filename contains the feature slug.
fn find_doc(sub: &str, feature: &str) -> Option<PathBuf> {
    if feature.is_empty() {
        return None;
    }
    let dir = framework_root().join("docs").join(sub);
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

fn gate_doc_exists(sub: &str, feature: &str) -> i32 {
    if feature.is_empty() {
        eprintln!("gatekeeper: --feature <slug> is required");
        return 2;
    }
    match find_doc(sub, feature) {
        Some(p) => {
            println!("PASS {sub} gate: {}", p.display());
            0
        }
        None => {
            println!("FAIL {sub} gate: no docs/{sub}/*{feature}*.md found");
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
        println!("FAIL plan gate: no docs/plans/*{feature}*.md found");
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
}
