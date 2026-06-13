# Topology — task runner (just)
# Thin one-line wrappers around the project's tooling. The Rust crate is
# `gatekeeper` (manifest: gatekeeper/Cargo.toml). `check` is the OFFLINE-safe
# aggregate; `ci` adds the network/slow gates (deny, links).

# Show available recipes.
default:
    @just --list

# Bootstrap a fresh framework clone or worktree:
#   1. install hooks/pre-commit.sh as the git pre-commit hook (copy, not symlink; survives
#      in-place edits); stops with an error if a non-topology pre-commit hook already exists.
#   2. build the release binary, then `gatekeeper adapt --harness claude` to (re)generate the
#      portable .claude/settings.json. The RELEASE build is load-bearing: portable settings drop
#      GATEKEEPER_BIN, so the hooks resolve gatekeeper/target/release/gatekeeper (see
#      docs/DEVELOPMENT.md + ADR-0019). adapt writes settings.json only on drift → re-run no-op.
setup:
    @HOOKS_DIR="$(git rev-parse --git-path hooks)" && \
    DEST="$HOOKS_DIR/pre-commit" && \
    if [ -f "$DEST" ]; then \
        if grep -q "Topology pre-commit" "$DEST" 2>/dev/null; then \
            cp hooks/pre-commit.sh "$DEST" && chmod +x "$DEST" && \
            echo "setup: updated $DEST"; \
        else \
            echo "setup: $DEST already exists and does not appear to be a topology hook." >&2; \
            echo "       Remove it manually and re-run 'just setup' to install the topology hook." >&2; \
            exit 1; \
        fi; \
    else \
        cp hooks/pre-commit.sh "$DEST" && chmod +x "$DEST" && \
        echo "setup: installed $DEST"; \
    fi
    @echo "setup: building gatekeeper (release) and wiring .claude/settings.json…"
    cargo build --release --manifest-path gatekeeper/Cargo.toml
    ./gatekeeper/target/release/gatekeeper adapt --harness claude

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

# Smoke-test the payload builder (builds into a tempdir, asserts manifest + VERSION).
test-payload:
    bash scripts/test-build-payload.sh

# Test fetch-gatekeeper.sh version resolution precedence (no network).
test-fetch:
    bash scripts/test-fetch-version.sh

# Refresh the gitleaks provider-rule REVIEW file (never edits rules.toml; no-op offline/CI).
sync-gitleaks:
    bash scripts/sync-gitleaks-rules.sh

# Offline end-to-end: build payload, serve via file://, install, assert gatekeeper works.
test-e2e:
    bash scripts/test-payload-e2e.sh

# Offline end-to-end re-verification: real install against a reference project, assert the five outcomes.
test-e2e-reference:
    bash scripts/test-e2e-reference.sh

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
