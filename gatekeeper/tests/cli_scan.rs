use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// A minimal framework root: a `skills/` marker (so `framework_root()` resolves here) and a
/// `security/rules.toml` with one content rule, the command rules under test, and protected paths.
fn scratch_root(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("topo_scan_{tag}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("skills")).unwrap();
    fs::create_dir_all(root.join("security")).unwrap();
    let rules = r#"schema_version = 1
[[rule]]
id = "aws"
kind = "content"
severity = "block"
description = "AWS key"
pattern = '\b(AKIA|ASIA)[0-9A-Z]{16}\b'
[[rule]]
id = "curl-pipe-shell"
kind = "command"
severity = "block"
description = "curl | sh"
pattern = '\b(curl|wget)\b[^|]*\|\s*(sudo\s+)?(sh|bash|zsh)\b'
[[rule]]
id = "rm-rf-root"
kind = "command"
severity = "block"
description = "rm -rf /"
pattern = '\brm\s+(-[a-zA-Z]*\s+)*-[a-zA-Z]*[rR][a-zA-Z]*f[a-zA-Z]*\s+(-[a-zA-Z]*\s+)*/(\s|$)'
[[rule]]
id = "git-push-force"
kind = "command"
severity = "block"
description = "force push"
pattern = '\bgit\b.*\bpush\b.*(--force($|\s)|\s-f($|\s))'
[[rule]]
id = "git-commit-no-verify"
kind = "command"
severity = "block"
description = "no-verify bypass"
pattern = '\bgit\b.*\bcommit\b.*(--no-verify|\s-n($|\s))'
[integrity]
protected_paths = ["security/rules.toml", "hooks/pre-commit.sh"]
"#;
    fs::write(root.join("security").join("rules.toml"), rules).unwrap();
    root
}

/// Run `gatekeeper <args>` from `cwd`, feeding `stdin`. Returns (exit code, stdout).
fn run(cwd: &Path, args: &[&str], stdin: &[u8]) -> (i32, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_gatekeeper"))
        .current_dir(cwd)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(stdin).unwrap();
    let out = child.wait_with_output().unwrap();
    (out.status.code().unwrap_or(-1), String::from_utf8_lossy(&out.stdout).into_owned())
}

/// An AWS-shaped key built by concatenation, so this test file never contains a literal key.
fn planted_key() -> String {
    format!("AKIA{}", "1234567890ABCDEF")
}

#[test]
fn content_blocks_planted_key_and_passes_clean() {
    let root = scratch_root("content");
    let (code, _) = run(&root, &["scan", "--content"], format!("k={}\n", planted_key()).as_bytes());
    assert_eq!(code, 1, "planted key must block");
    let (code, out) = run(&root, &["scan", "--content"], b"clean file\n");
    assert_eq!(code, 0, "clean input passes");
    assert!(out.is_empty(), "clean --content writes nothing to stdout");
    let _ = fs::remove_dir_all(&root);
}
