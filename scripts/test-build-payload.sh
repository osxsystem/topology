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
  # AGENTS.md is the ROOT_MARKERS sentinel required by is_marked_root(); without it
  # the unpacked payload tree cannot be resolved as the framework root (Note 4).
  "AGENTS.md"
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

# ── AC-marked-root: unpacked payload satisfies is_marked_root (skills/ + AGENTS.md) ──────
# is_marked_root in main.rs requires both: a skills/ directory AND at least one of
# ROOT_MARKERS = ["AGENTS.md", "gatekeeper"]. The unpacked tarball
# ships skills/ (tested above) and AGENTS.md (asserted in REQUIRED_ENTRIES); verify
# both are present together in the unpacked tree so a future build regression trips here.
if [[ -d "$TMPDIR_UNPACK/skills" ]]; then
  pass "unpacked payload has skills/ (required by is_marked_root)"
else
  fail "unpacked payload missing skills/ — is_marked_root would fail without it"
fi

if [[ -f "$TMPDIR_UNPACK/AGENTS.md" ]]; then
  pass "unpacked payload has AGENTS.md (ROOT_MARKERS sentinel for is_marked_root)"
else
  fail "unpacked payload missing AGENTS.md — is_marked_root would fall back to \$HOME without a marker"
fi

# ── AC-non-empty-stage: build-payload.sh refuses a non-empty stage dir ────────
# Ensures stale files from a previous run cannot silently leak into a new payload.
TMPDIR_STALE="$(mktemp -d)"
echo "stale-file" > "$TMPDIR_STALE/stale.txt"
STALE_OUT="$(bash "$BUILD_SCRIPT" "$TMPDIR_STALE" 2>&1)" && STALE_EXIT=0 || STALE_EXIT=$?
rm -rf "$TMPDIR_STALE"
if [[ "$STALE_EXIT" -ne 0 ]] && echo "$STALE_OUT" | grep -q "non-empty"; then
  pass "build-payload refuses non-empty stage dir with clear error"
else
  fail "build-payload should exit non-zero with 'non-empty' message for non-empty stage dir (exit=$STALE_EXIT, output: $STALE_OUT)"
fi

# ── AC-version-arg-beats-env: explicit arg overrides TOPOLOGY_VERSION env var ─
# The env var is the fallback for automation; an explicit positional argument must
# win so CI can pin a version without the env leaking an unintended override.
TMPDIR_VARG_STAGE="$(mktemp -d)"
TMPDIR_VARG_WORK="$(mktemp -d)"
trap 'rm -rf "$TMPDIR_STAGE" "$TMPDIR_WORK" "$TMPDIR_UNPACK" "$TMPDIR_VARG_STAGE" "$TMPDIR_VARG_WORK"' EXIT

VARG_TARBALL="$(cd "$TMPDIR_VARG_WORK" && TOPOLOGY_VERSION="99.99.99" bash "$BUILD_SCRIPT" "$TMPDIR_VARG_STAGE" "1.2.3")"
if [[ -f "$VARG_TARBALL" ]]; then
  VARG_UNPACK="$(mktemp -d)"
  tar -xzf "$VARG_TARBALL" -C "$VARG_UNPACK"
  VARG_VER="$(grep -m1 '^version' "$VARG_UNPACK/VERSION" | sed -E 's/^version[[:space:]]*=[[:space:]]*"([^"]*)".*/\1/')"
  rm -rf "$VARG_UNPACK"
  if [[ "$VARG_VER" == "1.2.3" ]]; then
    pass "explicit version arg (1.2.3) beats TOPOLOGY_VERSION env (99.99.99)"
  else
    fail "explicit version arg should beat env; got VERSION='$VARG_VER' (expected 1.2.3)"
  fi
else
  fail "build-payload did not produce a tarball when testing version-arg-beats-env"
fi

# ── Phase 10 assertions (guarded behind PHASE10_RED=1 until Task 5) ──────────
# These assertions are committed red in Task 1 and un-guarded in Task 5 once
# build-payload.sh ships templates/ and the skill sweep is complete.
if [[ "${PHASE10_RED:-0}" == "1" ]]; then
  # AC-7a: templates/CONTRACT.template.md must be present in the tarball.
  if echo "$LISTING" | grep -qF "templates/CONTRACT.template.md"; then
    pass "tarball contains templates/CONTRACT.template.md"
  else
    fail "tarball missing templates/CONTRACT.template.md (Phase 10: build-payload.sh must ship templates/)"
  fi

  # AC-7b: docs/DEVELOPMENT.md must NOT be in the tarball (it is framework-dev only).
  if echo "$LISTING" | grep -qE 'docs/DEVELOPMENT\.md'; then
    fail "tarball must not contain docs/DEVELOPMENT.md (framework-dev only, not for governed projects)"
  else
    pass "tarball does not contain docs/DEVELOPMENT.md"
  fi

  # AC-7c: CONTRACT.md must NOT be in the tarball (render-at-inject-time, Phase 9 scope).
  if echo "$LISTING" | grep -qE '^\.?/?CONTRACT\.md$'; then
    fail "tarball must not contain CONTRACT.md (reserved slot — render at inject time, Phase 9)"
  else
    pass "tarball does not contain CONTRACT.md"
  fi

  # AC-6: No docs/<kind>/ artifact paths in skills/ or instincts/ under the payload.
  # These are repo-only paths; governed projects use .claude/topology/<kind>/.
  TMPDIR_SKILL_CHECK="$(mktemp -d)"
  tar -xzf "$TARBALL" -C "$TMPDIR_SKILL_CHECK" 2>/dev/null || true
  DOCS_ARTIFACT_HITS="$(grep -rE 'docs/(specs|plans|research|verify|reviews|memory|learn)/' \
    "$TMPDIR_SKILL_CHECK/skills/" "$TMPDIR_SKILL_CHECK/instincts/" 2>/dev/null || true)"
  rm -rf "$TMPDIR_SKILL_CHECK"
  if [[ -n "$DOCS_ARTIFACT_HITS" ]]; then
    fail "skills/ or instincts/ contain docs/<kind>/ artifact paths (Phase 10 skill sweep incomplete):"$'\n'"$DOCS_ARTIFACT_HITS"
  else
    pass "no docs/<kind>/ artifact paths in payload skills/ or instincts/"
  fi
fi

# ── Summary ────────────────────────────────────────────────────────────────────
echo ""
echo "test-build-payload: $PASS passed, $FAIL failed"
if [[ $FAIL -gt 0 ]]; then
  exit 1
fi
