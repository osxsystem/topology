#!/usr/bin/env bash
# Build the gatekeeper binary, create the CLAUDE.md -> AGENTS.md symlink,
# make hooks/scripts executable, and print the hook config to paste into your client.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

echo "==> Building gatekeeper (release)"
if ! command -v cargo >/dev/null 2>&1; then
  echo "error: cargo (Rust) is not installed. Install from https://rustup.rs and re-run." >&2
  exit 1
fi
( cd gatekeeper && cargo build --release )
BIN="$ROOT/gatekeeper/target/release/gatekeeper"
echo "    built: $BIN"

echo "==> Linking CLAUDE.md -> AGENTS.md"
ln -sf AGENTS.md CLAUDE.md
echo "    $ROOT/CLAUDE.md -> AGENTS.md"

echo "==> Marking scripts executable"
chmod +x hooks/*.sh scripts/*.sh
echo "    done"

echo "==> Optional: put gatekeeper on PATH"
echo "    sudo ln -sf \"$BIN\" /usr/local/bin/gatekeeper"

cat <<EOF

==> Hook config (Claude Code: ~/.claude/settings.json or .claude/settings.json)
{
  "hooks": {
    "UserPromptSubmit": "$ROOT/hooks/skill-activation.sh"
  }
}

Verify:
  gatekeeper list
  echo "add a users table" | "$BIN" activate
EOF
