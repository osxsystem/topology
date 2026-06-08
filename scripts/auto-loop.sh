#!/usr/bin/env bash
#
# auto-loop.sh — Unattended Topology task loop.
#
#   For each plan task, in order:
#     1. Claude implements it headless via the tdd-loop methodology and commits.
#     2. `cargo test` must be GREEN (hard gate) before any review.
#     3. Codex reviews the task's diff and returns VERDICT: PASS | FAIL.
#     4. On FAIL, the findings are fed back to Claude to fix; re-review.
#        Repeat up to MAX_ATTEMPTS. If still not PASS, the run HALTS for you.
#     5. On PASS, advance to the next task.
#
#   You start it once and walk away. Stop it any time with Ctrl-C, or:
#       touch /tmp/topology-autoloop/STOP
#
# SAFETY MODEL (layered — no single fence is trusted alone):
#   * Refuses to run on main/master or with a dirty working tree.
#   * Claude runs headless with a deny-list overlay (auto-loop.settings.json):
#     no push / reset --hard / clean / filter-branch / rebase / rm -rf /
#     --no-verify / curl|wget / sudo. `deny` beats `allow` globally.
#   * The loop itself NEVER runs push or reset; it only add+commits scoped work.
#   * Per-call wall-clock timeout + per-call USD budget + global attempt cap.
#   * A stuck task (cap reached) HALTS the run, leaving state for inspection.
#
# STRONGLY RECOMMENDED: run this inside a dedicated git worktree so it cannot
# collide with an open interactive Claude session in your main checkout:
#       git worktree add -b auto/security-scanning ../topology-auto HEAD
#       cd ../topology-auto && ./scripts/auto-loop.sh
#   When it finishes, review the branch and merge:  git log auto/security-scanning
#
# USAGE:
#       ./scripts/auto-loop.sh              # run the default task list
#       ./scripts/auto-loop.sh 5            # run only task 5 (recommended first!)
#       ./scripts/auto-loop.sh 5 6 7        # run a custom subset, in order

set -euo pipefail

# ---------------------------- config (edit me) ----------------------------
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PLAN="docs/plans/2026-06-06-security-scanning.md"
SPEC="docs/specs/2026-06-06-security-scanning.md"
MANIFEST="gatekeeper/Cargo.toml"
DEFAULT_TASKS=(5 6 7 8 9 10 11 12 13 14 15 16 17)
MAX_ATTEMPTS=3                 # implement + (this many - 1) fix passes per task
CALL_TIMEOUT=2400              # hard wall-clock seconds per agent call (40 min)
CLAUDE_BUDGET_USD=8            # per claude invocation (only applies with --print)
CLAUDE_MODEL="opus"            # implementer model
SETTINGS="$REPO/scripts/auto-loop.settings.json"
LOGROOT="/tmp/topology-autoloop"
# --------------------------------------------------------------------------

cd "$REPO"

# Task list: CLI args override the default.
if [ "$#" -gt 0 ]; then TASKS=("$@"); else TASKS=("${DEFAULT_TASKS[@]}"); fi

RUN_ID="$(date +%Y%m%d-%H%M%S)"
LOGDIR="$LOGROOT/$RUN_ID"
STOPFILE="$LOGROOT/STOP"
mkdir -p "$LOGDIR"

# timeout shim: macOS ships neither `timeout` nor `gtimeout` by default.
TIMEOUT_BIN="$(command -v timeout || command -v gtimeout || true)"

log()  { printf '%s  %s\n' "$(date +%H:%M:%S)" "$*" | tee -a "$LOGDIR/run.log" ; }
die()  { log "FATAL: $*"; exit 1 ; }
need() { command -v "$1" >/dev/null 2>&1 || die "required tool not found: $1"; }

# Run a command with the optional timeout shim; never aborts the script.
# Echoes the command's exit code on stdout.
guarded() {
  local rc=0
  if [ -n "$TIMEOUT_BIN" ]; then
    "$TIMEOUT_BIN" "$CALL_TIMEOUT" "$@" || rc=$?
  else
    "$@" || rc=$?
  fi
  return "$rc"
}

