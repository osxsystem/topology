#!/usr/bin/env bash
# Topology PostToolUse hook (advisory, path-triggered routing).
# Reads the PostToolUse event JSON on stdin, asks the gatekeeper which skills the
# touched file path routes in, and prints a reminder block. It NEVER blocks the
# tool call: it always exits 0 (advisory only — blocking is the scan/gate layer's job).
#
# Wire it up (Claude Code settings.json):
#   "hooks": { "PostToolUse": [ { "matcher": "Write|Edit|MultiEdit",
#              "hooks": [ { "type": "command",
#                           "command": "/abs/path/to/topology/hooks/post-tool-routing.sh" } ] } ] }
#
# Falls back gracefully if the gatekeeper binary isn't built yet.
#
# Binary resolution order:
#   1. $GATEKEEPER_BIN          (explicit override, wins when set and executable)
#   2. $ROOT/bin/gatekeeper     (installer-placed prebuilt)
#   3. $CLAUDE_PLUGIN_DATA/bin/gatekeeper  (plugin-provisioned prebuilt)
#   4. gatekeeper on PATH
#   5. $ROOT/gatekeeper/target/release/gatekeeper  (repo release build)
#   6. $ROOT/gatekeeper/target/debug/gatekeeper    (repo debug build)
#   7. silent exit 0 (fail-open)
set -uo pipefail

HOOK_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="${CLAUDE_PLUGIN_ROOT:-$(dirname "$HOOK_DIR")}"

# Resolution order: explicit override → installer bin/ → plugin data bin/ →
# PATH → repo release build → repo debug build → silent exit 0 (fail-open).
if [[ -n "${GATEKEEPER_BIN:-}" && -x "${GATEKEEPER_BIN:-}" ]]; then
  GK="$GATEKEEPER_BIN"
elif [[ -x "$ROOT/bin/gatekeeper" ]]; then
  GK="$ROOT/bin/gatekeeper"
elif [[ -n "${CLAUDE_PLUGIN_DATA:-}" && -x "$CLAUDE_PLUGIN_DATA/bin/gatekeeper" ]]; then
  GK="$CLAUDE_PLUGIN_DATA/bin/gatekeeper"
elif command -v gatekeeper >/dev/null 2>&1; then
  GK="$(command -v gatekeeper)"
elif [[ -x "$ROOT/gatekeeper/target/release/gatekeeper" ]]; then
  GK="$ROOT/gatekeeper/target/release/gatekeeper"
elif [[ -x "$ROOT/gatekeeper/target/debug/gatekeeper" ]]; then
  GK="$ROOT/gatekeeper/target/debug/gatekeeper"
else
  # Binary not built — advisory hook stays silent and never blocks.
  exit 0
fi

# Pass the PostToolUse event through to the path router. Never fail the tool call.
EVENT="$(cat || true)"
printf '%s' "$EVENT" | (cd "$ROOT" && "$GK" route --hook) || true
exit 0
