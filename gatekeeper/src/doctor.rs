//! `gatekeeper doctor` — read-only health check with binary-resolution transparency.
//!
//! Prints one line per probe with an `ok` / `FAIL` / `n/a` tag, then a summary line.
//! Exit 0 if all probes pass; exit 1 on any FAIL.
//! Writes nothing; framework root is resolved the same way every other command does.
//! See docs/adr/0010-packaging-distribution.md §1 and §2.

use std::fs;
use std::path::Path;

use crate::instinct;
use crate::learn;
use crate::scan;
use crate::version;

/// Run all doctor probes and print a report. Returns 0 (all ok) or 1 (any FAIL).
pub fn cmd_doctor(root: &Path) -> i32 {
    use crate::{artifacts_root, project_root};
    let mut failures = 0usize;

    // ── Two-roots transparency ───────────────────────────────────────────────
    println!("framework root: {}", root.display());
    let proj_root = project_root();
    println!("project root: {}", proj_root.display());
    let art_root = artifacts_root();
    println!("artifacts root: {}", art_root.display());

    // ── Resolution transparency ──────────────────────────────────────────────
    // Print raw facts, not a single merged order — the two hooks resolve in opposite orders.

    // This binary's own path and version.
    match std::env::current_exe() {
        Ok(p) => println!("binary: {}", p.display()),
        Err(e) => println!("binary: <unknown: {e}>"),
    }
    println!(
        "version: gatekeeper {} (rules schema v{})",
        version::tool(),
        version::rules_schema()
    );

    // $GATEKEEPER_BIN override (new this phase — neither hook reads it yet; doctor surfaces it).
    let gk_bin_env = std::env::var("GATEKEEPER_BIN").ok();
    match &gk_bin_env {
        None => println!("GATEKEEPER_BIN: not set"),
        Some(p) => {
            let path = Path::new(p);
            if path.is_file() && is_executable(path) {
                println!("GATEKEEPER_BIN: set → {} (executable)", p);
            } else {
                println!(
                    "GATEKEEPER_BIN: set → {} (FAIL: missing or not executable)",
                    p
                );
                failures += 1;
            }
        }
    }

    // gatekeeper on PATH — probe its version and note any skew (informational, not a failure).
    match which_gatekeeper() {
        Some(p) => {
            // Run the PATH binary with --version to detect skew.
            let their_version = probe_version(&p);
            let my_version = version::tool();
            if let Some(theirs) = their_version {
                if theirs != my_version {
                    println!("PATH gatekeeper: {p} (version skew: {theirs} vs {my_version})");
                } else {
                    println!("PATH gatekeeper: {p}");
                }
            } else {
                println!("PATH gatekeeper: {p}");
            }
        }
        None => println!("PATH gatekeeper: not found (informational)"),
    }

    // Repo build.
    let repo_build = find_repo_build(root);
    match &repo_build {
        Some(p) => println!("repo build: {}", p.display()),
        None => println!("repo build: not found (informational)"),
    }

    // The split: one line naming how the two hooks resolve.
    println!(
        "resolution split: both hooks try $GATEKEEPER_BIN, then prebuilt bin/ and plugin-data \
         bin/; then scan prefers the repo build and activate prefers PATH"
    );

    // ── security/rules.toml ─────────────────────────────────────────────────
    let rules_path = root.join("security").join("rules.toml");
    match scan::load_rules(&rules_path) {
        Ok(_) => println!("security/rules.toml: ok"),
        Err(e) => {
            println!("security/rules.toml: FAIL: {e}");
            failures += 1;
        }
    }

    // ── instincts/ ──────────────────────────────────────────────────────────
    let instincts_dir = root.join("instincts");
    let instinct_failures = probe_instincts(&instincts_dir);
    if instinct_failures == 0 {
        println!("instincts/: ok");
    } else {
        failures += instinct_failures;
    }

    // ── skills/*/SKILL.md ────────────────────────────────────────────────────
    let skills_dir = root.join("skills");
    let skill_failures = probe_skills(&skills_dir);
    if skill_failures == 0 {
        println!("skills/: ok");
    } else {
        failures += skill_failures;
    }

    // ── hooks/*.sh ──────────────────────────────────────────────────────────
    let hooks_dir = root.join("hooks");
    let hook_failures = probe_hooks(&hooks_dir);
    if hook_failures == 0 {
        println!("hooks/*.sh: ok");
    } else {
        failures += hook_failures;
    }

    // ── .git/hooks/pre-commit ────────────────────────────────────────────────
    // Only checked when .git/ is present. A PATH/plugin install has no .git → n/a.
    let git_dir = root.join(".git");
    if git_dir.is_dir() {
        let pc = git_dir.join("hooks").join("pre-commit");
        if pc.is_file() && is_executable(&pc) {
            println!(".git/hooks/pre-commit: ok");
        } else {
            println!(
                ".git/hooks/pre-commit: FAIL: not installed (run scripts/install.sh or \
                 gatekeeper adapt --harness claude)"
            );
            failures += 1;
        }
    } else {
        println!(".git/hooks/pre-commit: n/a (no .git directory — plugin/PATH install)");
    }

    // ── Summary ─────────────────────────────────────────────────────────────
    if failures == 0 {
        println!("doctor: all probes ok");
        0
    } else {
        println!("doctor: {failures} probe(s) FAILED");
        1
    }
}

