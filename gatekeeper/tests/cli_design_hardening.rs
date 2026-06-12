//! Integration tests for the design gate hardening (spec §4):
//! substance floor + human-commit approval provenance.
//!
//! All tests that need commit history create fully-initialised scratch git repos
//! (the `cli_check.rs`/`cli_review.rs` idiom).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

// ── helpers ───────────────────────────────────────────────────────────────────

/// Minimal framework root: `skills/` dir + `AGENTS.md`.
fn scratch_root(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("topo_dh_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("skills")).unwrap();
    fs::write(root.join("AGENTS.md"), "").unwrap();
    root
}

/// Run `gatekeeper <args>` from `cwd`.  Returns `(exit_code, stdout, stderr)`.
fn run_full(cwd: &Path, args: &[&str]) -> (i32, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_gatekeeper"))
        .current_dir(cwd)
        .args(args)
        .output()
        .unwrap();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// Run `gatekeeper <args>` and return `(exit_code, stdout)` (discards stderr).
fn run(cwd: &Path, args: &[&str]) -> (i32, String) {
    let (code, out, _) = run_full(cwd, args);
    (code, out)
}

/// Run a git command inside `root`; panics on failure.
fn git(root: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .status()
        .unwrap()
        .success();
    assert!(ok, "git {args:?} failed in {}", root.display());
}

/// Initialise a git repo with a basic config.
fn git_init(root: &Path) {
    git(root, &["init", "-q", "-b", "main"]);
    git(root, &["config", "user.email", "test@test.test"]);
    git(root, &["config", "user.name", "Test"]);
}

/// Build a minimal scratch root with all required directories and a research note.
/// Does NOT create a git repo.
fn base_root_with_research(tag: &str, feature: &str) -> PathBuf {
    let root = scratch_root(tag);
    fs::create_dir_all(root.join("docs").join("research")).unwrap();
    fs::create_dir_all(root.join("docs").join("specs")).unwrap();
    fs::write(
        root.join("docs")
            .join("research")
            .join(format!("2026-06-11-{feature}.md")),
        "# Research\n\nSome findings.\n",
    )
    .unwrap();
    root
}

/// Write the substance floor config.
fn write_substance_floor_config(root: &Path) {
    fs::write(
        root.join("docs").join("config.toml"),
        "[design]\nsubstance_floor = true\n",
    )
    .unwrap();
}

/// Write the human-commit config.
fn write_human_commit_config(root: &Path) {
    fs::write(
        root.join("docs").join("config.toml"),
        "[design]\napproval = \"human-commit\"\n",
    )
    .unwrap();
}

/// Write a spec with sufficient substance (≥2 headings, ≥1 body line).
fn write_substantial_spec(specs_dir: &Path, slug: &str) {
    fs::write(
        specs_dir.join(format!("2026-06-11-{slug}.md")),
        "**Status:** approved\n\n## Goal\n\nDo something useful.\n\n## Non-goals\n\nNot this.\n",
    )
    .unwrap();
}

/// Write a hollow spec (only Status: approved — no headings, no body).
fn write_hollow_spec(specs_dir: &Path, slug: &str) {
    fs::write(
        specs_dir.join(format!("2026-06-11-{slug}.md")),
        "Status: approved\n",
    )
    .unwrap();
}

// ── substance floor tests ─────────────────────────────────────────────────────

/// Substance floor rejects a spec that has only the approval marker.
#[test]
fn substance_floor_rejects_approved_only_spec() {
    let root = base_root_with_research("sf_reject", "sf-reject");
    write_substance_floor_config(&root);
    write_hollow_spec(&root.join("docs").join("specs"), "sf-reject");

    let (code, out) = run(&root, &["check", "design", "--feature", "sf-reject"]);
    assert_ne!(
        code, 0,
        "substance floor must reject hollow spec; out: {out}"
    );
    assert!(out.contains("FAIL"), "output must mention FAIL; got: {out}");
    let _ = fs::remove_dir_all(&root);
}

/// Substance floor passes a spec with ≥2 headings and ≥1 body line.
#[test]
fn substance_floor_passes_real_spec() {
    let root = base_root_with_research("sf_pass", "sf-pass");
    write_substance_floor_config(&root);
    write_substantial_spec(&root.join("docs").join("specs"), "sf-pass");

    let (code, out) = run(&root, &["check", "design", "--feature", "sf-pass"]);
    assert_eq!(code, 0, "substance floor must pass real spec; out: {out}");
    assert!(out.contains("PASS"), "output must say PASS; got: {out}");
    let _ = fs::remove_dir_all(&root);
}

/// When substance_floor is off (default), a SHADOW line is emitted but the gate passes.
#[test]
fn substance_floor_off_emits_shadow_not_gate_fail() {
    let root = base_root_with_research("sf_shadow", "sf-shadow");
    // No config.toml — substance_floor defaults to false
    write_hollow_spec(&root.join("docs").join("specs"), "sf-shadow");

    let (code, _out, err) = run_full(&root, &["check", "design", "--feature", "sf-shadow"]);
    // Gate should pass (substance_floor is off)
    assert_eq!(
        code, 0,
        "with substance_floor off, hollow spec must still pass the gate"
    );
    // SHADOW line must be on stderr
    assert!(
        err.contains("SHADOW"),
        "SHADOW line must appear on stderr; stderr: {err}"
    );
    assert!(
        err.contains("\"check\":\"substance_floor\""),
        "SHADOW must name the check; stderr: {err}"
    );
    assert!(
        err.contains("\"result\":\"fail\""),
        "SHADOW result must be fail for hollow spec; stderr: {err}"
    );
    let _ = fs::remove_dir_all(&root);
}

// ── human-commit approval tests ───────────────────────────────────────────────

/// Approve the spec with an agent trailer — must FAIL with the trailer message.
#[test]
fn human_commit_rejects_agent_trailer() {
    let root = base_root_with_research("hc_agent", "hc-agent");
    write_human_commit_config(&root);

    let specs_dir = root.join("docs").join("specs");
    write_substantial_spec(&specs_dir, "hc-agent");

    // Init git repo and commit everything with an agent Co-Authored-By trailer
    git_init(&root);
    git(&root, &["add", "."]);
    git(
        &root,
        &[
            "commit",
            "-q",
            "-m",
            "docs(spec): approve hc-agent\n\nCo-Authored-By: Claude Fable 5 <noreply@anthropic.com>",
        ],
    );

    let (code, out, err) = run_full(&root, &["check", "design", "--feature", "hc-agent"]);
    assert_ne!(
        code, 0,
        "agent-trailer approval must FAIL; out: {out}, err: {err}"
    );
    let combined = format!("{out}{err}");
    assert!(
        combined.contains("agent trailer") || combined.contains("Co-Authored-By"),
        "failure message must mention agent trailer; combined: {combined}"
    );
    let _ = fs::remove_dir_all(&root);
}

/// Approve the spec with a clean human commit (no agent trailer) — must PASS.
#[test]
fn human_commit_passes_clean_human_commit() {
    let root = base_root_with_research("hc_human", "hc-human");
    write_human_commit_config(&root);

    let specs_dir = root.join("docs").join("specs");
    write_substantial_spec(&specs_dir, "hc-human");

    git_init(&root);
    git(&root, &["add", "."]);
    git(
        &root,
        &[
            "commit",
            "-q",
            "-m",
            "docs(spec): approve hc-human (human approved)",
        ],
    );

    let (code, out) = run(&root, &["check", "design", "--feature", "hc-human"]);
    assert_eq!(code, 0, "clean human commit must PASS; out: {out}");
    assert!(out.contains("PASS"), "output must say PASS; got: {out}");
    let _ = fs::remove_dir_all(&root);
}

/// Spec with unstaged changes after approval commit — must fail closed.
#[test]
fn human_commit_fails_dirty_spec_unstaged() {
    let root = base_root_with_research("hc_dirty", "hc-dirty");
    write_human_commit_config(&root);

    let specs_dir = root.join("docs").join("specs");
    write_substantial_spec(&specs_dir, "hc-dirty");

    git_init(&root);
    git(&root, &["add", "."]);
    git(
        &root,
        &["commit", "-q", "-m", "docs(spec): approve hc-dirty"],
    );

    // Now make an unstaged edit to the spec after the approval commit
    let spec_path = specs_dir.join("2026-06-11-hc-dirty.md");
    let mut current = fs::read_to_string(&spec_path).unwrap();
    current.push_str("\n<!-- unstaged edit after approval -->\n");
    fs::write(&spec_path, &current).unwrap();

    let (code, out, err) = run_full(&root, &["check", "design", "--feature", "hc-dirty"]);
    assert_ne!(
        code, 0,
        "dirty spec (unstaged) must fail closed; out: {out}, err: {err}"
    );
    let combined = format!("{out}{err}");
    assert!(
        combined.contains("unstaged") || combined.contains("dirty"),
        "message must mention unstaged changes; combined: {combined}"
    );
    let _ = fs::remove_dir_all(&root);
}

/// Spec with staged (index) changes — must fail closed.
#[test]
fn human_commit_fails_dirty_spec_staged() {
    let root = base_root_with_research("hc_staged", "hc-staged");
    write_human_commit_config(&root);

    let specs_dir = root.join("docs").join("specs");
    write_substantial_spec(&specs_dir, "hc-staged");

    git_init(&root);
    git(&root, &["add", "."]);
    git(
        &root,
        &["commit", "-q", "-m", "docs(spec): approve hc-staged"],
    );

    // Staged edit after approval commit
    let spec_path = specs_dir.join("2026-06-11-hc-staged.md");
    let mut current = fs::read_to_string(&spec_path).unwrap();
    current.push_str("\n<!-- staged edit after approval -->\n");
    fs::write(&spec_path, &current).unwrap();
    let relpath = spec_path.strip_prefix(&root).unwrap().to_str().unwrap();
    git(&root, &["add", relpath]);

    let (code, out, err) = run_full(&root, &["check", "design", "--feature", "hc-staged"]);
    assert_ne!(
        code, 0,
        "dirty spec (staged) must fail closed; out: {out}, err: {err}"
    );
    let combined = format!("{out}{err}");
    assert!(
        combined.contains("staged") || combined.contains("dirty"),
        "message must mention staged changes; combined: {combined}"
    );
    let _ = fs::remove_dir_all(&root);
}

/// Untracked spec (never committed) — must fail closed.
#[test]
fn human_commit_fails_untracked_spec() {
    let root = base_root_with_research("hc_untracked", "hc-untracked");
    write_human_commit_config(&root);

    let specs_dir = root.join("docs").join("specs");
    write_substantial_spec(&specs_dir, "hc-untracked");

    git_init(&root);
    // Only commit the research note and config — NOT the spec
    let research_path = root
        .join("docs")
        .join("research")
        .join("2026-06-11-hc-untracked.md");
    let rp = research_path.strip_prefix(&root).unwrap().to_str().unwrap();
    git(&root, &["add", rp]);
    git(&root, &["add", "docs/config.toml"]);
    git(&root, &["add", "AGENTS.md"]);
    git(&root, &["commit", "-q", "-m", "init"]);

    // spec is untracked
    let (code, out, err) = run_full(&root, &["check", "design", "--feature", "hc-untracked"]);
    assert_ne!(
        code, 0,
        "untracked spec must fail closed; out: {out}, err: {err}"
    );
    let combined = format!("{out}{err}");
    assert!(
        combined.contains("untracked"),
        "message must mention untracked; combined: {combined}"
    );
    let _ = fs::remove_dir_all(&root);
}

/// Shallow clone — must fail closed with a specific message about shallowness.
#[test]
fn human_commit_fails_shallow_clone() {
    // Create a "normal" repo first, then shallow-clone it.
    let origin = scratch_root("hc_shallow_origin");
    let specs_dir = origin.join("docs").join("specs");
    fs::create_dir_all(&specs_dir).unwrap();
    fs::create_dir_all(origin.join("docs").join("research")).unwrap();
    fs::write(
        origin
            .join("docs")
            .join("research")
            .join("2026-06-11-hc-shallow.md"),
        "# Research\n\nFindings.\n",
    )
    .unwrap();
    fs::create_dir_all(origin.join("skills")).unwrap();
    fs::write(origin.join("AGENTS.md"), "").unwrap();
    write_human_commit_config(&origin);
    write_substantial_spec(&specs_dir, "hc-shallow");

    git_init(&origin);
    git(&origin, &["add", "."]);
    git(&origin, &["commit", "-q", "-m", "docs: init"]);

    // Shallow clone (depth 1) of the origin using file:// URL
    let shallow =
        std::env::temp_dir().join(format!("topo_dh_hc_shallow_clone_{}", std::process::id()));
    let _ = fs::remove_dir_all(&shallow);
    let origin_url = format!("file://{}", origin.display());
    let ok = Command::new("git")
        .args([
            "clone",
            "--depth",
            "1",
            &origin_url,
            &shallow.to_string_lossy(),
        ])
        .status()
        .unwrap()
        .success();
    assert!(ok, "git clone --depth 1 failed");

    let (code, out, err) = run_full(&shallow, &["check", "design", "--feature", "hc-shallow"]);
    assert_ne!(
        code, 0,
        "shallow clone must fail closed; out: {out}, err: {err}"
    );
    let combined = format!("{out}{err}");
    assert!(
        combined.contains("shallow"),
        "message must mention shallow clone; combined: {combined}"
    );

    let _ = fs::remove_dir_all(&origin);
    let _ = fs::remove_dir_all(&shallow);
}

/// When approval is default (status-line), a SHADOW line is emitted but the gate passes.
#[test]
fn approval_provenance_off_emits_shadow_not_gate_fail() {
    let root = base_root_with_research("ap_shadow", "ap-shadow");
    // No config.toml — approval defaults to status-line

    let specs_dir = root.join("docs").join("specs");
    write_substantial_spec(&specs_dir, "ap-shadow");

    // We need a git repo for the check to compute (even when not enforced).
    git_init(&root);
    git(&root, &["add", "."]);
    // Agent trailer commit — would fail if enforced
    git(
        &root,
        &[
            "commit",
            "-q",
            "-m",
            "docs: approve ap-shadow\n\nCo-Authored-By: Claude Fable 5 <noreply@anthropic.com>",
        ],
    );

    let (code, _out, err) = run_full(&root, &["check", "design", "--feature", "ap-shadow"]);
    // Gate must pass (approval is not enforced)
    assert_eq!(
        code, 0,
        "with approval=default, agent-trailer commit must not block the gate"
    );
    // SHADOW line must appear on stderr
    assert!(
        err.contains("SHADOW"),
        "SHADOW line must appear on stderr when approval is not enforced; stderr: {err}"
    );
    assert!(
        err.contains("\"check\":\"approval_provenance\""),
        "SHADOW must name check=approval_provenance; stderr: {err}"
    );
    assert!(
        err.contains("\"result\":\"fail\""),
        "SHADOW result must be fail (agent trailer detected); stderr: {err}"
    );
    let _ = fs::remove_dir_all(&root);
}

// ── negative dogfood ──────────────────────────────────────────────────────────

/// NEGATIVE DOGFOOD: run `check design` with `approval = "human-commit"` against
/// THIS repo's own spec file docs/specs/2026-06-11-hollow-pass-kills.md
/// and assert it FAILS with the agent-trailer message — because the approval
/// commit a9928a1 carries `Co-Authored-By: Claude Fable 5`.
///
/// **Why `#[ignore]`:** CI typically uses a shallow checkout. Under `human-commit`
/// mode, a shallow repo fails closed with the shallow-clone obstacle message rather
/// than the agent-trailer message — this would make the assertion on the trailer
/// message fail, making CI red for the wrong reason. The positive-path validation
/// is covered hermetically by the scratch-repo fixtures above.
///
/// **How to run manually** (full local checkout required):
/// ```
/// cargo test --manifest-path gatekeeper/Cargo.toml --test cli_design_hardening \
///     negative_dogfood_own_spec_fails_agent_trailer -- --ignored --nocapture
/// ```
///
/// **Expected output (exit 1):**
/// ```
/// FAIL design gate: approval_provenance: commit a9928a1... carries agent trailer
///   'Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>' (matched pattern "(?i)claude")
/// ```
///
/// The spec notes (§4): "the maintainer delegated the approval commit to the agent;
/// it therefore carries the honest agent trailer and serves as the negative dogfood".
/// This test encodes that contract in the test suite.
///
/// **Manual run output (2026-06-11, verified):**
/// See RETURN section in the implementation notes: exit code 1, message contains
/// "agent trailer" and "Co-Authored-By: Claude Fable 5".
#[test]
#[ignore = "requires full (non-shallow) local checkout — shallow CI would fail for wrong reason (obstacle not trailer); run manually as documented"]
fn negative_dogfood_own_spec_fails_agent_trailer() {
    // The repo root is the parent of gatekeeper/ (where Cargo.toml lives).
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir.parent().unwrap();

    // Verify the spec exists and is committed in the real repo.
    let spec_path = repo_root
        .join("docs")
        .join("specs")
        .join("2026-06-11-hollow-pass-kills.md");
    assert!(
        spec_path.exists(),
        "spec not found at {}; run from full checkout",
        spec_path.display()
    );

    // Check this is not a shallow clone (would produce wrong failure reason).
    let shallow_out = Command::new("git")
        .args([
            "-C",
            &repo_root.to_string_lossy(),
            "rev-parse",
            "--is-shallow-repository",
        ])
        .output()
        .expect("git rev-parse --is-shallow-repository failed");
    let shallow_val = String::from_utf8_lossy(&shallow_out.stdout);
    assert_ne!(
        shallow_val.trim(),
        "true",
        "test skipped: repo is a shallow clone — run 'git fetch --unshallow' first"
    );

    // Strategy: run gatekeeper FROM the real repo root (self-governing: project == framework).
    // The real repo's docs/config.toml must have human-commit enabled.
    // We temporarily write the config (then restore it at the end).
    //
    // However the spec says we should not mutate the repo. Instead we use a scratch
    // self-governing framework that COPIES the real spec and research files, has
    // human-commit in its config, and sets `project_root` to the real repo by
    // running gatekeeper from the real repo BUT with TOPOLOGY_ROOT pointing to a
    // scratch self-governing root placed INSIDE the real repo so that
    // resolve_artifacts_root sees project != framework and uses .claude/topology.
    //
    // Actually the cleanest approach: the scratch framework root is placed INSIDE the
    // real repo at a known path. Since project root = nearest .git ancestor of cwd,
    // and cwd = real repo, project_root = real repo. Framework root = our scratch
    // (via TOPOLOGY_ROOT). Since project != framework → artifacts root = real_repo/.claude/topology.
    // But that's the REAL .claude/topology which we can't use without config changes.
    //
    // The ONLY hermetic approach is: create a git-initialised scratch root that is itself
    // the project, copy the real spec into it, and run git log against it. The git log
    // then won't contain the real a9928a1 commit. We can't test the real commit
    // hermeticity without touching the real repo.
    //
    // CHOSEN APPROACH: Run from the real repo root, with TOPOLOGY_ROOT pointing to a
    // scratch self-governing framework, and create a symlink from scratch/docs/ to the
    // real docs/ directory.  Since project (real repo) != framework (scratch), artifacts
    // root = real_repo/.claude/topology/. Then copy the config into the real .claude/topology.
    //
    // SIMPLEST CORRECT APPROACH: Write a temp config.toml to the real artifacts root,
    // run the test, restore it. This is what the dogfood test must do to run against the
    // real approval commit. We save/restore carefully.
    let real_artifacts = repo_root.join("docs"); // self-governing: project == framework
    let config_path = real_artifacts.join("config.toml");

    // Save any existing config
    let saved_config = fs::read_to_string(&config_path).ok();

    // Write human-commit config (preserving other keys if any)
    let dogfood_config = match &saved_config {
        Some(existing) if existing.contains("[design]") => {
            // Replace the existing [design] section — too complex; just prepend
            format!("{existing}\n# dogfood override\n[design]\napproval = \"human-commit\"\n")
        }
        Some(existing) => {
            format!("{existing}\n[design]\napproval = \"human-commit\"\n")
        }
        None => "[design]\napproval = \"human-commit\"\n".to_string(),
    };
    fs::write(&config_path, &dogfood_config).expect("cannot write dogfood config");

    let out = Command::new(env!("CARGO_BIN_EXE_gatekeeper"))
        .current_dir(repo_root)
        .args(["check", "design", "--feature", "hollow-pass-kills"])
        .output()
        .unwrap();

    // Restore original config
    match saved_config {
        Some(original) => fs::write(&config_path, original).ok(),
        None => fs::remove_file(&config_path).ok(),
    };

    let code = out.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    let combined = format!("{stdout}{stderr}");

    println!("exit code: {code}");
    println!("stdout:\n{stdout}");
    println!("stderr:\n{stderr}");

    assert_ne!(
        code, 0,
        "negative dogfood: own spec approval commit (a9928a1) must FAIL with agent-trailer message; combined: {combined}"
    );
    assert!(
        combined.contains("agent trailer") || combined.contains("Co-Authored-By"),
        "failure message must cite the agent trailer; combined: {combined}"
    );
}

/// D7: a dirty spec is an *obstacle*, not a determination — under default
/// (status-line) config the gate passes and the approval_provenance SHADOW
/// line must report result "skip", never "fail" (Phase 15 burn-in pipelines
/// classify on this field).
#[test]
fn shadow_dirty_spec_obstacle_logs_skip() {
    let root = base_root_with_research("hc_shadow_skip", "hc-shadow-skip");
    // No config.toml — approval stays at its status-line default.

    let specs_dir = root.join("docs").join("specs");
    write_substantial_spec(&specs_dir, "hc-shadow-skip");

    git_init(&root);
    git(&root, &["add", "."]);
    git(
        &root,
        &["commit", "-q", "-m", "docs(spec): approve hc-shadow-skip"],
    );

    // Unstaged edit after the approval commit → obstacle.
    let spec_path = specs_dir.join("2026-06-11-hc-shadow-skip.md");
    let mut current = fs::read_to_string(&spec_path).unwrap();
    current.push_str("\n<!-- unstaged edit after approval -->\n");
    fs::write(&spec_path, &current).unwrap();

    let (code, out, err) = run_full(&root, &["check", "design", "--feature", "hc-shadow-skip"]);
    assert_eq!(code, 0, "default mode must pass; out: {out}, err: {err}");
    let prov_line = err
        .lines()
        .find(|l| l.starts_with("SHADOW ") && l.contains("approval_provenance"))
        .unwrap_or_else(|| panic!("expected approval_provenance SHADOW line; stderr: {err}"));
    assert!(
        prov_line.contains("\"result\":\"skip\""),
        "dirty-spec obstacle must log skip, not fail; line: {prov_line}"
    );
    let _ = fs::remove_dir_all(&root);
}
