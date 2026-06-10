#!/usr/bin/env bash
# Smoke test for scripts/build-payload.sh.
#
# Builds the payload into a tempdir, then asserts:
#   - The tarball manifest contains all required entries (AC-3 positive).
#   - The tarball contains no *.rs, docs/, .git/, or plugin files (AC-3 negative).
#   - VERSION parses with the grep-m1 idiom and matches gatekeeper/Cargo.toml (AC-4).
#
# Exit 0 on pass, 1 on any failure (individual failures printed before exit).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
BUILD_SCRIPT="$SCRIPT_DIR/build-payload.sh"

PASS=0
FAIL=0

pass() { echo "PASS $*"; PASS=$((PASS + 1)); }
fail() { echo "FAIL $*"; FAIL=$((FAIL + 1)); }

# ── Build into a tempdir ───────────────────────────────────────────────────────
TMPDIR_STAGE="$(mktemp -d)"
TMPDIR_WORK="$(mktemp -d)"
cleanup() { rm -rf "$TMPDIR_STAGE" "$TMPDIR_WORK"; }
trap cleanup EXIT

# Run from TMPDIR_WORK so the tarball lands there.
TARBALL="$(cd "$TMPDIR_WORK" && bash "$BUILD_SCRIPT" "$TMPDIR_STAGE")"

if [[ ! -f "$TARBALL" ]]; then
  echo "FATAL: build-payload.sh did not produce a tarball at '$TARBALL'" >&2
  exit 1
fi

# Get the listing once.
LISTING="$(tar -tzf "$TARBALL")"

# ── AC-3 positive: required manifest entries ──────────────────────────────────
REQUIRED_ENTRIES=(
  "hooks/skill-activation.sh"
  "hooks/security-scan.sh"
  "hooks/pre-commit.sh"
  "hooks/learn-capture.sh"
  "hooks/skill-rules.json"
  "security/rules.toml"
  "scripts/fetch-gatekeeper.sh"
  "VERSION"
)

for entry in "${REQUIRED_ENTRIES[@]}"; do
  if echo "$LISTING" | grep -qF "$entry"; then
    pass "tarball contains $entry"
  else
    fail "tarball missing $entry"
  fi
done

# skills/ directory must have at least one entry
if echo "$LISTING" | grep -q "^./skills/\|^skills/"; then
  pass "tarball contains skills/ entries"
else
  fail "tarball missing skills/ entries"
fi

# instincts/ directory must have at least one entry
if echo "$LISTING" | grep -q "^./instincts/\|^instincts/"; then
  pass "tarball contains instincts/ entries"
else
  fail "tarball missing instincts/ entries"
fi

# ── AC-3 negative: excluded entries ──────────────────────────────────────────
# No *.rs files
if echo "$LISTING" | grep -qE '\.rs$'; then
  fail "tarball must not contain *.rs files — found: $(echo "$LISTING" | grep -E '\.rs$' | head -3)"
else
  pass "tarball contains no *.rs files"
fi

# No docs/ entries
if echo "$LISTING" | grep -qE '^\.?/?(docs)/'; then
  fail "tarball must not contain docs/ — found: $(echo "$LISTING" | grep -E '^\.?/?(docs)/' | head -3)"
else
  pass "tarball contains no docs/"
fi

# No .git entries
if echo "$LISTING" | grep -qE '^\.?/?\.git'; then
  fail "tarball must not contain .git entries"
else
  pass "tarball contains no .git entries"
fi

# No plugin files (.claude-plugin/ or hooks/hooks.json or hooks/ensure-gatekeeper.sh)
if echo "$LISTING" | grep -qE '\.claude-plugin'; then
  fail "tarball must not contain .claude-plugin/ entries"
else
  pass "tarball contains no .claude-plugin/ entries"
fi

if echo "$LISTING" | grep -qF "hooks/hooks.json"; then
  fail "tarball must not contain hooks/hooks.json (plugin-only)"
else
  pass "tarball does not contain hooks/hooks.json"
fi

if echo "$LISTING" | grep -qF "hooks/ensure-gatekeeper.sh"; then
  fail "tarball must not contain hooks/ensure-gatekeeper.sh (plugin-only)"
else
  pass "tarball does not contain hooks/ensure-gatekeeper.sh"
fi

# ── AC-4: VERSION parses and matches Cargo.toml ───────────────────────────────
# Extract VERSION from the tarball
TMPDIR_UNPACK="$(mktemp -d)"
trap 'rm -rf "$TMPDIR_STAGE" "$TMPDIR_WORK" "$TMPDIR_UNPACK"' EXIT
tar -xzf "$TARBALL" -C "$TMPDIR_UNPACK"

VERSION_FILE="$TMPDIR_UNPACK/VERSION"
if [[ ! -f "$VERSION_FILE" ]]; then
  fail "VERSION file not found in unpacked tarball"
else
  pass "VERSION file present after unpack"

  # Parse with the line-anchored grep idiom (same idiom fetch-gatekeeper.sh and the spec specify)
  PARSED_VERSION="$(grep -m1 '^version' "$VERSION_FILE" | sed -E 's/^version[[:space:]]*=[[:space:]]*"([^"]*)".*/\1/')"
  if [[ -z "$PARSED_VERSION" ]]; then
    fail "VERSION file: grep-m1 idiom failed to parse version"
  else
    pass "VERSION file: grep-m1 idiom parsed version='$PARSED_VERSION'"
  fi

  PARSED_SCHEMA="$(grep -m1 '^rules_schema' "$VERSION_FILE" | sed -E 's/^rules_schema[[:space:]]*=[[:space:]]*([0-9]+).*/\1/')"
  if [[ -z "$PARSED_SCHEMA" ]]; then
    fail "VERSION file: grep-m1 idiom failed to parse rules_schema"
  else
    pass "VERSION file: grep-m1 idiom parsed rules_schema=$PARSED_SCHEMA"
  fi

  # Compare parsed version to Cargo.toml
  CARGO_VERSION="$(grep -m1 '^version' "$REPO_ROOT/gatekeeper/Cargo.toml" | sed -E 's/^version[[:space:]]*=[[:space:]]*"([^"]*)".*/\1/')"
  if [[ "$PARSED_VERSION" == "$CARGO_VERSION" ]]; then
    pass "VERSION matches Cargo.toml ($PARSED_VERSION)"
  else
    fail "VERSION mismatch: VERSION='$PARSED_VERSION' vs Cargo.toml='$CARGO_VERSION'"
  fi

  # Verify rules_schema matches scan.rs constant
  SCAN_SCHEMA="$(grep -m1 'const SCHEMA_VERSION' "$REPO_ROOT/gatekeeper/src/scan.rs" | sed -E 's/.*=[[:space:]]*([0-9]+).*/\1/')"
  if [[ "$PARSED_SCHEMA" == "$SCAN_SCHEMA" ]]; then
    pass "rules_schema matches scan.rs SCHEMA_VERSION ($PARSED_SCHEMA)"
  else
    fail "rules_schema mismatch: VERSION='$PARSED_SCHEMA' vs scan.rs='$SCAN_SCHEMA'"
  fi
fi

# ── Summary ────────────────────────────────────────────────────────────────────
echo ""
echo "test-build-payload: $PASS passed, $FAIL failed"
if [[ $FAIL -gt 0 ]]; then
  exit 1
fi