// ── Internal helpers ─────────────────────────────────────────────────────────

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path)
        .map(|m| m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

/// Run `<bin> --version` and extract the version string (e.g. "0.2.0") from the first line.
/// Returns None if the binary can't be run or the output isn't parseable.
fn probe_version(bin: &str) -> Option<String> {
    let out = std::process::Command::new(bin)
        .arg("--version")
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    // Expected format: "gatekeeper X.Y.Z (rules schema vN)"
    // Extract the token at index 1 (the version).
    let first_line = stdout.lines().next()?;
    let version = first_line.split_whitespace().nth(1)?;
    Some(version.to_string())
}

/// Find `gatekeeper` on PATH, returning its path string.
fn which_gatekeeper() -> Option<String> {
    let path_var = std::env::var("PATH").unwrap_or_default();
    for dir in path_var.split(':') {
        let candidate = Path::new(dir).join("gatekeeper");
        if candidate.is_file() && is_executable(&candidate) {
            return Some(candidate.to_string_lossy().into_owned());
        }
    }
    None
}

/// Look for a repo-built gatekeeper binary (release first, then debug).
fn find_repo_build(root: &Path) -> Option<std::path::PathBuf> {
    let release = root
        .join("gatekeeper")
        .join("target")
        .join("release")
        .join("gatekeeper");
    if release.is_file() && is_executable(&release) {
        return Some(release);
    }
    let debug = root
        .join("gatekeeper")
        .join("target")
        .join("debug")
        .join("gatekeeper");
    if debug.is_file() && is_executable(&debug) {
        return Some(debug);
    }
    None
}

/// Parse every `instincts/*.md` file via `instinct::validate_instinct_str`.
/// Returns the number of failures (and prints a FAIL line per offender).
fn probe_instincts(dir: &Path) -> usize {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => {
            // Missing instincts dir is informational (not all setups have instincts yet).
            println!("instincts/: ok (directory absent — no instincts to check)");
            return 0;
        }
    };
    let mut fails = 0usize;
    for entry in entries.flatten() {
        let p = entry.path();
        if p.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let raw = match fs::read_to_string(&p) {
            Ok(r) => r,
            Err(e) => {
                println!(
                    "instincts/{}: FAIL: cannot read: {e}",
                    p.file_name().unwrap_or_default().to_string_lossy()
                );
                fails += 1;
                continue;
            }
        };
        if let Err(e) = instinct::validate_instinct_str(&raw) {
            println!(
                "instincts/{}: FAIL: {e}",
                p.file_name().unwrap_or_default().to_string_lossy()
            );
            fails += 1;
        }
    }
    fails
}

/// Validate every `skills/*/SKILL.md` via `learn::validate_skill_file`.
/// Returns the number of failures (and prints a FAIL line per offender).
fn probe_skills(dir: &Path) -> usize {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return 0, // Missing skills dir is already caught by framework_root().
    };
    let mut fails = 0usize;
    for entry in entries.flatten() {
        let skill_dir = entry.path();
        if !skill_dir.is_dir() {
            continue;
        }
        let skill_md = skill_dir.join("SKILL.md");
        if !skill_md.exists() {
            continue;
        }
        if let Err(e) = learn::validate_skill_file(&skill_md) {
            println!(
                "skills/{}/SKILL.md: FAIL: {e}",
                skill_dir.file_name().unwrap_or_default().to_string_lossy()
            );
            fails += 1;
        }
    }
    fails
}

/// Check every `hooks/*.sh` exists and is executable.
/// Returns the number of failures (and prints a FAIL line per offender).
fn probe_hooks(dir: &Path) -> usize {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => {
            println!("hooks/: FAIL: hooks directory not found");
            return 1;
        }
    };
    let mut fails = 0usize;
    for entry in entries.flatten() {
        let p = entry.path();
        if p.extension().and_then(|e| e.to_str()) != Some("sh") {
            continue;
        }
        if !is_executable(&p) {
            println!(
                "hooks/{}: FAIL: not executable",
                p.file_name().unwrap_or_default().to_string_lossy()
            );
            fails += 1;
        }
    }
    fails
}
