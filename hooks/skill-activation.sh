#!/usr/bin/env bash
# Topology UserPromptSubmit hook.
# Reads the user's prompt on stdin, asks the gatekeeper which skills to route,
# and prints an activation block that gets injected ahead of the agent's turn.
#
# Wire it up (Claude Code settings.json):
#   "hooks": { "UserPromptSubmit": "/abs/path/to/topology/hooks/skill-activation.sh" }
#
# Falls back gracefully if the gatekeeper binary isn't built yet.
set -euo pipefail

HOOK_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(dirname "$HOOK_DIR")"

# Prefer an installed binary, else the release build, else the debug build.
if command -v gatekeeper >/dev/null 2>&1; then
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
