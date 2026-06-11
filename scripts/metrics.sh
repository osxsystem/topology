#!/usr/bin/env sh
# metrics.sh — FM1 process-weight baseline CSV.
#
# Enumerates first-parent history on main since tag v0.3.0 and prints a CSV to
# stdout with one row per merge branch plus one residual row for direct-to-main
# commits. No jq, no python; POSIX sh + awk + git plumbing only.
#
# Header: branch,merge_commit,production_loc,artifact_loc,commits,lead_time_hours
#
# Usage: sh scripts/metrics.sh
set -eu

SINCE_TAG="v0.3.0"
BRANCH="main"

# Subshell-written scratch file (the rev-list pipe below cannot export vars);
# $$ is the parent shell's PID in both contexts, so the path agrees.
DIRECT_FILE="/tmp/metrics_direct_$$"
trap 'rm -f "$DIRECT_FILE"' EXIT

# ---------------------------------------------------------------------------
# Parse branch name from a merge commit subject.
# Handles:
#   "Merge pull request #N from owner/branch-name"  → branch-name
#   "Merge branch 'branch-name'"                    → branch-name
# Falls back to the raw subject on no match.
# ---------------------------------------------------------------------------
parse_branch() {
  subj="$1"
  # "Merge pull request #N from owner/branch"
  case "$subj" in
    Merge\ pull\ request\ *\ from\ */*)
      printf '%s' "$subj" | sed 's|.*from [^/]*/||'
      return ;;
    # "Merge pull request #N from branch" (no owner/)
    Merge\ pull\ request\ *\ from\ *)
      printf '%s' "$subj" | sed "s|.*from ||"
      return ;;
  esac
  # "Merge branch 'branch-name'"
  case "$subj" in
    Merge\ branch\ \'*\')
      printf '%s' "$subj" | sed "s/Merge branch '//;s/'$//"
      return ;;
    Merge\ branch\ \"*\")
      printf '%s' "$subj" | sed 's/Merge branch "//;s/"$//'
      return ;;
  esac
  # Fallback: raw subject
  printf '%s' "$subj"
}

# ---------------------------------------------------------------------------
# Compute production_loc and artifact_loc from git diff --numstat output.
# production: all files EXCEPT docs/** and *.md
# artifact:   docs/** and *.md files
# Binary files are reported as "-\t-\tpath" — counted as 0.
# ---------------------------------------------------------------------------
loc_from_numstat() {
  git diff --numstat "$1^1...$1^2" 2>/dev/null | awk '
  BEGIN { prod=0; art=0 }
  {
    added   = ($1 == "-") ? 0 : $1
    deleted = ($2 == "-") ? 0 : $2
    path    = $3
    lines   = added + deleted
    # artifact: under docs/ or any *.md
    if (path ~ /^docs\// || path ~ /\.md$/) {
      art += lines
    } else {
      prod += lines
    }
  }
  END { print prod "," art }
  '
}

# ---------------------------------------------------------------------------
# Lead time in hours (one decimal): earliest branch commit author date →
# merge commit author date.
# Returns empty string when there are no branch-only commits.
# ---------------------------------------------------------------------------
lead_time() {
  merge_hash="$1"
  merge_ts=$(git log -1 --format="%at" "$merge_hash")
  # Earliest (oldest) commit in branch range = last line of rev-list (newest first)
  earliest_ts=$(git rev-list "$merge_hash^1..$merge_hash^2" 2>/dev/null \
    | tail -1 \
    | xargs -I{} git log -1 --format="%at" {})
  if [ -z "$earliest_ts" ]; then
    printf ''
    return
  fi
  awk -v m="$merge_ts" -v e="$earliest_ts" \
    'BEGIN { diff = m - e; printf "%.1f", diff / 3600 }'
}

# ---------------------------------------------------------------------------
# Print header
# ---------------------------------------------------------------------------
printf 'branch,merge_commit,production_loc,artifact_loc,commits,lead_time_hours\n'

# ---------------------------------------------------------------------------
# Walk first-parent history; emit one row per merge commit.
# Collect direct-to-main commit hashes for the residual row.
# ---------------------------------------------------------------------------
direct_hashes=""

git rev-list --first-parent "$SINCE_TAG..$BRANCH" | while read -r hash; do
  num_parents=$(git cat-file commit "$hash" | grep -c "^parent ")
  if [ "$num_parents" -ge 2 ]; then
    # Merge commit — emit one row
    subj=$(git log -1 --format="%s" "$hash")
    branch=$(parse_branch "$subj")
    locs=$(loc_from_numstat "$hash")
    prod_loc=$(printf '%s' "$locs" | cut -d',' -f1)
    art_loc=$(printf '%s' "$locs" | cut -d',' -f2)
    commits=$(git rev-list --count "$hash^1..$hash^2" 2>/dev/null || echo 0)
    lt=$(lead_time "$hash")
    # Escape branch name: replace commas to avoid CSV breakage
    branch_safe=$(printf '%s' "$branch" | tr ',' '_')
    printf '%s,%s,%s,%s,%s,%s\n' \
      "$branch_safe" "$hash" "$prod_loc" "$art_loc" "$commits" "$lt"
  else
    # Direct-to-main — accumulate for the residual row.
    printf '%s\n' "$hash" >> "$DIRECT_FILE"
  fi
done

# ---------------------------------------------------------------------------
# Residual row: aggregate all direct-to-main commits.
# ---------------------------------------------------------------------------
if [ -f "$DIRECT_FILE" ]; then
  # Iteration order is irrelevant — only sums are computed; sort just gives a
  # stable traversal.
  sorted_hashes=$(sort "$DIRECT_FILE")
  rm -f "$DIRECT_FILE"

  total_prod=0
  total_art=0
  total_commits=0

  for hash in $sorted_hashes; do
    total_commits=$((total_commits + 1))
    # For a direct commit: diff against its own parent (single parent)
    locs=$(git diff --numstat "${hash}^..${hash}" 2>/dev/null | awk '
    BEGIN { prod=0; art=0 }
    {
      added   = ($1 == "-") ? 0 : $1
      deleted = ($2 == "-") ? 0 : $2
      path    = $3
      lines   = added + deleted
      if (path ~ /^docs\// || path ~ /\.md$/) {
        art += lines
      } else {
        prod += lines
      }
    }
    END { print prod "," art }
    ')
    p=$(printf '%s' "$locs" | cut -d',' -f1)
    a=$(printf '%s' "$locs" | cut -d',' -f2)
    total_prod=$((total_prod + p))
    total_art=$((total_art + a))
  done

  printf '(direct-to-main),,%s,%s,%s,\n' \
    "$total_prod" "$total_art" "$total_commits"
else
  # No direct-to-main commits found — emit empty residual row so row count
  # is always merge_count+1 and the reader can rely on it existing.
  printf '(direct-to-main),,%s,%s,%s,\n' 0 0 0
fi
