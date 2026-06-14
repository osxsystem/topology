//! Path-triggered skill routing.
//!
//! Companion to the keyword router (`route()` in `main.rs`): instead of keying on the
//! prompt's words, it keys on the file paths an edit touches, reading each skill's
//! `pathTriggers.globs` from `hooks/skill-rules.json`.

/// Dep-free path glob matcher.
///
/// Mirrors the documented semantics of the security scanner's `glob_match`
/// (`scan.rs:498-527`); the `path_glob_match_parity` unit test below pins the shared cases
/// against drift (design R3). Kept a separate copy so the routing module does not edit the
/// protected scanner (design D1 / approach 1).
///
/// Semantics:
/// - a trailing `/` makes the glob a directory prefix (matches the dir itself or anything beneath);
/// - `*` matches any run of characters (including none); the first literal segment is anchored at
///   the start and the last at the end;
/// - a `*`-free glob therefore matches exactly.
pub(crate) fn path_glob_match(path: &str, glob: &str) -> bool {
    if let Some(prefix) = glob.strip_suffix('/') {
        return path == prefix || path.starts_with(&format!("{prefix}/"));
    }
    let parts: Vec<&str> = glob.split('*').collect();
    let mut pos = 0;
    for (idx, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if idx == 0 {
            if !path[pos..].starts_with(part) {
                return false;
            }
            pos += part.len();
        } else if idx == parts.len() - 1 {
            return path[pos..].ends_with(part) && path.len() - pos >= part.len();
        } else {
            match path[pos..].find(part) {
                Some(off) => pos += off + part.len(),
                None => return false,
            }
        }
    }
    glob.ends_with('*') || pos == path.len()
}

/// Given parsed skill-rules JSON and a list of touched paths, return (skill, enforcement) matches.
///
/// Mirrors the keyword router (`route()` in `main.rs`) but reads `pathTriggers.globs`: a skill
/// matches if ANY of its globs matches ANY of the paths. Results are deduped and sorted.
pub(crate) fn route_by_paths(rules: &serde_json::Value, paths: &[&str]) -> Vec<(String, String)> {
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
        let globs = cfg
            .get("pathTriggers")
            .and_then(|t| t.get("globs"))
            .and_then(|g| g.as_array());
        if let Some(globs) = globs {
            let hit = globs
                .iter()
                .filter_map(|g| g.as_str())
                .any(|glob| paths.iter().any(|p| path_glob_match(p, glob)));
            if hit {
                out.push((name.clone(), enforcement));
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Route skills from a PostToolUse hook event (JSON on stdin).
///
/// Mirrors the JSON shape `scan --hook` consumes (`scan.rs` `HookEvent`/`ToolInput`) but with a
/// local minimal deserialize struct, so the protected scanner is untouched (design D1). On any
/// parse failure or a missing `file_path`, returns an empty vec (never panics) — the hook is
/// advisory and must fail open (design D2). Otherwise routes by the touched path via
/// `route_by_paths`.
pub(crate) fn route_by_hook_json(rules: &serde_json::Value, stdin: &str) -> Vec<(String, String)> {
    #[derive(serde::Deserialize)]
    struct HookEvent {
        tool_input: Option<ToolInput>,
    }
    #[derive(serde::Deserialize)]
    struct ToolInput {
        file_path: Option<String>,
    }
    let Ok(event) = serde_json::from_str::<HookEvent>(stdin) else {
        return Vec::new();
    };
    let Some(file_path) = event.tool_input.and_then(|t| t.file_path) else {
        return Vec::new();
    };
    route_by_paths(rules, &[&file_path])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_by_paths_matches_security() {
        let rules = serde_json::json!({
            "skills": {
                "security-scanning": {
                    "enforcement": "require",
                    "pathTriggers": { "globs": ["hooks/*"] }
                }
            }
        });
        assert_eq!(
            route_by_paths(&rules, &["hooks/x.sh"]),
            vec![("security-scanning".to_string(), "require".to_string())]
        );
        assert_eq!(
            route_by_paths(&rules, &["README.md"]),
            Vec::<(String, String)>::new()
        );
    }

    #[test]
    fn route_by_hook_json_extracts_path() {
        let rules = serde_json::json!({
            "skills": {
                "security-scanning": {
                    "enforcement": "require",
                    "pathTriggers": { "globs": ["hooks/*"] }
                }
            }
        });
        // A trigger path inside the PostToolUse JSON routes the skill.
        let trigger = r#"{"tool_name":"Edit","tool_input":{"file_path":"hooks/x.sh"}}"#;
        assert_eq!(
            route_by_hook_json(&rules, trigger),
            vec![("security-scanning".to_string(), "require".to_string())]
        );
        // A non-trigger path routes nothing.
        let non_trigger = r#"{"tool_name":"Edit","tool_input":{"file_path":"README.md"}}"#;
        assert_eq!(
            route_by_hook_json(&rules, non_trigger),
            Vec::<(String, String)>::new()
        );
        // Malformed JSON returns empty (never panics).
        assert_eq!(
            route_by_hook_json(&rules, "}{ not json"),
            Vec::<(String, String)>::new()
        );
        // Missing file_path returns empty.
        let no_path = r#"{"tool_name":"Edit","tool_input":{}}"#;
        assert_eq!(
            route_by_hook_json(&rules, no_path),
            Vec::<(String, String)>::new()
        );
    }

    #[test]
    fn path_glob_match_parity() {
        // Wildcard prefix / exact / substring / non-match (basic cases).
        assert!(path_glob_match("hooks/x.sh", "hooks/*"));
        assert!(path_glob_match(
            "gatekeeper/src/scan.rs",
            "gatekeeper/src/scan.rs"
        ));
        assert!(path_glob_match("src/a/secret.txt", "*secret*"));
        assert!(!path_glob_match("README.md", "hooks/*"));

        // Drift tripwire vs scan.rs:498-527 — the load-bearing edge cases (R3):
        // trailing-`/` directory glob matches the dir itself and anything beneath, but not a
        // sibling whose name merely starts with the prefix, nor a nested same-named dir.
        assert!(path_glob_match("tests/fixtures/", "tests/fixtures/"));
        assert!(path_glob_match("tests/fixtures/neg.txt", "tests/fixtures/"));
        assert!(!path_glob_match("tests/fixtures-bak/x", "tests/fixtures/"));
        assert!(!path_glob_match("pkg/tests/fixtures/x", "tests/fixtures/"));
        // exact glob (no `*`) must match exactly — a longer string does not.
        assert!(!path_glob_match("Cargo.tomlx", "Cargo.toml"));
        // middle `*` spans path separators; first/last literals anchored.
        assert!(path_glob_match("src/a/b.rs", "src/*b.rs"));
        assert!(!path_glob_match("src/a/b.rsx", "src/*b.rs"));
    }
}
