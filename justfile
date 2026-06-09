# Topology — task runner (just)
# Thin one-line wrappers around the project's tooling. The Rust crate is
# `gatekeeper` (manifest: gatekeeper/Cargo.toml). `check` is the OFFLINE-safe
# aggregate; `ci` adds the network/slow gates (deny, links).

# Show available recipes.
default:
    @just --list

# Format Rust sources in place.
fmt:
    cargo fmt --manifest-path gatekeeper/Cargo.toml

# Verify Rust formatting without writing.
fmt-check:
    cargo fmt --manifest-path gatekeeper/Cargo.toml --check

# Clippy with warnings as errors.
lint:
    cargo clippy --manifest-path gatekeeper/Cargo.toml -- -D warnings

# Run the Rust test suite.
test:
    cargo test --manifest-path gatekeeper/Cargo.toml

# Dependency/license/advisory audit (deny.toml lives in gatekeeper/).
deny:
    cargo deny --manifest-path gatekeeper/Cargo.toml check

# ShellCheck across hooks and scripts.
shell:
    shellcheck hooks/*.sh scripts/*.sh

# shfmt diff check (no writes; manual/advisory — intentionally NOT in `check` or the
# pre-commit gate, since the scripts use a deliberate hand-aligned style shfmt would undo).
shfmt:
    shfmt -d hooks scripts

# Spell check.
typos:
    typos

# Markdown link check — NEEDS NETWORK (lychee fetches URLs).
links:
    lychee --config lychee.toml 'docs/**/*.md' '*.md'

# Docs-coverage lint: skills frontmatter, ADR index, ROADMAP evidence paths.
docs:
    cargo run --manifest-path gatekeeper/Cargo.toml --quiet -- check docs

# OFFLINE-safe aggregate: formatting, lint, tests, shell, spelling.
check: fmt-check lint test shell typos docs

# Full gate: everything, including network/slow checks (deny, links).
ci: check deny links
