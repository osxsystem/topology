#!/usr/bin/env bash
# burn-in-replay-tdd.sh — TDD red-green replay burn-in (ADR-0017): replay the engine over
# the repo's own historical merge pairs (M^1 = base, M^2 = feature tip), accruing tdd/replay
# verdicts into a dedicated gitignored log, then summarise via shadow-stats.sh. Pure
# measurement: history mode logs the would-be verdict and never blocks. Flips nothing.
#
# Usage: burn-in-replay-tdd.sh [LIMIT]   (LIMIT = most-recent N merge commits, default 15)
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(git rev-parse --show-toplevel)"
GATEKEEPER="${GATEKEEPER_BIN:-$REPO_ROOT/gatekeeper/target/release/gatekeeper}"
LOG="$REPO_ROOT/docs/logs/burn-in-tdd.jsonl"
TEST_CMD="${BURN_IN_TEST_CMD:-cargo test --manifest-path gatekeeper/Cargo.toml --quiet}"
LIMIT="${1:-15}"

mkdir -p "$(dirname "$LOG")"
: >"$LOG" # truncate per run → idempotent

# Scratch worktree + cleanup on every exit/interrupt (mirrors the engine's RAII guard).
WT="$(mktemp -d "${TMPDIR:-/tmp}/burnin-wt.XXXXXX")"
cleanup() {
  git -C "$REPO_ROOT" worktree remove --force "$WT" 2>/dev/null || rm -rf "$WT"
  git -C "$REPO_ROOT" worktree prune 2>/dev/null || true
}
trap cleanup EXIT INT TERM

mapfile -t merges < <(git -C "$REPO_ROOT" log --merges --format='%H' -n "$LIMIT")
if ((${#merges[@]} == 0)); then
  echo "no merge commits in window (0 evaluations)"
  exit 0
fi

evals=0
for m in "${merges[@]}"; do
  p1=$(git -C "$REPO_ROOT" rev-parse "$m^1" 2>/dev/null || true)
  p2=$(git -C "$REPO_ROOT" rev-parse "$m^2" 2>/dev/null || true)
  { [ -n "$p1" ] && [ -n "$p2" ]; } || continue
  short=$(git -C "$REPO_ROOT" rev-parse --short "$m")

  git -C "$REPO_ROOT" worktree remove --force "$WT" 2>/dev/null || true
  rm -rf "$WT"
  git -C "$REPO_ROOT" worktree add --quiet --detach "$WT" "$p2"

  # Ephemeral, untracked config so replay fires (history mode → log, never block).
  # The default allowed_command_prefixes already covers "cargo test" (config.rs:180).
  mkdir -p "$WT/docs"
  printf 'test_command = "%s"\n' "$TEST_CMD" >"$WT/docs/config.toml"

  out="$(cd "$WT" && "$GATEKEEPER" check tdd --feature "$short" --base "$p1" 2>&1)" || true
  # The engine's stderr SHADOW line has no `ts`; the file sink does. Inject one so
  # shadow-stats.sh's would-block triage section (keyed on `ts`) renders.
  matched="$(printf '%s\n' "$out" | grep -E '^SHADOW .*"check":"replay"' || true)"
  if [ -n "$matched" ]; then
    ts=$(date +%s)
    printf '%s\n' "$matched" | sed "s/^SHADOW {/SHADOW {\"ts\":$ts,/" >>"$LOG"
    evals=$((evals + 1))
  fi
done

echo "replayed ${#merges[@]} merge(s); $evals produced a replay verdict -> $LOG"
echo
"$SCRIPT_DIR/shadow-stats.sh" "$LOG"
