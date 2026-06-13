#!/usr/bin/env bash
# Topology pre-commit hook — block a commit that stages a secret, an unscannable blob, or a change
# to a protected safety file. Fail-closed. A human who must commit a legitimate protected change
# types, at their own terminal: git commit --no-verify
set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"

# Prefer the repo-built binary (the trusted, feature-current scanner) over whatever `gatekeeper`
# happens to be on PATH — a stale or unrelated PATH binary must not stand in for the real veto.
# In a governed project (no gatekeeper/ source here) the vendored .topology/bin or an
# installer-placed bin/ holds the trusted copy.
if [[ -x "$ROOT/gatekeeper/target/release/gatekeeper" ]]; then
  GK="$ROOT/gatekeeper/target/release/gatekeeper"
elif [[ -x "$ROOT/gatekeeper/target/debug/gatekeeper" ]]; then
  GK="$ROOT/gatekeeper/target/debug/gatekeeper"
elif [[ -x "$ROOT/.topology/bin/gatekeeper" ]]; then
  GK="$ROOT/.topology/bin/gatekeeper"
elif [[ -x "$ROOT/bin/gatekeeper" ]]; then
  GK="$ROOT/bin/gatekeeper"
elif command -v gatekeeper >/dev/null 2>&1; then
  GK="$(command -v gatekeeper)"
else
  echo "Topology pre-commit: security scanner unavailable - run ./scripts/install.sh" >&2
  exit 1
fi

# Do not pin TOPOLOGY_ROOT here. The binary resolves the framework root itself via its
# is_marked_root ladder: a self-governed clone resolves to the repo, and a governed project
# resolves to its marked .topology payload (reached binary-adjacent from .topology/bin). Pinning
# it on the bare existence of a .topology directory misfires when that directory is a deliberate
# non-marked stub, blocking every commit (issue #60).

if (cd "$ROOT" && "$GK" scan --staged); then
  exit 0
else
  echo "Topology pre-commit: BLOCKED (see the BLOCK lines above)." >&2
  echo "A human may override a legitimate change at their terminal: git commit --no-verify" >&2
  exit 1
fi
