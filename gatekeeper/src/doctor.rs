//! `gatekeeper doctor` — read-only health check with binary-resolution transparency.
//!
//! Prints one line per probe with an `ok` / `FAIL` / `n/a` tag, then a summary line.
//! Exit 0 if all probes pass; exit 1 on any FAIL.
//! Writes nothing; framework root is resolved the same way every other command does.
//! See docs/adr/0010-packaging-distribution.md §1 and §2.

use std::fs;
use std::path::Path;

use serde::Deserialize;

use crate::instinct;
use crate::learn;
use crate::scan;
use crate::version;
use crate::RootSource;

// toml is a direct dependency — re-use the crate already in Cargo.toml

// ── VERSION file types ───────────────────────────────────────────────────────

/// Parsed contents of the `VERSION` file at the framework root.
#[derive(Debug, Deserialize, PartialEq)]
pub struct VersionFile {
    pub version: String,
    pub rules_schema: u32,
}

/// Result of attempting to parse the `VERSION` file at `root/VERSION`.
#[derive(Debug, PartialEq)]
pub enum VersionProbe {
    /// File present and parsed successfully.
    Present(VersionFile),
    /// File absent (dev checkout) — informational, not a failure.
    Absent,
    /// File present but could not be parsed.
    ParseError(String),
    /// File present, parsed, but `version` field is missing.
    MissingField(String),
}

/// Parse the `VERSION` file at `path`.
///
/// The file uses line-anchored TOML (two fields: `version = "x.y.z"` and
/// `rules_schema = N`) parseable by both the `toml` crate and the bash
/// `grep -m1` idiom.
pub fn parse_version_file(path: &Path) -> VersionProbe {
    let raw = match fs::read_to_string(path) {
        Ok(r) => r,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return VersionProbe::Absent,
        Err(e) => return VersionProbe::ParseError(e.to_string()),
    };
    match toml::from_str::<VersionFile>(&raw) {
        Ok(v) => VersionProbe::Present(v),
        Err(e) => {
            // Distinguish "missing field" from other parse errors for better messages.
            let msg = e.to_string();
            if msg.contains("missing field") {
                VersionProbe::MissingField(msg)
            } else {
                VersionProbe::ParseError(msg)
            }
        }
    }
}

/// Returns `true` when the payload version in `vf` does not match the running binary version.
///
/// Extracted from the `cmd_doctor` match arm so the skew decision is independently testable:
/// the test can call this function with both a matching and a mismatched `VersionFile` and
/// assert the return value — which means a regression in the comparison logic would fail the
/// test rather than just asserting values the test itself constructed.
pub fn version_skew(vf: &VersionFile) -> bool {
    vf.version != version::tool()
}

