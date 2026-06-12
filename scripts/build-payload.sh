#!/usr/bin/env bash
# Build the topology distribution payload tarball.
#
# Usage: build-payload.sh <stage-dir> [version]
#
#   stage-dir   Directory to stage files into (created if absent; must be empty or non-existent).
#   version     Payload version string (default: extracted from gatekeeper/Cargo.toml).
#
# Outputs: topology-payload.tar.gz in the current working directory; its absolute path is
# printed on stdout.
#
# Test seams:
#   TOPOLOGY_VERSION   override the version (bypasses Cargo.toml read)
set -euo pipefail

if [[ $# -lt 1 || $# -gt 2 ]]; then
  echo "usage: build-payload.sh <stage-dir> [version]" >&2
  exit 1
fi

STAGE_DIR="$1"

# Locate repo root relative to this script.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# ── Guard: stage dir must be absent or empty ───────────────────────────────────
# Stale content in an existing stage dir leaks into the tarball; fail fast rather
# than silently shipping unexpected files. The caller is responsible for passing a
# fresh path (or cleaning up before re-invoking).
if [[ -d "$STAGE_DIR" ]]; then
  if [[ -n "$(ls -A "$STAGE_DIR" 2>/dev/null)" ]]; then
    echo "build-payload: stage dir '$STAGE_DIR' already exists and is non-empty; remove it before running" >&2
    exit 1
  fi
fi

# ── Resolve version ────────────────────────────────────────────────────────────
# An explicit positional argument beats the TOPOLOGY_VERSION env var.
# TOPOLOGY_VERSION is the *fallback* for automation (e.g. CI sets it when no
# explicit arg is provided); it must NOT silently shadow an argument that was
# intentionally passed.
if [[ $# -eq 2 && -n "$2" ]]; then
  VERSION="$2"
elif [[ -n "${TOPOLOGY_VERSION:-}" ]]; then
  VERSION="$TOPOLOGY_VERSION"
else
  CARGO_TOML="$REPO_ROOT/gatekeeper/Cargo.toml"
  if [[ ! -f "$CARGO_TOML" ]]; then
    echo "build-payload: cannot find $CARGO_TOML to read version" >&2
    exit 1
  fi
  VERSION="$(grep -m1 '^version' "$CARGO_TOML" | sed -E 's/^version[[:space:]]*=[[:space:]]*"([^"]*)".*/\1/')"
  if [[ -z "$VERSION" ]]; then
    echo "build-payload: failed to parse version from $CARGO_TOML" >&2
    exit 1
  fi
fi

# ── Resolve rules_schema ───────────────────────────────────────────────────────
SCAN_RS="$REPO_ROOT/gatekeeper/src/scan.rs"
if [[ ! -f "$SCAN_RS" ]]; then
  echo "build-payload: cannot find $SCAN_RS to read rules_schema constant" >&2
  exit 1
fi
RULES_SCHEMA="$(grep -m1 'const SCHEMA_VERSION' "$SCAN_RS" | sed -E 's/.*=[[:space:]]*([0-9]+).*/\1/')"
if [[ -z "$RULES_SCHEMA" ]]; then
  echo "build-payload: failed to parse SCHEMA_VERSION from $SCAN_RS" >&2
  exit 1
fi

# ── Stage files ────────────────────────────────────────────────────────────────
mkdir -p "$STAGE_DIR"

# hooks/ — only the four payload hooks + skill-rules.json (NOT ensure-gatekeeper.sh, hooks.json)
mkdir -p "$STAGE_DIR/hooks"
for f in skill-activation.sh security-scan.sh pre-commit.sh learn-capture.sh; do
  SRC="$REPO_ROOT/hooks/$f"
  if [[ ! -f "$SRC" ]]; then
    echo "build-payload: missing required hook: hooks/$f" >&2
    exit 1
  fi
  cp "$SRC" "$STAGE_DIR/hooks/$f"
done
cp "$REPO_ROOT/hooks/skill-rules.json" "$STAGE_DIR/hooks/skill-rules.json"

# skills/ — entire directory
mkdir -p "$STAGE_DIR/skills"
cp -R "$REPO_ROOT/skills/." "$STAGE_DIR/skills/"

# instincts/ — entire directory
mkdir -p "$STAGE_DIR/instincts"
cp -R "$REPO_ROOT/instincts/." "$STAGE_DIR/instincts/"

# security/rules.toml
mkdir -p "$STAGE_DIR/security"
cp "$REPO_ROOT/security/rules.toml" "$STAGE_DIR/security/rules.toml"

# scripts/fetch-gatekeeper.sh
mkdir -p "$STAGE_DIR/scripts"
cp "$REPO_ROOT/scripts/fetch-gatekeeper.sh" "$STAGE_DIR/scripts/fetch-gatekeeper.sh"

# templates/ — contract template (Phase 10); CONTRACT.md slot reserved for Phase 9 inject.
mkdir -p "$STAGE_DIR/templates"
SRC_TEMPLATE="$REPO_ROOT/templates/CONTRACT.template.md"
if [[ ! -f "$SRC_TEMPLATE" ]]; then
  echo "build-payload: missing required file: templates/CONTRACT.template.md" >&2
  exit 1
fi
cp "$SRC_TEMPLATE" "$STAGE_DIR/templates/CONTRACT.template.md"

# AGENTS.md — root-marker file required by is_marked_root() in main.rs.
# The unpacked payload tree contains skills/ (shipped above); is_marked_root also
# requires at least one of ROOT_MARKERS = ["AGENTS.md", "gatekeeper"].
# AGENTS.md is the right choice: it is a real, human-readable install-doc that makes
# sense at the payload root, unlike the "gatekeeper" binary (not yet present at unpack
# time).  Without this marker the binary walks up to $HOME or further when TOPOLOGY_ROOT
# is unset.
SRC_AGENTS="$REPO_ROOT/AGENTS.md"
if [[ ! -f "$SRC_AGENTS" ]]; then
  echo "build-payload: missing required marker file: AGENTS.md" >&2
  exit 1
fi
cp "$SRC_AGENTS" "$STAGE_DIR/AGENTS.md"

# ── Write VERSION ──────────────────────────────────────────────────────────────
printf 'version = "%s"\nrules_schema = %s\n' "$VERSION" "$RULES_SCHEMA" > "$STAGE_DIR/VERSION"

# ── Emit tarball ───────────────────────────────────────────────────────────────
TARBALL="$(pwd)/topology-payload.tar.gz"
tar -czf "$TARBALL" -C "$STAGE_DIR" .

echo "$TARBALL"
