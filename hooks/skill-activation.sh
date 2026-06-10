#!/usr/bin/env bash
# Topology UserPromptSubmit hook.
# Reads the user's prompt on stdin, asks the gatekeeper which skills to route,
# and prints an activation block that gets injected ahead of the agent's turn.
#
# Wire it up (Claude Code settings.json):
#   "hooks": { "UserPromptSubmit": "/abs/path/to/topology/hooks/skill-activation.sh" }
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
#   7. advisory message + exit 0 (fail-open)
set -euo pipefail

HOOK_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="${CLAUDE_PLUGIN_ROOT:-$(dirname "$HOOK_DIR")}"

# Resolution order: explicit override → installer bin/ → plugin data bin/ →
# PATH → repo release build → repo debug build → advisory + exit 0 (fail-open).
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
  echo "Topology: gatekeeper not built — run ./scripts/install.sh. Still: evaluate your skills before acting."
  exit 0
fi

# Pass the prompt through to the router. Never fail the user's turn on hook error.
PROMPT="$(cat || true)"
printf '%s' "$PROMPT" | (cd "$ROOT" && "$GK" activate) || \
  echo "Topology: evaluate your skills before acting (router unavailable)."
