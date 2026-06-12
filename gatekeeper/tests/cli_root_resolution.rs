//! Integration fixtures for root-resolution hardening (Phase 11).
//!
//! All fixtures are `#[ignore]`-tagged so the default suite stays green until
//! the production code is rewritten in task 2 (main.rs) and task 3 (doctor.rs).
//! Un-ignoring happens per-task:
//!   - task 2 un-ignores (a), (b), (c)
//!   - task 3 un-ignores (d), (e)
//!
//! Tests exercise the binary directly via `env!("CARGO_BIN_EXE_gatekeeper")`.
//! Layout helpers create tempdir trees; env vars are passed via `Command::env`
//! and `Command::env_remove` — the real process env is never mutated.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

// ── Layout helpers ────────────────────────────────────────────────────────────

/// Create a minimal marked root at `dir`.
/// A marked root satisfies `is_marked_root`: has `skills/` + at least one of
/// `AGENTS.md` / `gatekeeper/` / `.claude-plugin/`.
fn make_marked_root(dir: &Path) {
    fs::create_dir_all(dir.join("skills")).unwrap();
    fs::write(dir.join("AGENTS.md"), "# agents\n").unwrap();
}

/// Create a plain (unmarked) directory at `dir`.
fn make_plain_dir(dir: &Path) {
    fs::create_dir_all(dir).unwrap();
}

/// Build a `<root>/bin/gatekeeper` layout: copy the test binary into `<root>/bin/`.
/// Returns `<root>/bin/gatekeeper`.
fn install_binary_at(root: &Path) -> PathBuf {
    let bin_dir = root.join("bin");
    fs::create_dir_all(&bin_dir).unwrap();
    let src = PathBuf::from(env!("CARGO_BIN_EXE_gatekeeper"));
    let dst = bin_dir.join("gatekeeper");
    fs::copy(&src, &dst).unwrap();
    fs::set_permissions(&dst, fs::Permissions::from_mode(0o755)).unwrap();
    dst
}

/// Run the gatekeeper binary with the given cwd, args, and env vars.
/// Returns (exit_code, stdout, stderr).
fn run_binary(
    binary: &Path,
    cwd: &Path,
    args: &[&str],
    env_set: &[(&str, &str)],
    env_remove: &[&str],
) -> (i32, String, String) {
    let mut cmd = Command::new(binary);
    cmd.current_dir(cwd).args(args);
    for (k, v) in env_set {
        cmd.env(k, v);
    }
    for k in env_remove {
        cmd.env_remove(k);
    }
    // Isolate from the real process's TOPOLOGY_ROOT so fixtures are self-contained.
    cmd.env_remove("TOPOLOGY_ROOT");
    let out = cmd.output().expect("failed to spawn gatekeeper");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn gk_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_gatekeeper"))
}

// ── (a) Hijack-class regression ───────────────────────────────────────────────
//
// Layout:
//   /tmp/rr_hijack_<pid>/
//     ancestor/          ← marked root (would win under the old cwd walk)
//       skills/
//       AGENTS.md
//       project/         ← plain project directory (cwd for the binary)
//     bin/               ← binary installed here (no marked root above it)
//       gatekeeper
//
// With the old algorithm, running `gatekeeper doctor` from `ancestor/project/`
// resolves `ancestor/` because the cwd walk finds a marked ancestor.
// After the rewrite, no deterministic source points at `ancestor/`, so resolution
// falls back to cwd — `ancestor/project/` is NOT the framework root, doctor prints
// the fallback warning and exits non-zero (F1: unmarked root).
//
// The binary is installed at <base>/bin/ (a plain directory, not inside any marked
// root) so that binary-adjacent does not resolve to the ancestor either.
// HOME is remapped to a directory with no .topology so the global-install step also misses.

