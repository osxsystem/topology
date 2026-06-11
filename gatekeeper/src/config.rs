//! Per-project topology config — `<artifacts_root>/config.toml`.
//!
//! All keys are optional. Unknown keys are silently ignored (forward compatibility).
//! A missing file yields all defaults. A malformed TOML file emits a stderr warning and
//! falls back to defaults — a bad config must never hard-fail a gate.

use std::path::Path;
use toml::Value;

/// Project-level configuration loaded from `<artifacts_root>/config.toml`.
#[derive(Debug, Default, PartialEq, Eq, Clone)]
pub struct ProjectConfig {
    /// The default integration branch for the `review` gate (`--base` flag overrides this).
    /// Precedence: `--base` flag > `base_branch` > detection > built-in default "main".
    pub base_branch: Option<String>,

    /// The test command run by `check finish` when no `-- <cmd...>` is given on the CLI.
    /// Explicit `-- cmd` on the CLI always overrides this.
    pub test_command: Option<String>,
}

impl ProjectConfig {
    /// Load from `<artifacts_root>/config.toml`, returning defaults on any error.
    ///
    /// - Missing file → silent, all `None`.
    /// - Malformed TOML → warn to stderr, all `None`.
    /// - Unknown keys → silently ignored.
    pub fn load(artifacts_root: &Path) -> Self {
        let path = artifacts_root.join("config.toml");
        let raw = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(_) => return Self::default(), // missing file is fine
        };
        match raw.parse::<Value>() {
            Ok(val) => Self::from_toml(&val),
            Err(e) => {
                eprintln!(
                    "topology: warning: malformed {}: {e} — using defaults",
                    path.display()
                );
                Self::default()
            }
        }
    }

    fn from_toml(val: &Value) -> Self {
        let table = match val.as_table() {
            Some(t) => t,
            None => return Self::default(),
        };
        let base_branch = table
            .get("base_branch")
            .and_then(|v| v.as_str())
            .map(str::to_owned);
        let test_command = table
            .get("test_command")
            .and_then(|v| v.as_str())
            .map(str::to_owned);
        Self {
            base_branch,
            test_command,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("topo_config_{tag}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn missing_file_returns_defaults() {
        let dir = tmp("missing");
        let cfg = ProjectConfig::load(&dir);
        assert_eq!(cfg, ProjectConfig::default());
        assert!(cfg.base_branch.is_none());
        assert!(cfg.test_command.is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn full_keys_parsed() {
        let dir = tmp("full");
        fs::write(
            dir.join("config.toml"),
            "base_branch = \"master\"\ntest_command = \"npm test\"\n",
        )
        .unwrap();
        let cfg = ProjectConfig::load(&dir);
        assert_eq!(cfg.base_branch.as_deref(), Some("master"));
        assert_eq!(cfg.test_command.as_deref(), Some("npm test"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn partial_keys_only_base_branch() {
        let dir = tmp("partial_base");
        fs::write(dir.join("config.toml"), "base_branch = \"develop\"\n").unwrap();
        let cfg = ProjectConfig::load(&dir);
        assert_eq!(cfg.base_branch.as_deref(), Some("develop"));
        assert!(cfg.test_command.is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn partial_keys_only_test_command() {
        let dir = tmp("partial_cmd");
        fs::write(dir.join("config.toml"), "test_command = \"cargo test\"\n").unwrap();
        let cfg = ProjectConfig::load(&dir);
        assert!(cfg.base_branch.is_none());
        assert_eq!(cfg.test_command.as_deref(), Some("cargo test"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn unknown_keys_silently_ignored() {
        let dir = tmp("unknown");
        fs::write(
            dir.join("config.toml"),
            "base_branch = \"main\"\nfuture_key = \"something\"\n",
        )
        .unwrap();
        let cfg = ProjectConfig::load(&dir);
        // Known keys still parsed; no panic from the unknown key.
        assert_eq!(cfg.base_branch.as_deref(), Some("main"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn malformed_toml_warns_and_returns_defaults() {
        let dir = tmp("malformed");
        fs::write(dir.join("config.toml"), "base_branch = [\nnot valid toml\n").unwrap();
        // Must not panic; must return defaults.
        let cfg = ProjectConfig::load(&dir);
        assert_eq!(cfg, ProjectConfig::default());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_file_returns_defaults() {
        let dir = tmp("empty");
        fs::write(dir.join("config.toml"), "").unwrap();
        let cfg = ProjectConfig::load(&dir);
        assert_eq!(cfg, ProjectConfig::default());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn commented_out_keys_return_defaults() {
        let dir = tmp("commented");
        fs::write(
            dir.join("config.toml"),
            "# base_branch = \"master\"\n# test_command = \"npm test\"\n",
        )
        .unwrap();
        let cfg = ProjectConfig::load(&dir);
        assert_eq!(cfg, ProjectConfig::default());
        let _ = fs::remove_dir_all(&dir);
    }
}
