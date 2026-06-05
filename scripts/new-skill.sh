#!/usr/bin/env bash
# Scaffold a new Topology skill from the house template.
# Usage: ./scripts/new-skill.sh <skill-slug> "<one-line description with trigger>"
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

SLUG="${1:-}"
DESC="${2:-}"
if [[ -z "$SLUG" || -z "$DESC" ]]; then
  echo "usage: $0 <skill-slug> \"<verb phrase>. Use when <triggers>.\"" >&2
  exit 2
fi
if [[ ! "$SLUG" =~ ^[a-z0-9-]+$ ]]; then
  echo "error: slug must be lowercase letters, numbers, hyphens only." >&2
  exit 2
fi

DIR="$ROOT/skills/$SLUG"
if [[ -e "$DIR" ]]; then
  echo "error: $DIR already exists." >&2
  exit 1
fi

mkdir -p "$DIR/references"
cat > "$DIR/SKILL.md" <<EOF
---
name: $SLUG
description: $DESC
---

# ${SLUG//-/ }

## When to use
<trigger conditions, in the user's words>

## Process
1. <step>
2. <step>

## Gate check (if this is a process skill)
\`\`\`bash
# gatekeeper check <gate> --feature <slug>
\`\`\`

## Common rationalizations (rebutted)
| Excuse | Reality |
|--------|---------|
| "<excuse>" | "<rebuttal>" |
EOF

echo "created $DIR/SKILL.md"
echo "remember: add a routing entry for '$SLUG' in hooks/skill-rules.json"