#[test]
fn a_hijack_class_ancestor_no_longer_wins() {
    let base = std::env::temp_dir().join(format!("rr_hijack_{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);

    let ancestor = base.join("ancestor");
    make_marked_root(&ancestor);
    let project = ancestor.join("project");
    make_plain_dir(&project);

    // Fake HOME with no .topology so the global-install step misses.
    let fake_home = base.join("home");
    fs::create_dir_all(&fake_home).unwrap();

    // Install the binary in a plain (unmarked) dir so binary-adjacent doesn't find
    // the ancestor either. The binary is placed at <base>/bin/gatekeeper — no marked
    // root exists above <base>/bin/.
    let plain_bin_dir = base.join("bin");
    fs::create_dir_all(&plain_bin_dir).unwrap();
    let src = PathBuf::from(env!("CARGO_BIN_EXE_gatekeeper"));
    let installed_bin = plain_bin_dir.join("gatekeeper");
    fs::copy(&src, &installed_bin).unwrap();
    fs::set_permissions(&installed_bin, fs::Permissions::from_mode(0o755)).unwrap();

    let (code, stdout, stderr) = run_binary(
        &installed_bin,
        &project,
        &["doctor"],
        &[("HOME", fake_home.to_str().unwrap())],
        &["TOPOLOGY_ROOT"],
    );

    // After the rewrite the ancestor must NOT appear as the framework root.
    // Resolution falls back → stderr fallback warning is emitted once.
    assert!(
        stderr.contains("no framework root found") || stderr.contains("falling back"),
        "expected fallback warning on stderr; got:\nstdout: {stdout}\nstderr: {stderr}"
    );
    // Doctor F1: the unmarked fallback root must cause a non-zero exit.
    assert_ne!(
        code, 0,
        "doctor must exit non-zero when ancestor hijack is removed (fallback to unmarked root);\n\
         stdout: {stdout}\nstderr: {stderr}"
    );

    let _ = fs::remove_dir_all(&base);
}

// ── (b) W2: governed project outside $HOME resolves ~/.topology ───────────────
//
// Layout:
//   /tmp/rr_w2_<pid>/
//     home/
//       .topology/       ← marked root (global install)
//         skills/
//         AGENTS.md
//         security/rules.toml
//         instincts/
//         hooks/hook.sh
//     project/           ← plain project, no .topology/ child, cwd for binary
//
// With the old algorithm, running from `/tmp/rr_w2_<pid>/project/` with HOME
// remapped to `/tmp/rr_w2_<pid>/home/` falls back to cwd (the walk never reaches
// `~/.topology` because it is not an ancestor of cwd).
// After the rewrite, step 5 explicitly probes `$HOME/.topology` by real path and
// resolves it.

const VALID_RULES_TOML: &str = "schema_version = 1\n";
const VALID_SKILL_MD: &str = "\
---\n\
name: test-skill\n\
description: A test skill.\n\
---\n\
\n\
Body text.\n";

fn make_full_marked_root(dir: &Path) {
    make_marked_root(dir);
    fs::create_dir_all(dir.join("security")).unwrap();
    fs::write(dir.join("security").join("rules.toml"), VALID_RULES_TOML).unwrap();
    fs::create_dir_all(dir.join("instincts")).unwrap();
    fs::create_dir_all(dir.join("hooks")).unwrap();
    let hook = dir.join("hooks").join("test.sh");
    fs::write(&hook, "#!/usr/bin/env bash\necho ok\n").unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    fs::create_dir_all(dir.join("skills").join("test-skill")).unwrap();
    fs::write(
        dir.join("skills").join("test-skill").join("SKILL.md"),
        VALID_SKILL_MD,
    )
    .unwrap();
}

#[test]
fn b_w2_global_topology_resolves_for_project_outside_home() {
    let base = std::env::temp_dir().join(format!("rr_w2_{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);

    // Global install at fake HOME/.topology
    let fake_home = base.join("home");
    let global_topology = fake_home.join(".topology");
    make_full_marked_root(&global_topology);

    // Project is outside fake HOME (sibling of home/, not under it)
    let project = base.join("project");
    make_plain_dir(&project);

    // Install the binary in a plain (unmarked) dir so binary-adjacent does NOT resolve
    // to the actual topology repo — we need the global-home step to be the winner.
    let plain_bin_dir = base.join("bin");
    fs::create_dir_all(&plain_bin_dir).unwrap();
    let src = PathBuf::from(env!("CARGO_BIN_EXE_gatekeeper"));
    let installed_bin = plain_bin_dir.join("gatekeeper");
    fs::copy(&src, &installed_bin).unwrap();
    fs::set_permissions(&installed_bin, fs::Permissions::from_mode(0o755)).unwrap();

    let (code, stdout, stderr) = run_binary(
        &installed_bin,
        &project,
        &["doctor"],
        &[("HOME", fake_home.to_str().unwrap())],
        &["TOPOLOGY_ROOT"],
    );

    // After the rewrite, ~/.topology is found via the global-home step.
    // Doctor should exit 0 (healthy root) and the framework root line should
    // name the global .topology path (canonicalized on macOS).
    assert_eq!(
        code, 0,
        "doctor must exit 0 when ~/.topology is a healthy marked root;\nstdout: {stdout}\nstderr: {stderr}"
    );
    // Use canonicalize-aware comparison: on macOS /var → /private/var.
    let canonical_topology = fs::canonicalize(&global_topology).unwrap_or(global_topology.clone());
    assert!(
        stdout.contains(&global_topology.to_string_lossy().to_string())
            || stdout.contains(&canonical_topology.to_string_lossy().to_string()),
        "framework root must be ~/.topology;\nstdout: {stdout}\nstderr: {stderr}"
    );
    // No fallback warning should appear on stderr.
    assert!(
        !stderr.contains("no framework root found"),
        "must not emit fallback warning when ~/.topology found;\nstderr: {stderr}"
    );

    let _ = fs::remove_dir_all(&base);
}

// ── (c) Binary-adjacent: binary at <root>/bin/gatekeeper resolves <root> ──────
//
// Layout:
//   /tmp/rr_binadj_<pid>/
//     framework/         ← marked root
//       skills/
//       AGENTS.md
//       security/rules.toml
//       instincts/
//       hooks/hook.sh
//       bin/gatekeeper   ← copy of the test binary (the exe we exec)
//     cwd/               ← unrelated directory; TOPOLOGY_ROOT unset
//
// Running `framework/bin/gatekeeper doctor` from `cwd/` with HOME remapped to a
// directory that has no `~/.topology`: binary-adjacent step walks up from exe path
// (`framework/bin/gatekeeper` → `framework/`) and finds the marked root.

#[test]
fn c_binary_adjacent_bin_layout_resolves_root() {
    let base = std::env::temp_dir().join(format!("rr_binadj_{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);

    let framework = base.join("framework");
    make_full_marked_root(&framework);

    // Install binary at <framework>/bin/gatekeeper
    let installed_bin = install_binary_at(&framework);

    let cwd = base.join("cwd");
    make_plain_dir(&cwd);

    let fake_home = base.join("home");
    fs::create_dir_all(&fake_home).unwrap();

    let (code, stdout, stderr) = run_binary(
        &installed_bin,
        &cwd,
        &["doctor"],
        &[("HOME", fake_home.to_str().unwrap())],
        &["TOPOLOGY_ROOT"],
    );

    assert_eq!(
        code, 0,
        "doctor must exit 0 when binary-adjacent framework is healthy;\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains(&framework.to_string_lossy().to_string()),
        "framework root must be the binary-adjacent <root>;\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        !stderr.contains("no framework root found"),
        "must not emit fallback warning when binary-adjacent root found;\nstderr: {stderr}"
    );

    let _ = fs::remove_dir_all(&base);
}

// ── (d) Doctor F1: no root anywhere → exit non-zero ───────────────────────────
//
// Layout: a plain cwd with no markers, no .topology, no marked binary-adjacent
// directory, HOME pointing to an empty directory. Resolution falls back to cwd.
// Doctor must detect the unmarked fallback root and exit non-zero (F1 FAIL).

#[test]
fn d_doctor_f1_no_root_anywhere_exits_nonzero() {
    let base = std::env::temp_dir().join(format!("rr_f1_{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);

    let cwd = base.join("cwd");
    make_plain_dir(&cwd);
    let fake_home = base.join("home");
    fs::create_dir_all(&fake_home).unwrap();

    // Run the default binary (from cargo target/); its binary-adjacent path is the
    // repo, which IS marked — so we must use the installed binary (a copy in a temp
    // dir without markers around it) to avoid binary-adjacent resolution hitting the repo.
    //
    // We place the binary inside a plain directory with no markers.
    let plain_bin_dir = base.join("plain_bin");
    fs::create_dir_all(&plain_bin_dir).unwrap();
    let src = PathBuf::from(env!("CARGO_BIN_EXE_gatekeeper"));
    let dst = plain_bin_dir.join("gatekeeper");
    fs::copy(&src, &dst).unwrap();
    fs::set_permissions(&dst, fs::Permissions::from_mode(0o755)).unwrap();

    let (code, stdout, stderr) = run_binary(
        &dst,
        &cwd,
        &["doctor"],
        &[("HOME", fake_home.to_str().unwrap())],
        &["TOPOLOGY_ROOT"],
    );

    assert_ne!(
        code, 0,
        "doctor must exit non-zero when no marked root is found (F1);\nstdout: {stdout}\nstderr: {stderr}"
    );
    // Must mention the F1 failure (unmarked root).
    assert!(
        stdout.contains("FAIL") || stderr.contains("FAIL"),
        "output must contain FAIL for the unmarked root;\nstdout: {stdout}\nstderr: {stderr}"
    );

    let _ = fs::remove_dir_all(&base);
}

// ── (e) Doctor F2: cwd inside a payload clone with VERSION → exit non-zero ────
//
// Layout:
//   /tmp/rr_f2_<pid>/
//     payload/           ← marked root with VERSION file (simulates installed payload)
//       skills/
//       AGENTS.md
//       security/rules.toml
//       instincts/
//       hooks/hook.sh
//       VERSION           ← presence signals "payload install"
//       bin/gatekeeper   ← copy of binary
//     payload/work/      ← cwd (inside the payload — user accidentally cd'd in)
//
// Doctor F2: project == framework AND a VERSION file is present → FAIL.

#[test]
fn e_doctor_f2_cwd_inside_payload_exits_nonzero() {
    let base = std::env::temp_dir().join(format!("rr_f2_{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);

    let payload = base.join("payload");
    make_full_marked_root(&payload);

    // Write a VERSION file to simulate a payload install (not a dev checkout).
    fs::write(
        payload.join("VERSION"),
        "version = \"0.5.0\"\nrules_schema = 1\n",
    )
    .unwrap();

    // Install binary inside the payload.
    let installed_bin = install_binary_at(&payload);

    // cwd is inside the payload (simulates user cd-ing into the payload clone)
    let work_dir = payload.join("work");
    fs::create_dir_all(&work_dir).unwrap();

    let fake_home = base.join("home");
    fs::create_dir_all(&fake_home).unwrap();

    let (code, stdout, stderr) = run_binary(
        &installed_bin,
        &work_dir,
        &["doctor"],
        &[("HOME", fake_home.to_str().unwrap())],
        &["TOPOLOGY_ROOT"],
    );

    assert_ne!(
        code, 0,
        "doctor must exit non-zero when cwd is inside a payload clone (F2);\nstdout: {stdout}\nstderr: {stderr}"
    );
    // Must contain a FAIL mentioning the inside-payload condition.
    assert!(
        stdout.contains("FAIL") || stderr.contains("FAIL"),
        "output must contain FAIL for the inside-payload condition;\nstdout: {stdout}\nstderr: {stderr}"
    );

    let _ = fs::remove_dir_all(&base);
}
