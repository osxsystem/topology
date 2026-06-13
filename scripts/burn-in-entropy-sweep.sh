#!/usr/bin/env bash
# burn-in-entropy-sweep.sh — entropy-scanner burn-in (ADR-0018): count entropy WARN
# hits per 10k scanned lines across the working tree. Pure measurement: never blocks,
# never edits. Glue over `gatekeeper scan --content`. `exclude_paths` is applied here
# because `--content` carries no path (scan.rs), so the figure matches the path-bearing
# --staged/--hook lanes. Flips nothing.
set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
GATEKEEPER="${GATEKEEPER_BIN:-$REPO_ROOT/gatekeeper/target/release/gatekeeper}"
CAP_BYTES=$((5 * 1024 * 1024)) # scan --content HOOK_INPUT_CAP (scan.rs:25)

# Globs the entropy lane skips on path-bearing lanes (CHANGELOG v0.6.0 / spec).
is_excluded() {
  case "$1" in
  *.lock | *.svg | *.min.js) return 0 ;;
  tests/fixtures/* | */tests/fixtures/*) return 0 ;;
  esac
  return 1
}

warn_hits=0
total_lines=0
scanned=0
skipped_excluded=0
skipped_oversize=0

while IFS= read -r f; do
  if is_excluded "$f"; then
    skipped_excluded=$((skipped_excluded + 1))
    continue
  fi
  [ -f "$f" ] || continue
  bytes=$(wc -c <"$f")
  if ((bytes > CAP_BYTES)); then
    echo "skip (oversize >5MiB): $f" >&2
    skipped_oversize=$((skipped_oversize + 1))
    continue
  fi
  lines=$(wc -l <"$f")
  total_lines=$((total_lines + lines))
  scanned=$((scanned + 1))
  out="$("$GATEKEEPER" scan --content <"$f" 2>&1)" || true
  hits=$(printf '%s\n' "$out" | grep -cE '^WARN (hex|base64)-high-entropy:' || true)
  warn_hits=$((warn_hits + hits))
done < <(git -C "$REPO_ROOT" ls-files)

if ((scanned == 0)); then
  echo "no source files scanned (0 evaluations)"
  exit 0
fi

rate=$(awk -v h="$warn_hits" -v l="$total_lines" \
  'BEGIN { if (l == 0) print "0.00"; else printf "%.2f", h / l * 10000 }')
echo "Entropy burn-in sweep"
echo "  files scanned:        $scanned"
echo "  excluded (path glob): $skipped_excluded"
echo "  skipped (oversize):   $skipped_oversize"
echo "  total lines:          $total_lines"
echo "  entropy WARN hits:    $warn_hits"
echo "  WARN per 10k lines:   $rate"
echo "  criterion (ADR-0018): FP <1 per 10k lines (current-tree proxy for full-history)"
