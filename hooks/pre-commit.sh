#!/usr/bin/env bash
# Topology pre-commit hook — block a commit that stages a secret, an unscannable blob, or a change
# to a protected safety file. Fail-closed. A human who must commit a legitimate protected change
# types, at their own terminal: git commit --no-verify
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"

if command -v gatekeeper >/dev/null 2>&1; then
  GK="$(command -v gatekeeper)"
elif [[ -x "$ROOT/gatekeeper/target/release/gatekeeper" ]]; then
  GK="$ROOT/gatekeeper/target/release/gatekeeper"
elif [[ -x "$ROOT/gatekeeper/target/debug/gatekeeper" ]]; then
  GK="$ROOT/gatekeeper/target/debug/gatekeeper"
else
  echo "Topology pre-commit: security scanner unavailable - run ./scripts/install.sh" >&2
  exit 1
fi

if (cd "$ROOT" && "$GK" scan --staged); then
  exit 0
else
  echo "Topology pre-commit: BLOCKED (see the BLOCK lines above)." >&2
  echo "A human may override a legitimate change at their terminal: git commit --no-verify" >&2
  exit 1
fi