# ------------------------------- preflight --------------------------------
log "Topology auto-loop  run=$RUN_ID  repo=$REPO"
log "Tasks: ${TASKS[*]}   max attempts/task: $MAX_ATTEMPTS"
log "Logs:  $LOGDIR"
[ -n "$TIMEOUT_BIN" ] || log "WARN: no timeout binary found; agent calls run uncapped (Ctrl-C still works). Install coreutils for gtimeout."

need claude; need codex; need cargo; need git; need uuidgen
[ -f "$SETTINGS" ] || die "missing deny-list overlay: $SETTINGS"
[ -f "$PLAN" ]     || die "missing plan: $PLAN"
[ -f "$SPEC" ]     || die "missing spec: $SPEC"

BRANCH="$(git rev-parse --abbrev-ref HEAD)"
case "$BRANCH" in
  main|master|HEAD) die "refusing to run on '$BRANCH' — switch to a feature branch (or a worktree).";;
esac
dirty_tracked="$(git status --porcelain | grep -v '^??' || true)"
[ -z "$dirty_tracked" ] || die "tracked files are modified or staged — commit or stash them first so the loop's commits stay unambiguous:
$dirty_tracked"
untracked="$(git status --porcelain | grep '^??' || true)"
if [ -n "$untracked" ]; then
  log "NOTE: untracked files present; left untouched (mass-add is denied by the overlay):"
  log "$untracked"
fi

log "Branch '$BRANCH' has no tracked changes. Starting."

# ---------------------------- the task loop -------------------------------
CLAUDE_COMMON=( --print --model "$CLAUDE_MODEL" --permission-mode acceptEdits
                --settings "$SETTINGS" --max-budget-usd "$CLAUDE_BUDGET_USD"
                --output-format text )

CODEX_COMMON=( exec --cd "$REPO" -s read-only -a never )

for N in "${TASKS[@]}"; do
  [ -f "$STOPFILE" ] && { log "STOP file present — halting before Task $N. Remove $STOPFILE to resume."; exit 0; }

  log "================ Task $N ================"
  SID="$(uuidgen)"
  BEFORE="$(git rev-parse HEAD)"
  feedback=""
  passed=0

  attempt=1
  while [ "$attempt" -le "$MAX_ATTEMPTS" ]; do
    [ -f "$STOPFILE" ] && { log "STOP file present — halting mid-Task $N."; exit 0; }
    log "Task $N — attempt $attempt/$MAX_ATTEMPTS"
    clog="$LOGDIR/task${N}-attempt${attempt}-claude.log"

    # ---- build the implement / fix prompt (no backticks: they break in shell) ----
    if [ -z "$feedback" ]; then
      prompt="Implement Task $N of the security-scanning plan, in this repo, using the tdd-loop methodology (skill: tdd-loop).

Read first:
- Plan: $PLAN  — find the section headed 'Task $N' and do EXACTLY that task.
- Spec & conventions: $SPEC  — obey its Conventions, especially: the regex crate has NO look-around or backreferences (express 'not followed by' with anchors + alternation); every read is bounded to cap+1 bytes (no full allocation before the size check); allow_blob is pinned by git blob OID, not sha256; block severity exits 1, warn writes to stderr and exits 0.

Do ONLY Task $N. Follow tdd-loop strictly: write the smallest test, watch it FAIL for the right reason (an assertion failure, not a compile error), write the minimum code to pass, refactor while green, then COMMIT (one commit per red-green cycle; commit message ends with the Co-Authored-By: Claude trailer). Use cargo test with --manifest-path $MANIFEST. Do not run cargo fmt or clippy unless Task $N is the final verify task. Do NOT start Task $((N+1)). Do NOT push or reset. When Task $N is fully implemented and committed, stop."
      guarded claude "${CLAUDE_COMMON[@]}" --session-id "$SID" "$prompt" >"$clog" 2>&1 || log "  (claude exit $?)"
    else
      prompt="Codex reviewed your Task $N work and returned VERDICT: FAIL. Address EVERY finding below, keeping tdd-loop discipline (for a behavior bug, add a failing test first), keep the suite green, and commit the fixes. Do NOT push or reset. Do NOT start the next task.

