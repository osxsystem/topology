#!/usr/bin/env bash
# Test the three-step version resolution in fetch-gatekeeper.sh.
#
# Uses TOPOLOGY_RELEASE_BASE_URL=file:// pointing at a dummy directory; the
# script will fail on the actual download (no binary there), but we assert the
# version it *resolves* from the error/log output on stderr — the version
# appears in the "downloading <URL>" line which encodes it.
#
# Three cases tested (precedence order):
#   1. TOPOLOGY_VERSION env var wins over everything.
#   2. VERSION file at root wins over Cargo.toml when env var is absent.
#   3. gatekeeper/Cargo.toml is used when neither env var nor VERSION file present.
#
# Exit 0 on pass, 1 on any failure.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
FETCH_SCRIPT="$SCRIPT_DIR/fetch-gatekeeper.sh"

PASS=0
FAIL=0

pass() { echo "PASS $*"; PASS=$((PASS + 1)); }
fail() { echo "FAIL $*"; FAIL=$((FAIL + 1)); }

# Helper: run fetch-gatekeeper.sh with a fake file:// release base so it fails
# on the download (no actual files) but not before printing the resolved URL.
# Returns the stderr output.
run_fetch_capture_stderr() {
  local extra_env=("$@")
  local fake_release_dir
  fake_release_dir="$(mktemp -d)"
  local stderr_out
  # fetch-gatekeeper.sh prints "fetch-gatekeeper: downloading <URL>" to stderr
  # before any network attempt; capture that.
  stderr_out="$(env "${extra_env[@]}" TOPOLOGY_RELEASE_BASE_URL="file://$fake_release_dir" \
    bash "$FETCH_SCRIPT" "$fake_release_dir/dest" 2>&1 || true)"
  rm -rf "$fake_release_dir"
  echo "$stderr_out"
}

# ── Piece 1: TOPOLOGY_VERSION env wins ────────────────────────────────────────
TMPDIR_FAKE="$(mktemp -d)"
trap 'rm -rf "$TMPDIR_FAKE"' EXIT

# Build a fake root layout with a VERSION file that would give a different version.
mkdir -p "$TMPDIR_FAKE/scripts"
ln -s "$FETCH_SCRIPT" "$TMPDIR_FAKE/scripts/fetch-gatekeeper.sh"
printf 'version = "9.9.9"\nrules_schema = 1\n' > "$TMPDIR_FAKE/VERSION"

OUTPUT="$(TOPOLOGY_VERSION="1.2.3" TOPOLOGY_RELEASE_BASE_URL="file://$TMPDIR_FAKE" \
  bash "$FETCH_SCRIPT" "$TMPDIR_FAKE/dest" 2>&1 || true)"

if echo "$OUTPUT" | grep -q "v1.2.3/gatekeeper-"; then
  pass "TOPOLOGY_VERSION env overrides VERSION file (resolved v1.2.3)"
else
  fail "TOPOLOGY_VERSION env did not override (output: $OUTPUT)"
fi

# ── Piece 2: VERSION file wins over Cargo.toml ────────────────────────────────
# Build a fake root: scripts/ beside VERSION (no gatekeeper/Cargo.toml).
TMPDIR_VFILE="$(mktemp -d)"
trap 'rm -rf "$TMPDIR_FAKE" "$TMPDIR_VFILE"' EXIT

mkdir -p "$TMPDIR_VFILE/scripts"
cp "$FETCH_SCRIPT" "$TMPDIR_VFILE/scripts/fetch-gatekeeper.sh"
printf 'version = "7.7.7"\nrules_schema = 1\n' > "$TMPDIR_VFILE/VERSION"
# No gatekeeper/Cargo.toml in TMPDIR_VFILE.

OUTPUT="$(TOPOLOGY_RELEASE_BASE_URL="file://$TMPDIR_VFILE" \
  bash "$TMPDIR_VFILE/scripts/fetch-gatekeeper.sh" "$TMPDIR_VFILE/dest" 2>&1 || true)"

if echo "$OUTPUT" | grep -q "v7.7.7/gatekeeper-"; then
  pass "VERSION file used when env var absent (resolved v7.7.7)"
else
  fail "VERSION file not used (output: $OUTPUT)"
fi

# ── Piece 3: Cargo.toml fallback when no VERSION file ────────────────────────
# Build a fake root: scripts/ beside a gatekeeper/Cargo.toml, no VERSION.
TMPDIR_CARGO="$(mktemp -d)"
trap 'rm -rf "$TMPDIR_FAKE" "$TMPDIR_VFILE" "$TMPDIR_CARGO"' EXIT

mkdir -p "$TMPDIR_CARGO/scripts"
cp "$FETCH_SCRIPT" "$TMPDIR_CARGO/scripts/fetch-gatekeeper.sh"
mkdir -p "$TMPDIR_CARGO/gatekeeper"
printf '[package]\nname = "gatekeeper"\nversion = "5.5.5"\n' > "$TMPDIR_CARGO/gatekeeper/Cargo.toml"
# No VERSION file in TMPDIR_CARGO.

OUTPUT="$(TOPOLOGY_RELEASE_BASE_URL="file://$TMPDIR_CARGO" \
  bash "$TMPDIR_CARGO/scripts/fetch-gatekeeper.sh" "$TMPDIR_CARGO/dest" 2>&1 || true)"

if echo "$OUTPUT" | grep -q "v5.5.5/gatekeeper-"; then
  pass "Cargo.toml fallback used when VERSION absent (resolved v5.5.5)"
else
  fail "Cargo.toml fallback not used (output: $OUTPUT)"
fi

# ── Summary ────────────────────────────────────────────────────────────────────
echo ""
echo "test-fetch-version: $PASS passed, $FAIL failed"
if [[ $FAIL -gt 0 ]]; then
  exit 1
fi
