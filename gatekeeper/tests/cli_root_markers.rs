//! Phase 8 red fixture — `.claude-plugin/` must stop being a root marker.
//!
//! A directory carrying `skills/` + `.claude-plugin/` is the standard layout of any
//! Claude Code plugin checkout. Since Phase 11 a marked git root self-governs
//! (resolution step 2), so keeping `.claude-plugin` in ROOT_MARKERS lets an unrelated
//! plugin repo claim to be a Topology framework root. After the Phase 8 retirement,
//! such a directory must NOT resolve as the framework root: resolution falls back to
//! cwd and doctor FAILs (F1, unmarked root).
//!
//! Red at introduction: with `.claude-plugin` still in ROOT_MARKERS this layout is
//! `SelfGoverned` and doctor exits 0 — the assertions below fail. Un-ignored in the
//! plugin-retirement commit.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

fn run_doctor(bin: &Path, cwd: &Path, home: &Path) -> (i32, String, String) {
    let out = Command::new(bin)
        .arg("doctor")
        .current_dir(cwd)
        .env_remove("TOPOLOGY_ROOT")
        .env("HOME", home)
        .output()
        .expect("failed to run gatekeeper");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
#[ignore = "red until the plugin retirement removes .claude-plugin from ROOT_MARKERS"]
fn plugin_checkout_layout_is_not_a_framework_root() {
    let base = std::env::temp_dir().join(format!("rm_plugin_marker_{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);

    // A git repo shaped like a Claude Code plugin checkout: skills/ + .claude-plugin/,
    // but neither AGENTS.md nor gatekeeper/.
    let plugin_repo = base.join("some-plugin");
    fs::create_dir_all(plugin_repo.join("skills")).unwrap();
    fs::create_dir_all(plugin_repo.join(".claude-plugin")).unwrap();
    let st = Command::new("git")
        .args(["init", "-q"])
        .current_dir(&plugin_repo)
        .status()
        .unwrap();
    assert!(st.success());

    // Binary installed in a plain dir so binary-adjacent cannot resolve anything;
    // HOME remapped to a dir with no .topology so the global step misses too.
    let plain_bin = base.join("bin");
    fs::create_dir_all(&plain_bin).unwrap();
    let src = PathBuf::from(env!("CARGO_BIN_EXE_gatekeeper"));
    let bin = plain_bin.join("gatekeeper");
    fs::copy(&src, &bin).unwrap();
    fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();
    let fake_home = base.join("home");
    fs::create_dir_all(&fake_home).unwrap();

    let (code, stdout, stderr) = run_doctor(&bin, &plugin_repo, &fake_home);

    // The plugin-shaped repo must not self-govern: resolution falls back and doctor
    // FAILs (F1). Before the marker removal this resolved `SelfGoverned` with exit 0.
    assert_ne!(
        code, 0,
        "a skills/ + .claude-plugin/ repo must not be a framework root;\n\
         stdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("resolved by: fallback (cwd)"),
        "expected fallback resolution, not self-governed;\nstdout: {stdout}\nstderr: {stderr}"
    );

    let _ = fs::remove_dir_all(&base);
}
