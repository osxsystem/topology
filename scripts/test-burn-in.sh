#!/usr/bin/env bash
# test-burn-in.sh — self-tests for the Phase 15 burn-in harness scripts.
#
# These test the *orchestration* of burn-in-entropy-sweep.sh and
# burn-in-replay-tdd.sh (exclusion, counting, rate math, log truncation,
# zero-data handling) using throwaway git repos and a STUB `gatekeeper`
# injected via GATEKEEPER_BIN. The engines' real detection is covered by the
# Rust suites (cli_scan_bench.rs, cli_tdd_replay.rs); here we never run the
# slow real replay.
set -uo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PASS=0
FAIL=0
ok() {
  PASS=$((PASS + 1))
  echo "  $1 PASS"
}
bad() {
  FAIL=$((FAIL + 1))
  echo "  $1 FAIL: $2"
}

HEX64="0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"

# A throwaway git repo.
mk_repo() {
  local d
  d="$(mktemp -d "${TMPDIR:-/tmp}/burnin-test.XXXXXX")"
  git -C "$d" init -q -b main
  git -C "$d" config user.email t@t.t
  git -C "$d" config user.name t
  printf '%s' "$d"
}

# A stub `gatekeeper`, created OUTSIDE any repo (so `git ls-files` never sees it):
# `scan --content` WARNs on a >=32 hex run; `check tdd` emits one replay SHADOW line.
# Returns the stub path on stdout.
mk_stub() {
  local s
  s="$(mktemp "${TMPDIR:-/tmp}/burnin-stub.XXXXXX")"
  cat >"$s" <<'STUB'
#!/usr/bin/env bash
case "$1" in
  scan)
    data="$(cat)"
    if printf '%s' "$data" | grep -qE '[0-9a-f]{32,}'; then
      echo "WARN hex-high-entropy: high-entropy token [stdin] (redacted: xxx)" >&2
    fi
    exit 0
    ;;
  check)
    echo 'SHADOW {"ts":1,"gate":"tdd","check":"replay","configured":"default","artifact":null,"command":"x","result":"pass","detail":"stub"}'
    exit 0
    ;;
esac
exit 0
STUB
  chmod +x "$s"
  printf '%s' "$s"
}

# ── entropy: zero data (only excluded files) ────────────────────────────────
test_entropy_zero_data() {
  local d stub out rc=0
  d="$(mk_repo)"
  stub="$(mk_stub)"
  printf '%s\n' "$HEX64" >"$d/a.lock" # excluded glob
  git -C "$d" add -A
  git -C "$d" commit -qm x
  out="$(cd "$d" && GATEKEEPER_BIN="$stub" bash "$REPO_ROOT/scripts/burn-in-entropy-sweep.sh" 2>&1)" || rc=$?
  rm -rf "$d" "$stub"
  if [ "$rc" -eq 0 ] && printf '%s' "$out" | grep -q "0 evaluations"; then
    ok entropy_zero_data
  else
    bad entropy_zero_data "rc=$rc out=[$out]"
  fi
}

# ── entropy: detects a hit, excludes a .lock ────────────────────────────────
test_entropy_detects_and_excludes() {
  local d stub out rc=0
  d="$(mk_repo)"
  stub="$(mk_stub)"
  printf '%s\n' "$HEX64" >"$d/secret.txt"
  printf '%s\n' "$HEX64" >"$d/vendor.lock"
  git -C "$d" add -A
  git -C "$d" commit -qm x
  out="$(cd "$d" && GATEKEEPER_BIN="$stub" bash "$REPO_ROOT/scripts/burn-in-entropy-sweep.sh" 2>&1)" || rc=$?
  rm -rf "$d" "$stub"
  if [ "$rc" -eq 0 ] &&
    printf '%s' "$out" | grep -qE "files scanned: +1" &&
    printf '%s' "$out" | grep -qE "excluded \(path glob\): +1" &&
    printf '%s' "$out" | grep -qE "entropy WARN hits: +1"; then
    ok entropy_detects_and_excludes
  else
    bad entropy_detects_and_excludes "rc=$rc out=[$out]"
  fi
}

# ── replay: zero data (no merges) ───────────────────────────────────────────
test_replay_zero_data() {
  local d stub out rc=0
  d="$(mk_repo)"
  stub="$(mk_stub)"
  echo base >"$d/base.txt"
  git -C "$d" add -A
  git -C "$d" commit -qm base
  out="$(cd "$d" && GATEKEEPER_BIN="$stub" bash "$REPO_ROOT/scripts/burn-in-replay-tdd.sh" 2>&1)" || rc=$?
  rm -rf "$d" "$stub"
  if [ "$rc" -eq 0 ] && printf '%s' "$out" | grep -q "0 evaluations"; then
    ok replay_zero_data
  else
    bad replay_zero_data "rc=$rc out=[$out]"
  fi
}

# ── replay: log truncated per run (idempotent) ──────────────────────────────
test_replay_idempotent() {
  local d stub rc=0 n1 n2
  d="$(mk_repo)"
  stub="$(mk_stub)"
  echo base >"$d/base.txt"
  git -C "$d" add -A
  git -C "$d" commit -qm base
  git -C "$d" checkout -q -b feat
  echo t >"$d/test.txt"
  git -C "$d" add -A
  git -C "$d" commit -qm test
  git -C "$d" checkout -q main
  git -C "$d" merge -q --no-ff feat -m "merge feat"
  (cd "$d" && GATEKEEPER_BIN="$stub" bash "$REPO_ROOT/scripts/burn-in-replay-tdd.sh" >/dev/null 2>&1) || rc=$?
  n1=$(grep -c '"check":"replay"' "$d/docs/logs/burn-in-tdd.jsonl" 2>/dev/null || echo 0)
  (cd "$d" && GATEKEEPER_BIN="$stub" bash "$REPO_ROOT/scripts/burn-in-replay-tdd.sh" >/dev/null 2>&1) || rc=$?
  n2=$(grep -c '"check":"replay"' "$d/docs/logs/burn-in-tdd.jsonl" 2>/dev/null || echo 0)
  rm -rf "$d" "$stub"
  if [ "$rc" -eq 0 ] && [ "$n1" = "1" ] && [ "$n2" = "1" ]; then
    ok replay_idempotent
  else
    bad replay_idempotent "rc=$rc n1=$n1 n2=$n2 (expected 1 and 1)"
  fi
}

echo "burn-in harness self-tests"
test_entropy_zero_data
test_entropy_detects_and_excludes
test_replay_zero_data
test_replay_idempotent
echo "----"
echo "$PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]
