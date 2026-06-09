#!/usr/bin/env bash
# Topology PreToolUse hook — a security veto before Bash/Write/Edit/MultiEdit.
# Pipes the event JSON (stdin) to `gatekeeper scan --hook`, which emits the Claude permission
# decision on stdout (deny/ask) or stays silent (allow). Fail-closed: a missing/erroring binary
# emits a deny. No jq; the binary owns all JSON parsing.
set -euo pipefail

HOOK_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="${CLAUDE_PLUGIN_ROOT:-$(dirname "$HOOK_DIR")}"

deny() {
  printf '{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"%s"}}\n' "$1"
  exit 0
}

# $GATEKEEPER_BIN (an explicit override) wins when set and executable; otherwise prefer the
# repo-built binary (the trusted, feature-current scanner) over whatever `gatekeeper` happens to
# be on PATH — a stale or unrelated PATH binary must not stand in for the real veto.
if [[ -n "${GATEKEEPER_BIN:-}" && -x "${GATEKEEPER_BIN:-}" ]]; then
  GK="$GATEKEEPER_BIN"
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