Reviewer findings:
$feedback"
      guarded claude "${CLAUDE_COMMON[@]}" --resume "$SID" "$prompt" >"$clog" 2>&1 || log "  (claude exit $?)"
    fi

    # ---- did Claude actually commit anything for this task? ----
    if [ "$(git rev-parse HEAD)" = "$BEFORE" ] && [ "$attempt" -eq 1 ]; then
      log "  no commit produced on first attempt — see $clog"
      feedback="You produced no commit. Implement Task $N per the plan and commit it (tdd-loop, green, Co-Authored-By trailer)."
      attempt=$((attempt+1)); continue
    fi

    # ---- hard gate 1: the suite must be green ----
    tlog="$LOGDIR/task${N}-attempt${attempt}-test.log"
    trc=0; cargo test --manifest-path "$MANIFEST" >"$tlog" 2>&1 || trc=$?
    if [ "$trc" -ne 0 ]; then
      log "  cargo test RED (exit $trc) — feeding failure back"
      feedback="cargo test is RED (the suite must be green before review). Fix it (failing-test-first for any behavior change), commit when green. Test output tail:
$(tail -n 40 "$tlog")"
      attempt=$((attempt+1)); continue
    fi
    log "  cargo test GREEN"

    # ---- gate 2: independent Codex review of this task's diff ----
    vfile="$LOGDIR/task${N}-attempt${attempt}-verdict.md"
    review="You are an independent code reviewer for the Topology project. Maintainers lead the project and you advise: be rigorous but precise, and do not invent issues.

Review ONLY the work committed for Task $N — the diff: git diff $BEFORE..HEAD on the current branch. For context read $PLAN (the 'Task $N' section) and $SPEC (its Conventions).

Check:
1. Correctness and security-scanner edge cases (non-UTF8/NUL bytes, CRLF, size caps, span-scoped allows).
2. Real TDD: does git log / the diff show failing-test-first (not code-then-test), minimal green, one commit per cycle?
3. Spec fidelity: regex has NO look-around/backreferences; reads bounded to cap+1 (no full allocation before the cap check); allow_blob pinned by git blob OID not sha256; block exits 1, warn -> stderr exits 0.
4. No scope creep past Task $N; no planted secrets; no dangerous shell.

Inspect with git show / git diff / git log and by reading files (you are read-only). List concrete findings as file:line. Then print a FINAL line that is EXACTLY one of:
VERDICT: PASS
VERDICT: FAIL
Choose PASS only if Task $N is correct, faithful to the spec, and TDD-disciplined."

    crc=0; guarded codex "${CODEX_COMMON[@]}" -o "$vfile" "$review" >"$LOGDIR/task${N}-attempt${attempt}-codex.log" 2>&1 || crc=$?

    if [ "$crc" -ne 0 ] || [ ! -s "$vfile" ]; then
      log "  codex review did not complete cleanly (exit $crc) — treating as FAIL"
      feedback="The reviewer did not return a clear verdict (codex exit $crc). Re-examine your Task $N work for correctness and spec fidelity and harden it."
      attempt=$((attempt+1)); continue
    fi

    if grep -Eq '^VERDICT:[[:space:]]*PASS[[:space:]]*$' "$vfile"; then
      log "  VERDICT: PASS  ✔  Task $N accepted"
      passed=1; break
    else
      log "  VERDICT: FAIL  — feeding findings back"
      feedback="$(cat "$vfile")"
      attempt=$((attempt+1))
    fi
  done

  if [ "$passed" -ne 1 ]; then
    log "HALT: Task $N did not reach VERDICT: PASS within $MAX_ATTEMPTS attempts."
    log "      Inspect $LOGDIR and the branch, fix manually, then re-run:  ./scripts/auto-loop.sh $N"
    exit 2
  fi
done

log "ALL TASKS PASSED: ${TASKS[*]}"
log "Review the branch and merge when satisfied:  git log --oneline   /   git diff main..HEAD"