/// Run all doctor probes and print a report. Returns 0 (all ok) or 1 (any FAIL).
pub fn cmd_doctor(root: &Path, source: &RootSource) -> i32 {
    use crate::{artifacts_root, is_marked_root, project_root};
    let mut failures = 0usize;

    // ── Two-roots transparency ───────────────────────────────────────────────
    println!("framework root: {}", root.display());

    // Print which resolution step produced this root (spec: "resolved by:" line).
    let source_label = match source {
        RootSource::EnvOverride => "env override ($TOPOLOGY_ROOT)",
        RootSource::SelfGoverned => "self-governed project",
        RootSource::BinaryAdjacent => "binary-adjacent",
        RootSource::ProjectVendored => "project .topology",
        RootSource::GlobalHome => "global ~/.topology",
        RootSource::Fallback => "fallback (cwd)",
    };
    println!("resolved by: {source_label}");

    // F1: the resolved root must be a marked root (unless it was an explicit env pin,
    // which is obeyed verbatim — the user is responsible for the pin's content).
    // A fallback root is by definition an unmarked directory → FAIL.
    if !is_marked_root(root) {
        println!(
            "framework root: FAIL: {} is not a marked topology root \
             (missing skills/ + one of AGENTS.md / gatekeeper/ / .claude-plugin/); \
             run 'gatekeeper doctor' after 'gatekeeper adapt --harness claude' or set TOPOLOGY_ROOT",
            root.display()
        );
        failures += 1;
    }

    let proj_root = project_root();
    println!("project root: {}", proj_root.display());
    let art_root = artifacts_root();
    println!("artifacts root: {}", art_root.display());

    // F2: running from inside a payload clone (project == framework AND VERSION present)
    // means the user accidentally cd'd into the payload directory. Their "project" is
    // the payload itself — artifacts would land inside the payload. Report FAIL and tell
    // them to cd into their real project.
    let roots_same = match (fs::canonicalize(&proj_root), fs::canonicalize(root)) {
        (Ok(p), Ok(f)) => p == f,
        _ => proj_root == root,
    };
    if roots_same {
        let version_path = root.join("VERSION");
        if matches!(parse_version_file(&version_path), VersionProbe::Present(_)) {
            println!(
                "framework root: FAIL: running from inside a payload install at {} \
                 (project root == framework root and VERSION present); \
                 cd into your real project, then run gatekeeper from there",
                root.display()
            );
            failures += 1;
        }
    }

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

    // ── VERSION file ─────────────────────────────────────────────────────────
    // Reports payload version + rules_schema; FAILs on binary↔payload version skew.
    // Absent VERSION (dev checkout) is informational only.
    // `is_payload_install` is used below to suppress the repo-build probe: a payload
    // root has no gatekeeper/target/ tree, so "not found" would be misleading.
    let version_path = root.join("VERSION");
    let version_probe = parse_version_file(&version_path);
    let is_payload_install = matches!(version_probe, VersionProbe::Present(_));
    match version_probe {
        VersionProbe::Present(ref vf) => {
            if version_skew(vf) {
                println!(
                    "VERSION: FAIL: payload version {} does not match binary version {}",
                    vf.version,
                    version::tool()
                );
                failures += 1;
            } else {
                println!(
                    "VERSION: payload {} (rules schema v{})",
                    vf.version, vf.rules_schema
                );
            }
        }
        VersionProbe::Absent => {
            println!("VERSION: not present (dev checkout)");
        }
        VersionProbe::ParseError(ref e) => {
            println!("VERSION: FAIL: parse error: {e}");
            failures += 1;
        }
        VersionProbe::MissingField(ref e) => {
            println!("VERSION: FAIL: missing field: {e}");
            failures += 1;
        }
    }

    // $GATEKEEPER_BIN override (new this phase — neither hook reads it yet; doctor surfaces it).
    let gk_bin_env = std::env::var("GATEKEEPER_BIN").ok();
    match &gk_bin_env {
        None => println!("GATEKEEPER_BIN: not set (optional override; informational)"),
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

    // Repo build: only meaningful for dev checkouts — a payload root has no
    // gatekeeper/target/ tree, so printing "not found" would falsely imply something
    // is missing. Suppress the lookup when VERSION confirms a payload install.
    if is_payload_install {
        println!("repo build: n/a (payload install)");
    } else {
        let repo_build = find_repo_build(root);
        match &repo_build {
            Some(p) => println!("repo build: {}", p.display()),
            None => println!("repo build: not found (informational)"),
        }
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
    // The hook must guard the repo the developer COMMITS to — the project root. In the
    // framework repo project == framework, so this matches the old behavior; in a governed
    // project, checking the vendored clone's own .git would report "ok" while the
    // developer's commits go entirely unscanned.
    //
    // We resolve the live hooks directory via `git rev-parse --git-path hooks` so this
    // probe works correctly in linked worktrees (where `.git` is a file, not a directory,
    // and hooks live in the main worktree's object store) as well as normal checkouts.
    let commit_repo = crate::project_root();
    // Detect whether we are running inside the framework dev checkout itself.
    let is_framework_dev_checkout = {
        use std::fs;
        let same = match (fs::canonicalize(&commit_repo), fs::canonicalize(root)) {
            (Ok(p), Ok(f)) => p == f,
            _ => commit_repo == root,
        };
        same && !is_payload_install
    };
    match resolve_git_hooks_dir(&commit_repo) {
        Some(hooks_dir) => {
            let pc = hooks_dir.join("pre-commit");
            if pc.is_file() && is_executable(&pc) {
                println!(".git/hooks/pre-commit: ok ({})", pc.display());
            } else if is_framework_dev_checkout {
                println!(
                    ".git/hooks/pre-commit: FAIL: not installed in {} \
                     (framework dev clone missing its own hook — run: just setup)",
                    commit_repo.display()
                );
                failures += 1;
            } else {
                println!(
                    ".git/hooks/pre-commit: FAIL: not installed in {} (run scripts/install.sh or \
                     gatekeeper adapt --harness claude)",
                    commit_repo.display()
                );
                failures += 1;
            }
        }
        None => {
            println!(".git/hooks/pre-commit: n/a (no git repository — plugin/PATH install)");
        }
    }

    // ── git version ≥ 2.15 + capabilities ───────────────────────────────────
    // Required by the human-commit approval provenance check (spec §4).
    let proj_root_for_git = crate::project_root();
    let has_git_repo = proj_root_for_git.join(".git").exists();
    match crate::probe_git_version() {
        crate::GitVersionResult::Ok => {
            println!("git version: ok (≥ 2.15)");
            if !has_git_repo {
                println!("git shallow: n/a (no .git repository at project root)");
            } else {
                match crate::probe_git_shallow(&proj_root_for_git) {
                    crate::ShallowResult::NotShallow => {
                        println!("git shallow: ok (full clone)");
                    }
                    crate::ShallowResult::Shallow => {
                        println!(
                            "git shallow: FAIL: repository is a shallow clone; \
                             run 'git fetch --unshallow' to enable human-commit approval check"
                        );
                        failures += 1;
                    }
                    crate::ShallowResult::Error(e) => {
                        println!("git shallow: FAIL: cannot determine: {e}");
                        failures += 1;
                    }
                }
            }
        }
        crate::GitVersionResult::TooOld(v) => {
            println!(
                "git version: FAIL: {v} is too old (need ≥ 2.15 for %(trailers) support and \
                 --is-shallow-repository); upgrade git"
            );
            failures += 1;
        }
        crate::GitVersionResult::Unparsable(raw) => {
            println!(
                "git version: FAIL: cannot parse git version output {raw:?}; upgrade git or fix PATH"
            );
            failures += 1;
        }
    }

    // ── config.toml unknown keys ─────────────────────────────────────────────
    // Doctor lists unrecognized keys under known tables (catches typos).
    let art_root_for_cfg = crate::artifacts_root();
    probe_config_unknown_keys(&art_root_for_cfg);

    // ── Summary ─────────────────────────────────────────────────────────────
    if failures == 0 {
        println!("doctor: all probes ok");
        0
    } else {
        println!("doctor: {failures} probe(s) FAILED");
        1
    }
}

/// Known top-level config keys.
const KNOWN_TOP_KEYS: &[&str] = &["base_branch", "test_command", "verify", "design", "finish"];

/// Known [verify] sub-keys.
const KNOWN_VERIFY_KEYS: &[&str] = &["mode", "replay_timeout_secs", "allowed_command_prefixes"];

/// Known [design] sub-keys.
const KNOWN_DESIGN_KEYS: &[&str] = &["substance_floor", "approval", "agent_trailer_patterns"];

/// Known [finish] sub-keys.
const KNOWN_FINISH_KEYS: &[&str] = &["require_test_count", "extra_count_patterns"];

/// Parse config.toml and list any unrecognized keys under known tables.
/// Prints informational lines (not FAIL lines — forward-compat is the policy for unknown keys).
fn probe_config_unknown_keys(artifacts_root: &Path) {
    let config_path = artifacts_root.join("config.toml");
    let raw = match fs::read_to_string(&config_path) {
        Ok(s) => s,
        Err(_) => {
            // Missing config is fine; nothing to report.
            return;
        }
    };
    let val: toml::Value = match raw.parse() {
        Ok(v) => v,
        Err(_) => {
            // Parse errors are reported by the gate; doctor just skips the key scan.
            println!("config.toml unknown-keys: skipped (config.toml is malformed)");
            return;
        }
    };
    let table = match val.as_table() {
        Some(t) => t,
        None => return,
    };

    let mut unknown_top: Vec<String> = Vec::new();
    for key in table.keys() {
        if !KNOWN_TOP_KEYS.contains(&key.as_str()) {
            unknown_top.push(key.clone());
        }
    }
    if !unknown_top.is_empty() {
        println!(
            "config.toml: unrecognized top-level key(s): {} (ignored — forward compat; possible typo?)",
            unknown_top.join(", ")
        );
    }

    // Check sub-tables
    let sub_checks: &[(&str, &[&str])] = &[
        ("verify", KNOWN_VERIFY_KEYS),
        ("design", KNOWN_DESIGN_KEYS),
        ("finish", KNOWN_FINISH_KEYS),
    ];
    for (table_name, known) in sub_checks {
        if let Some(sub) = table.get(*table_name).and_then(|v| v.as_table()) {
            let unknown: Vec<String> = sub
                .keys()
                .filter(|k| !known.contains(&k.as_str()))
                .cloned()
                .collect();
            if !unknown.is_empty() {
                println!(
                    "config.toml [{}]: unrecognized key(s): {} (ignored — forward compat; possible typo?)",
                    table_name,
                    unknown.join(", ")
                );
            }
        }
    }
}

// ── Internal helpers ─────────────────────────────────────────────────────────

/// Resolve the live git hooks directory for the repo at `repo_root`.
///
/// Uses `git rev-parse --git-path hooks` so the result is correct for both
/// normal checkouts (`.git/hooks`) and linked worktrees (where `.git` is a
/// file and hooks live in the main worktree's object store).
///
/// Returns `None` when `repo_root` is not a git repository (no `.git` entry at
/// all) so the caller can emit `n/a` rather than a spurious FAIL.
pub fn resolve_git_hooks_dir(repo_root: &Path) -> Option<std::path::PathBuf> {
    // Fast-path: if there is no `.git` entry whatsoever (file or dir), this is
    // not a git repo — return None without running git.
    let git_entry = repo_root.join(".git");
    if !git_entry.exists() {
        return None;
    }
    // Use `git rev-parse --git-path hooks` for correctness in worktrees.
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["rev-parse", "--git-path", "hooks"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let raw = String::from_utf8_lossy(&out.stdout);
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let p = std::path::PathBuf::from(trimmed);
    // `git rev-parse --git-path hooks` may return a relative path — resolve it
    // relative to repo_root so callers always get an absolute path.
    if p.is_absolute() {
        Some(p)
    } else {
        Some(repo_root.join(p))
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::fs;

    // ── parse_version_file unit tests ─────────────────────────────────────────

    #[test]
    fn version_file_well_formed_parses() {
        let base = env::temp_dir().join("topology_doctor_vf_ok");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        let path = base.join("VERSION");
        fs::write(&path, "version = \"0.4.0\"\nrules_schema = 1\n").unwrap();

        let probe = parse_version_file(&path);
        assert_eq!(
            probe,
            VersionProbe::Present(VersionFile {
                version: "0.4.0".to_string(),
                rules_schema: 1,
            }),
            "well-formed VERSION must parse to Present"
        );
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn version_file_missing_field_returns_missing_field() {
        let base = env::temp_dir().join("topology_doctor_vf_missing");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        let path = base.join("VERSION");
        // Missing rules_schema field
        fs::write(&path, "version = \"0.4.0\"\n").unwrap();

        let probe = parse_version_file(&path);
        assert!(
            matches!(probe, VersionProbe::MissingField(_)),
            "VERSION with missing field must return MissingField, got: {probe:?}"
        );
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn version_file_absent_returns_absent() {
        let base = env::temp_dir().join("topology_doctor_vf_absent");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        let path = base.join("VERSION"); // does not exist

        let probe = parse_version_file(&path);
        assert_eq!(
            probe,
            VersionProbe::Absent,
            "absent VERSION file must return Absent"
        );
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn version_file_skew_vs_match() {
        // Call the production `version_skew()` function for both cases so this test
        // goes red if the skew comparison is broken (previously the test only asserted
        // values it had just constructed, never exercising the production code path).
        let my_ver = version::tool();

        // Case: matching version — version_skew must return false (no skew → no failure).
        let matching = VersionFile {
            version: my_ver.to_string(),
            rules_schema: 1,
        };
        assert!(
            !version_skew(&matching),
            "version_skew must return false when payload version matches binary version"
        );

        // Case: mismatched version — version_skew must return true (skew → failure).
        let skewed = VersionFile {
            version: "99.99.99".to_string(),
            rules_schema: 1,
        };
        assert!(
            version_skew(&skewed),
            "version_skew must return true when payload version differs from binary version"
        );
    }

    #[test]
    fn version_file_parse_error_on_bad_toml() {
        let base = env::temp_dir().join("topology_doctor_vf_bad");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        let path = base.join("VERSION");
        // Malformed TOML
        fs::write(&path, "not valid = toml [[[\n").unwrap();

        let probe = parse_version_file(&path);
        assert!(
            matches!(probe, VersionProbe::ParseError(_)),
            "malformed TOML must return ParseError, got: {probe:?}"
        );
        let _ = fs::remove_dir_all(&base);
    }

    // ── resolve_git_hooks_dir unit tests ──────────────────────────────────────

    #[test]
    fn resolve_git_hooks_dir_no_git_entry_returns_none() {
        let base = env::temp_dir().join("topology_doctor_hooks_none");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        // No .git at all — must return None without calling git.
        assert!(
            resolve_git_hooks_dir(&base).is_none(),
            "directory without .git must return None"
        );
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn resolve_git_hooks_dir_normal_repo_returns_git_hooks() {
        let base = env::temp_dir().join("topology_doctor_hooks_normal");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        // Initialise a real git repo.
        let status = std::process::Command::new("git")
            .args(["-C", base.to_str().unwrap(), "init"])
            .output()
            .expect("git init failed");
        assert!(status.status.success(), "git init must succeed");

        let result = resolve_git_hooks_dir(&base);
        assert!(
            result.is_some(),
            "normal git repo must return Some(hooks_dir)"
        );
        let hooks_dir = result.unwrap();
        // In a normal checkout the hooks dir lives inside .git/.
        assert!(
            hooks_dir.ends_with("hooks"),
            "hooks dir must end with 'hooks'; got: {}",
            hooks_dir.display()
        );
        let _ = fs::remove_dir_all(&base);
    }
}
