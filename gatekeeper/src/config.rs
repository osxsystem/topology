//! Per-project topology config — `<artifacts_root>/config.toml`.
//!
//! All keys are optional. Unknown keys are silently ignored (forward compatibility).
//! A missing file yields all defaults.
//!
//! **Config strictness (spec § shadow-convention):**
//! - A malformed TOML file causes `LoadResult::ParseFailed` — the three hardened gates
//!   (`check verify`, `check design`, `check finish`) treat this as exit 2. Non-gate
//!   commands keep the existing warn-and-default behaviour via `load()`.
//! - A known key with an invalid value causes the owning gate to exit 2 with an
//!   actionable message.

use std::path::Path;
use toml::Value;

// ── public result type ────────────────────────────────────────────────────────

/// The outcome of loading `<artifacts_root>/config.toml`.
#[derive(Debug)]
pub enum LoadResult {
    /// The file was absent — use defaults everywhere.
    Missing,
    /// The file was present but failed TOML parsing.
    ParseFailed(String),
    /// Parsed successfully.
    Ok(ProjectConfig),
}

impl LoadResult {
    /// Return the config or a default. Emits a warning on `ParseFailed`.
    pub fn unwrap_or_default_warn(self, path: &Path) -> ProjectConfig {
        match self {
            LoadResult::Ok(c) => c,
            LoadResult::Missing => ProjectConfig::default(),
            LoadResult::ParseFailed(e) => {
                eprintln!(
                    "topology: warning: malformed {}: {e} — using defaults",
                    path.display()
                );
                ProjectConfig::default()
            }
        }
    }

    /// Return whether this is `ParseFailed`.
    // used in unit tests and integration test helpers
    #[allow(dead_code)]
    pub fn is_parse_failed(&self) -> bool {
        matches!(self, LoadResult::ParseFailed(_))
    }
}

// ── project config ────────────────────────────────────────────────────────────

/// Project-level configuration loaded from `<artifacts_root>/config.toml`.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct ProjectConfig {
    /// The default integration branch for the `review` gate (`--base` flag overrides this).
    /// Precedence: `--base` flag > `base_branch` > detection > built-in default "main".
    pub base_branch: Option<String>,

    /// The test command run by `check finish` when no `-- <cmd...>` is given on the CLI.
    /// Explicit `-- cmd` on the CLI always overrides this.
    pub test_command: Option<String>,

    // ── hardened verify settings (§3) ────────────────────────────────────────
    /// `[verify] mode` — `"presence"` (default) | `"replay"`.
    pub verify_mode: VerifyMode,
    /// `[verify] replay_timeout_secs` — default 300.
    pub replay_timeout_secs: u64,
    /// `[verify] allowed_command_prefixes` — token-boundary list.
    pub allowed_command_prefixes: Vec<String>,

    // ── hardened design settings (§4) ────────────────────────────────────────
    /// `[design] substance_floor` — default false.
    pub design_substance_floor: bool,
    /// `[design] approval` — `"status-line"` (default) | `"human-commit"`.
    pub design_approval: DesignApproval,

    // ── hardened finish settings (§5) ────────────────────────────────────────
    /// `[finish] require_test_count` — default false.
    pub finish_require_test_count: bool,
    /// `[finish] extra_count_patterns` — user-supplied regex escape-hatch.
    pub finish_extra_count_patterns: Vec<String>,
}

// ── hardened enum types ───────────────────────────────────────────────────────

/// `[verify] mode`
#[derive(Debug, Default, PartialEq, Eq, Clone, Copy)]
pub enum VerifyMode {
    #[default]
    Presence,
    Replay,
}

impl VerifyMode {
    /// Parse from a TOML string value; returns `Err` for a known-but-invalid value.
    pub fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "presence" => Ok(VerifyMode::Presence),
            "replay" => Ok(VerifyMode::Replay),
            other => Err(format!(
                "invalid value for [verify] mode = {:?}; expected \"presence\" or \"replay\"",
                other
            )),
        }
    }
}

/// `[design] approval`
#[derive(Debug, Default, PartialEq, Eq, Clone, Copy)]
pub enum DesignApproval {
    #[default]
    StatusLine,
    HumanCommit,
}

