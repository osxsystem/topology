#!/usr/bin/env bash
# Topology PreToolUse hook — a security veto before Bash/Write/Edit/MultiEdit.
# Pipes the event JSON (stdin) to `gatekeeper scan --hook`, which emits the Claude permission
# decision on stdout (deny/ask) or stays silent (allow). Fail-closed: a missing/erroring binary
# emits a deny. No jq; the binary owns all JSON parsing.
#
# Binary resolution order:
#   1. $GATEKEEPER_BIN          (explicit override, wins when set and executable)
#   2. $ROOT/bin/gatekeeper     (installer-placed prebuilt)
#   3. $CLAUDE_PLUGIN_DATA/bin/gatekeeper  (plugin-provisioned prebuilt)
#   4. $ROOT/gatekeeper/target/release/gatekeeper  (repo release build)
#   5. $ROOT/gatekeeper/target/debug/gatekeeper    (repo debug build)
#   6. gatekeeper on PATH
#   7. deny (fail-closed)
set -euo pipefail

HOOK_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="${CLAUDE_PLUGIN_ROOT:-$(dirname "$HOOK_DIR")}"

deny() {
  printf '{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"%s"}}\n' "$1"
  exit 0
}

# Resolution order: explicit override → installer bin/ → plugin data bin/ →
# repo release build → repo debug build → PATH → deny (fail-closed).
if [[ -n "${GATEKEEPER_BIN:-}" && -x "${GATEKEEPER_BIN:-}" ]]; then
  GK="$GATEKEEPER_BIN"
elif [[ -x "$ROOT/bin/gatekeeper" ]]; then
  GK="$ROOT/bin/gatekeeper"
elif [[ -n "${CLAUDE_PLUGIN_DATA:-}" && -x "$CLAUDE_PLUGIN_DATA/bin/gatekeeper" ]]; then
  GK="$CLAUDE_PLUGIN_DATA/bin/gatekeeper"
elif [[ -x "$ROOT/gatekeeper/target/release/gatekeeper" ]]; then
  GK="$ROOT/gatekeeper/target/release/gatekeeper"
elif [[ -x "$ROOT/gatekeeper/target/debug/gatekeeper" ]]; then
  GK="$ROOT/gatekeeper/target/debug/gatekeeper"
elif command -v gatekeeper >/dev/null 2>&1; then
  GK="$(command -v gatekeeper)"
else
  deny "Topology: security scanner unavailable - run ./scripts/install.sh"
fi

if out="$(cd "$ROOT" && "$GK" scan --hook 2>/dev/null)"; then
  [[ -n "$out" ]] && printf '%s\n' "$out"
  exit 0
else
  deny "Topology: security scanner error - failing closed"
fi
