#!/usr/bin/env bash
# Topology Stop hook — persist a session's gotcha into the learning ledger (Phase 3, ADR-0005).
#
# This is the OPTIONAL automated capture path; the always-available path is the `capture-gotcha` skill,
# where the agent recognizes a recurring failure and runs `gatekeeper learn capture` directly.
#
# Wire it up (Claude Code settings.json) — .claude/settings.json is a protected path, so a HUMAN adds:
#   "hooks": { "Stop": "/abs/path/to/topology/hooks/learn-capture.sh" }
#
# Contract: capture ONLY when $TOPOLOGY_GOTCHA holds a one-line lesson, so the hook never spams the ledger
# on an ordinary Stop. Set it during the turn when you hit something worth remembering:
#   export TOPOLOGY_GOTCHA="verify passed on green unit tests but missed an integration break"
# To record a gate failure by hand instead (no hook needed):
#   gatekeeper learn capture --trigger gate-failure --gate verify --date "$(date +%F)" \
#     --summary "what went wrong and the lesson"
#
# CWD CONTRACT: the binary MUST run from the SESSION cwd (the project directory), not from the
# payload root. The binary derives artifacts_root() from its process cwd; running from the payload
# root would anchor the ledger under the payload's docs/ instead of the project's
# .claude/topology/learn/ledger.md — the data-loss scenario ADR-0013 exists to prevent.
# The framework/payload root travels via TOPOLOGY_ROOT env, never via cd.
set -euo pipefail

HOOK_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(dirname "$HOOK_DIR")"

# Drain the Stop event JSON on stdin (unused today; never let a closed pipe fail the Stop).
cat >/dev/null 2>&1 || true

# Nothing to record → do nothing. Never break the Stop, never spam the ledger.
SUMMARY="${TOPOLOGY_GOTCHA:-}"
if [[ -z "$SUMMARY" ]]; then
  exit 0
fi

# Prefer an installed binary, else the release build, else the debug build.
if command -v gatekeeper >/dev/null 2>&1; then
  GK="$(command -v gatekeeper)"
elif [[ -x "$ROOT/gatekeeper/target/release/gatekeeper" ]]; then
  GK="$ROOT/gatekeeper/target/release/gatekeeper"
elif [[ -x "$ROOT/gatekeeper/target/debug/gatekeeper" ]]; then
  GK="$ROOT/gatekeeper/target/debug/gatekeeper"
else
  echo "Topology: gatekeeper not built — run ./scripts/install.sh (gotcha not captured)." >&2
  exit 0
fi

# Never fail the Stop on a capture error.
TOPOLOGY_ROOT="${TOPOLOGY_ROOT:-$ROOT}" "$GK" learn capture --trigger stop --date "$(date +%F)" --summary "$SUMMARY" \
  || echo "Topology: learn capture failed (gotcha not recorded)." >&2
exit 0