impl DesignApproval {
    pub fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "status-line" => Ok(DesignApproval::StatusLine),
            "human-commit" => Ok(DesignApproval::HumanCommit),
            other => Err(format!(
                "invalid value for [design] approval = {:?}; expected \"status-line\" or \"human-commit\"",
                other
            )),
        }
    }
}

// ── default allowed_command_prefixes ─────────────────────────────────────────

fn default_allowed_prefixes() -> Vec<String> {
    vec![
        "cargo test".to_string(),
        "cargo run".to_string(),
        "just".to_string(),
        "git diff".to_string(),
        "git log".to_string(),
        "git show".to_string(),
        "git status".to_string(),
    ]
}

// ── Default impl (manually: includes non-zero defaults like timeout=300) ─────

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            base_branch: None,
            test_command: None,
            verify_mode: VerifyMode::default(),
            replay_timeout_secs: 300,
            allowed_command_prefixes: default_allowed_prefixes(),
            design_substance_floor: false,
            design_approval: DesignApproval::default(),
            finish_require_test_count: false,
            finish_extra_count_patterns: Vec::new(),
        }
    }
}

// ── loading ───────────────────────────────────────────────────────────────────

impl ProjectConfig {
    /// Load from `<artifacts_root>/config.toml`, returning defaults on any error.
    ///
    /// - Missing file → silent, all `None`.
    /// - Malformed TOML → warn to stderr, all `None`.
    /// - Unknown keys → silently ignored.
    ///
    /// Non-gate callers use this; gate callers should use `load_strict` so that
    /// a parse failure causes exit 2 rather than a silent revert to defaults.
    pub fn load(artifacts_root: &Path) -> Self {
        let path = artifacts_root.join("config.toml");
        Self::load_result(artifacts_root).unwrap_or_default_warn(&path)
    }

