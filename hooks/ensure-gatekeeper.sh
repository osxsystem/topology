#!/usr/bin/env bash
# Topology SessionStart hook — silently ensures gatekeeper is available.
#
# Fast path: if any binary resolves on the standard probe chain, exit 0 with no output.
# Provision path: call fetch-gatekeeper.sh to download the prebuilt binary, then report.
# Fail-open: on any failure, print an advisory and exit 0 so the session still starts.
#
# Binary probe order:
#   1. $GATEKEEPER_BIN
#   2. $ROOT/bin/gatekeeper
#   3. $CLAUDE_PLUGIN_DATA/bin/gatekeeper
#   4. $ROOT/gatekeeper/target/release/gatekeeper
#   5. $ROOT/gatekeeper/target/debug/gatekeeper
#   6. gatekeeper on PATH
set -euo pipefail

HOOK_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="${CLAUDE_PLUGIN_ROOT:-$(dirname "$HOOK_DIR")}"

# Fast path — probe the full resolution chain.
if [[ -n "${GATEKEEPER_BIN:-}" && -x "${GATEKEEPER_BIN:-}" ]]; then
  exit 0
fi
if [[ -x "$ROOT/bin/gatekeeper" ]]; then
  exit 0
fi
if [[ -n "${CLAUDE_PLUGIN_DATA:-}" && -x "$CLAUDE_PLUGIN_DATA/bin/gatekeeper" ]]; then
  exit 0
fi
if [[ -x "$ROOT/gatekeeper/target/release/gatekeeper" ]]; then
  exit 0
fi
if [[ -x "$ROOT/gatekeeper/target/debug/gatekeeper" ]]; then
  exit 0
fi
if command -v gatekeeper >/dev/null 2>&1; then
  exit 0
fi

# Provision path — no binary found; try to fetch one.
FETCH_SCRIPT="$ROOT/scripts/fetch-gatekeeper.sh"
DEST_BIN_DIR="${CLAUDE_PLUGIN_DATA:-$ROOT}/bin"

if [[ ! -x "$FETCH_SCRIPT" ]]; then
  echo "Topology: fetch-gatekeeper.sh not found — run scripts/install.sh or cd gatekeeper && cargo build --release" >&2
  exit 0
fi

INSTALLED_PATH=""
if INSTALLED_PATH="$("$FETCH_SCRIPT" "$DEST_BIN_DIR" 2>/dev/null)"; then
  VERSION_LINE="$("$INSTALLED_PATH" --version 2>/dev/null || true)"
  echo "Topology: gatekeeper $VERSION_LINE provisioned at $INSTALLED_PATH"
else
  echo "Topology: could not provision gatekeeper automatically. To fix, run one of:" >&2
  echo "  bash scripts/install.sh" >&2
  echo "  cd gatekeeper && cargo build --release" >&2
fi

exit 0
