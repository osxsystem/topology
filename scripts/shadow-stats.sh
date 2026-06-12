#!/usr/bin/env bash
# shadow-stats.sh — Summarise the shadow-verdict burn-in log.
#
# Usage: shadow-stats.sh [path-to-shadow.jsonl]
#
#   Reads the JSONL file written by gatekeeper's shadow-verdict sink and prints:
#     1. A per-(gate,check) table with evaluation counts and would-block rate.
#     2. A "Would-block details" section listing each fail verdict for human triage.
#     3. The flip criterion reminder (>=50 evaluations, <2% false-block rate).
#
#   Default path: docs/logs/shadow.jsonl relative to the repo root.
#
# No jq dependency. Uses only grep, sed, awk, and git plumbing.
set -euo pipefail

# ---------------------------------------------------------------------------
# Resolve repo root so the default path works from any subdirectory.
# ---------------------------------------------------------------------------
REPO_ROOT="$(git rev-parse --show-toplevel)"
DEFAULT_LOG="$REPO_ROOT/docs/logs/shadow.jsonl"
LOG_PATH="${1:-$DEFAULT_LOG}"

# ---------------------------------------------------------------------------
# Missing or empty file → informational exit.
# ---------------------------------------------------------------------------
if [[ ! -f "$LOG_PATH" ]] || [[ ! -s "$LOG_PATH" ]]; then
  echo "no shadow log at $LOG_PATH (0 evaluations)"
  exit 0
fi

# ---------------------------------------------------------------------------
# Per-(gate,check) aggregation.
# Fields are written in a stable order by our own code:
#   {"ts":<n>,"gate":"...","check":"...","configured":"...","artifact":...,"command":...,"result":"...","detail":"..."}
# We extract gate, check, and result with sed into a flat tab-separated stream,
# then aggregate with awk (POSIX-compatible: no gawk extensions).
# ---------------------------------------------------------------------------

echo "Shadow-verdict burn-in summary"
echo "================================"
printf '\n'

# Step 1: flatten each JSONL line to "gate<TAB>check<TAB>result"
flatten() {
  sed -n 's/.*"gate":"\([^"]*\)".*"check":"\([^"]*\)".*"result":"\([^"]*\)".*/\1\t\2\t\3/p' "$LOG_PATH"
}

# Step 2: aggregate with POSIX awk; sort keys by hand (insertion order is enough
# for a summary — we accumulate a list and sort it via the shell if needed).
flatten | awk -F'\t' '
{
  gate   = $1
  check  = $2
  result = $3
  key    = gate SUBSEP check
  total[key]++
  if      (result == "pass")   pass_[key]++
  else if (result == "fail")   fail_[key]++
  else if (result == "skip")   skip_[key]++
  else if (result == "static") static_[key]++
  # Track insertion order
  if (!(key in seen)) {
    seen[key] = 1
    keys[++nkeys] = key
  }
}
END {
  printf "%-20s %-25s %5s %5s %5s %5s %8s %15s\n",
    "gate", "check", "evals", "pass", "fail", "skip", "static", "would-block%"
  printf "%-20s %-25s %5s %5s %5s %5s %8s %15s\n",
    "--------------------",
    "-------------------------",
    "-----", "-----", "-----", "-----", "--------", "---------------"
  for (i = 1; i <= nkeys; i++) {
    k  = keys[i]
    t  = total[k]   + 0
    p  = pass_[k]   + 0
    f  = fail_[k]   + 0
    s  = skip_[k]   + 0
    st = static_[k] + 0
    rate = (t > 0) ? (f / t * 100) : 0
    split(k, parts, SUBSEP)
    printf "%-20s %-25s %5d %5d %5d %5d %8d %14.1f%%\n",
      parts[1], parts[2], t, p, f, s, st, rate
  }
}
'

printf '\n'

# ---------------------------------------------------------------------------
# Would-block details section — list every "fail" verdict for human triage.
# Fields extracted: ts, gate, check, detail (via sed).
# ---------------------------------------------------------------------------

fail_count=$(grep -c '"result":"fail"' "$LOG_PATH" 2>/dev/null || true)

echo "Would-block details ($fail_count verdict(s) to triage)"
echo "--------------------------------------------------------"

if [[ "$fail_count" -eq 0 ]]; then
  echo "  (none)"
else
  grep '"result":"fail"' "$LOG_PATH" | sed -n '
    # Extract ts, gate, check, detail from each line.
    # detail is extracted with [^"]* because our json_str escaper never emits
    # a bare unescaped " inside a value, so stopping at the next " is safe.
    s/.*"ts":\([0-9]*\).*"gate":"\([^"]*\)".*"check":"\([^"]*\)".*"detail":"\([^"]*\)".*/\1\t\2\t\3\t\4/p
  ' | awk -F'\t' '{
    printf "  ts=%-12s  gate=%-10s  check=%-25s  detail=%s\n", $1, $2, $3, $4
  }'
fi

printf '\n'

# ---------------------------------------------------------------------------
# Flip criterion reminder.
# ---------------------------------------------------------------------------
echo "Flip criterion (per gate): >=50 evaluations AND human-triaged false-block rate <2%"
echo "  — triage each would-block above and record conclusions in a committed research note."
