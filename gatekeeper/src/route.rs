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
    fn path_glob_match_parity() {
        assert!(path_glob_match("hooks/x.sh", "hooks/*"));
        assert!(path_glob_match(
            "gatekeeper/src/scan.rs",
            "gatekeeper/src/scan.rs"
        ));
        assert!(path_glob_match("src/a/secret.txt", "*secret*"));
        assert!(!path_glob_match("README.md", "hooks/*"));
    }
}
