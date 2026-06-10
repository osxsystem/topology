#!/usr/bin/env bash
# Download, verify, and install the prebuilt gatekeeper binary for the current platform.
#
# Usage: fetch-gatekeeper.sh <dest-dir>
#
# On success: <dest-dir>/gatekeeper is installed and the absolute path is printed on stdout.
# On failure: a diagnostic is printed on stderr and exit 1.
#
# Test seams:
#   TOPOLOGY_RELEASE_BASE_URL  override the URL prefix (supports file:// for offline tests)
#   TOPOLOGY_VERSION           override the pinned version read from plugin.json
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: fetch-gatekeeper.sh <dest-dir>" >&2
  exit 1
fi

DEST_DIR="$1"

# Locate repo root relative to this script.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Determine platform triple.
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS/$ARCH" in
  Darwin/arm64|Darwin/aarch64)
    TRIPLE="aarch64-apple-darwin"
    SHASUM_CMD="shasum -a 256"
    ;;
  Darwin/x86_64)
    TRIPLE="x86_64-apple-darwin"
    SHASUM_CMD="shasum -a 256"
    ;;
  Linux/x86_64)
    TRIPLE="x86_64-unknown-linux-gnu"
    SHASUM_CMD="sha256sum"
    ;;
  Linux/aarch64|Linux/arm64)
    TRIPLE="aarch64-unknown-linux-gnu"
    SHASUM_CMD="sha256sum"
    ;;
  *)
    echo "fetch-gatekeeper: unsupported platform $OS/$ARCH — build from source: cd gatekeeper && cargo build --release" >&2
    exit 1
    ;;
esac

# Resolve version: env override, else extract from plugin.json (line-anchored, no jq).
if [[ -n "${TOPOLOGY_VERSION:-}" ]]; then
  VERSION="$TOPOLOGY_VERSION"
else
  PLUGIN_JSON="$REPO_ROOT/.claude-plugin/plugin.json"
  if [[ ! -f "$PLUGIN_JSON" ]]; then
    echo "fetch-gatekeeper: cannot find $PLUGIN_JSON to read pinned version" >&2
    exit 1
  fi
  VERSION="$(grep -m1 '"version"' "$PLUGIN_JSON" | sed -E 's/.*"version"[[:space:]]*:[[:space:]]*"([^"]*)".*/\1/')"
  if [[ -z "$VERSION" ]]; then
    echo "fetch-gatekeeper: failed to parse version from $PLUGIN_JSON" >&2
    exit 1
  fi
fi

BASE_URL="${TOPOLOGY_RELEASE_BASE_URL:-https://github.com/osxsystem/topology/releases/download}"
ASSET_URL="$BASE_URL/v$VERSION/gatekeeper-$TRIPLE"
SUMS_URL="$BASE_URL/v$VERSION/SHA256SUMS"

ASSET_NAME="gatekeeper-$TRIPLE"

# Work in a temp dir; clean up on exit.
TMPDIR_WORK="$(mktemp -d)"
cleanup() { rm -rf "$TMPDIR_WORK"; }
trap cleanup EXIT

echo "fetch-gatekeeper: downloading $ASSET_URL" >&2
curl -fsSL --max-time 60 -o "$TMPDIR_WORK/$ASSET_NAME" "$ASSET_URL"

echo "fetch-gatekeeper: downloading $SUMS_URL" >&2
curl -fsSL --max-time 60 -o "$TMPDIR_WORK/SHA256SUMS" "$SUMS_URL"

# Filter the SUMS file to just the line for our asset, then verify.
SUMS_LINE="$(grep "$ASSET_NAME" "$TMPDIR_WORK/SHA256SUMS" || true)"
if [[ -z "$SUMS_LINE" ]]; then
  echo "fetch-gatekeeper: $ASSET_NAME not found in SHA256SUMS" >&2
  exit 1
fi
echo "$SUMS_LINE" > "$TMPDIR_WORK/SHA256SUMS.single"

# Verification chatter goes to stderr — the path printed at the end must be the only stdout line.
(cd "$TMPDIR_WORK" && $SHASUM_CMD -c SHA256SUMS.single >&2) || {
  echo "fetch-gatekeeper: checksum verification failed" >&2
  exit 1
}

# Smoke test.
chmod +x "$TMPDIR_WORK/$ASSET_NAME"
"$TMPDIR_WORK/$ASSET_NAME" --version >/dev/null || {
  echo "fetch-gatekeeper: smoke test (--version) failed" >&2
  exit 1
}

# Atomic install.
mkdir -p "$DEST_DIR"
mv "$TMPDIR_WORK/$ASSET_NAME" "$DEST_DIR/gatekeeper"

FINAL_PATH="$(cd "$DEST_DIR" && pwd)/gatekeeper"
echo "$FINAL_PATH"