    /// Load and return a `LoadResult` — callers decide how to handle `ParseFailed`.
    pub fn load_result(artifacts_root: &Path) -> LoadResult {
        let path = artifacts_root.join("config.toml");
        let raw = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(_) => return LoadResult::Missing,
        };
        match raw.parse::<Value>() {
            Ok(val) => match Self::from_toml_strict(&val) {
                Ok(cfg) => LoadResult::Ok(cfg),
                Err(e) => LoadResult::ParseFailed(e),
            },
            Err(e) => LoadResult::ParseFailed(e.to_string()),
        }
    }

    /// Parse a TOML `Value` into a `ProjectConfig`.
    ///
    /// Returns `Err` with an actionable message if a *known* key has an *invalid* value.
    /// Unknown keys are silently ignored.
    pub(crate) fn from_toml_strict(val: &Value) -> Result<Self, String> {
        let table = match val.as_table() {
            Some(t) => t,
            None => return Ok(Self::default()),
        };

        // Top-level simple keys
        let base_branch = table
            .get("base_branch")
            .and_then(|v| v.as_str())
            .map(str::to_owned);
        let test_command = table
            .get("test_command")
            .and_then(|v| v.as_str())
            .map(str::to_owned);

        // [verify] sub-table
        let mut verify_mode = VerifyMode::default();
        let mut replay_timeout_secs: u64 = 300;
        let mut allowed_command_prefixes = default_allowed_prefixes();

        if let Some(verify_tbl) = table.get("verify").and_then(|v| v.as_table()) {
            if let Some(mode_val) = verify_tbl.get("mode") {
                let mode_str = mode_val.as_str().unwrap_or("");
                verify_mode = VerifyMode::from_str(mode_str)?;
            }
            if let Some(timeout_val) = verify_tbl.get("replay_timeout_secs") {
                if let Some(n) = timeout_val.as_integer() {
                    if n > 0 {
                        replay_timeout_secs = n as u64;
                    }
                }
            }
            if let Some(prefixes_val) = verify_tbl.get("allowed_command_prefixes") {
                if let Some(arr) = prefixes_val.as_array() {
                    allowed_command_prefixes = arr
                        .iter()
                        .filter_map(|v| v.as_str())
                        .map(str::to_owned)
                        .collect();
                }
            }
        }

        // [design] sub-table
        let mut design_substance_floor = false;
        let mut design_approval = DesignApproval::default();

        if let Some(design_tbl) = table.get("design").and_then(|v| v.as_table()) {
            if let Some(sf_val) = design_tbl.get("substance_floor") {
                design_substance_floor = sf_val.as_bool().unwrap_or(false);
            }
            if let Some(appr_val) = design_tbl.get("approval") {
                let appr_str = appr_val.as_str().unwrap_or("");
                design_approval = DesignApproval::from_str(appr_str)?;
            }
        }

        // [finish] sub-table
        let mut finish_require_test_count = false;
        let mut finish_extra_count_patterns = Vec::new();

        if let Some(finish_tbl) = table.get("finish").and_then(|v| v.as_table()) {
            if let Some(rtc_val) = finish_tbl.get("require_test_count") {
                finish_require_test_count = rtc_val.as_bool().unwrap_or(false);
            }
            if let Some(ecp_val) = finish_tbl.get("extra_count_patterns") {
                if let Some(arr) = ecp_val.as_array() {
                    finish_extra_count_patterns = arr
                        .iter()
                        .filter_map(|v| v.as_str())
                        .map(str::to_owned)
                        .collect();
                }
            }
        }

        Ok(Self {
            base_branch,
            test_command,
            verify_mode,
            replay_timeout_secs,
            allowed_command_prefixes,
            design_substance_floor,
            design_approval,
            finish_require_test_count,
            finish_extra_count_patterns,
        })
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

    /// Malformed TOML: non-gate code paths (load()) warn and return defaults.
    /// Gate code paths use load_result() and check is_parse_failed().
    #[test]
    fn malformed_toml_warns_and_returns_defaults_for_non_gate_paths() {
        let dir = tmp("malformed");
        fs::write(dir.join("config.toml"), "base_branch = [\nnot valid toml\n").unwrap();
        // Non-gate path: load() must not panic; must return defaults.
        let cfg = ProjectConfig::load(&dir);
        assert_eq!(cfg, ProjectConfig::default());
        let _ = fs::remove_dir_all(&dir);
    }

    /// Malformed TOML: load_result() returns ParseFailed (gate paths use this).
    #[test]
    fn malformed_toml_load_result_is_parse_failed() {
        let dir = tmp("malformed_strict");
        fs::write(dir.join("config.toml"), "base_branch = [\nnot valid toml\n").unwrap();
        let result = ProjectConfig::load_result(&dir);
        assert!(
            result.is_parse_failed(),
            "malformed TOML must return ParseFailed; got: {result:?}"
        );
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

    #[test]
    fn verify_mode_replay_parsed() {
        let dir = tmp("verify_replay");
        fs::write(dir.join("config.toml"), "[verify]\nmode = \"replay\"\n").unwrap();
        let cfg = ProjectConfig::load(&dir);
        assert_eq!(cfg.verify_mode, VerifyMode::Replay);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn verify_mode_invalid_returns_error() {
        let dir = tmp("verify_invalid");
        fs::write(dir.join("config.toml"), "[verify]\nmode = \"bogus\"\n").unwrap();
        let result = ProjectConfig::load_result(&dir);
        assert!(
            result.is_parse_failed(),
            "invalid known value for mode must be ParseFailed"
        );
        if let LoadResult::ParseFailed(e) = &result {
            assert!(
                e.contains("bogus"),
                "error message should mention the bad value: {e}"
            );
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn design_approval_invalid_returns_error() {
        let dir = tmp("design_invalid");
        fs::write(dir.join("config.toml"), "[design]\napproval = \"bogus\"\n").unwrap();
        let result = ProjectConfig::load_result(&dir);
        assert!(result.is_parse_failed());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn default_allowed_prefixes_present() {
        let dir = tmp("default_prefixes");
        fs::write(dir.join("config.toml"), "").unwrap();
        let cfg = ProjectConfig::load(&dir);
        assert!(cfg
            .allowed_command_prefixes
            .contains(&"cargo test".to_string()));
        assert!(cfg
            .allowed_command_prefixes
            .contains(&"git diff".to_string()));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn finish_require_test_count_parsed() {
        let dir = tmp("finish_rtc");
        fs::write(
            dir.join("config.toml"),
            "[finish]\nrequire_test_count = true\n",
        )
        .unwrap();
        let cfg = ProjectConfig::load(&dir);
        assert!(cfg.finish_require_test_count);
        let _ = fs::remove_dir_all(&dir);
    }
}
