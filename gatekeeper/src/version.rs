//! Version seam — the single source of truth for the binary version and the rules-schema version.
//!
//! `--version` and `doctor` depend on this module; `check docs` is not a consumer (it reads no
//! schema version). Keeping the seam thin means neither consumer reaches into the private
//! `scan::SCHEMA_VERSION` const directly. See docs/adr/0010-packaging-distribution.md §2.

use crate::scan;

/// The tool version string from `Cargo.toml` at compile time.
pub fn tool() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// The rules-schema version the binary currently accepts.
/// Delegates to `scan::schema_version()` so the const stays private in scan.rs.
pub fn rules_schema() -> u32 {
    scan::schema_version()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan;

    #[test]
    fn tool_version_matches_cargo_pkg_version() {
        assert_eq!(tool(), env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn rules_schema_delegates_to_scan() {
        // Contract guard: rules_schema() must keep delegating to scan::schema_version().
        // This cannot go red while the delegation holds, but will catch a future re-declaration.
        assert_eq!(rules_schema(), scan::schema_version());
    }

    #[test]
    fn advertised_schema_is_accepted_by_parser() {
        // Round-trip: the schema version we print must be one the scanner accepts.
        // If the two decouple (e.g. someone bumps SCHEMA_VERSION but forgets version::rules_schema),
        // this test turns red.
        let toml = format!("schema_version = {}", rules_schema());
        assert!(
            scan::parse_rules_pub(&toml).is_ok(),
            "schema version {} is printed by --version but rejected by the parser",
            rules_schema()
        );
    }
}
