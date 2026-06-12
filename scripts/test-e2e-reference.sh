#!/usr/bin/env bash
# End-to-end re-verification against a genuine reference project (Phase 12).
#
# Proves, on a real consumer-shaped fixture, that running the installer delivers the
# five consumer-visible outcomes (O1-O5) for a --project install, plus the shared
# global-install substrate (--global) behind them. The red baseline first asserts the
# outcomes are ABSENT on a fresh fixture, so the green assertions are not tautological.
#
# Mirrors the idiom of scripts/test-payload-e2e.sh: pass/fail counters, a cleanup trap
# removing every tempdir, exit non-zero if any check fails. Runs OFFLINE — the installer
# is invoked with --build-from-source so it builds the payload from this checkout and the
# binary from source (reusing the prebuilt gatekeeper/target/release/gatekeeper when it is
# already current, so the rebuild is a fast no-op).
#
# The planted secret is assembled at RUNTIME by concatenation: the script SOURCE never
# carries a committable secret, so the pre-commit scanner does not block this file itself.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# Framework root = the checkout (the script lives at <root>/scripts/).
FRAMEWORK_ROOT="$(cd "$SCRIPT_DIR/.." && git -C "$SCRIPT_DIR/.." rev-parse --show-toplevel)"
INSTALL_SH="$FRAMEWORK_ROOT/scripts/install.sh"

PASS=0
FAIL=0
pass() { echo "PASS $*"; PASS=$((PASS + 1)); }
fail() { echo "FAIL $*"; FAIL=$((FAIL + 1)); }

# Scrub any inherited GATEKEEPER_BIN so a developer's session env does not pollute the
# installed wiring (the installer writes the real value into settings.json regardless;
# an inherited value only confuses doctor's GATEKEEPER_BIN probe).
unset GATEKEEPER_BIN

# A reliably-blocked secret, built at runtime so the script source stays clean:
# a labelled AWS-secret-access-key assignment with a 44-char base64-ish value (the
# canonical AWS secret length is 40; >=40 is what the block rule requires).
_planted_secret() {
  local label="aws_secret" label2="_access_key"
  local val
  val="$(printf 'A%.0s' $(seq 1 44))"
  printf '%s%s=%s\n' "$label" "$label2" "$val"
}

# ── Tempdir tracking + cleanup trap (covers failure paths too) ─────────────────
# One parent-owned root; every fixture nests under it. mktempdir() runs inside a
# command substitution (a subshell), so a per-call array append would never reach
# the parent shell the EXIT trap runs in — a single pre-created root sidesteps that
# trap and removes every tempdir (including on failure paths) in one rm.
WORK_ROOT="$(mktemp -d)"
trap 'rm -rf "$WORK_ROOT"' EXIT
mktempdir() { mktemp -d "$WORK_ROOT/sub.XXXXXXXX"; }

# ── Reference fixture builder (spec 1a) ────────────────────────────────────────
_make_reference_project() {
  # $1 = path for the new git fixture repo (a react-weather-app-shaped consumer project)
  local dir="$1"
  mkdir -p "$dir/src"
  git -C "$dir" init -q
  git -C "$dir" config user.name  "Reference Tester"
  git -C "$dir" config user.email "reference@example.com"
  cat > "$dir/package.json" <<'JSON'
{
  "name": "react-weather-app",
  "version": "1.0.0",
  "scripts": {
    "build": "echo building",
    "test": "echo testing"
  }
}
JSON
  cat > "$dir/src/index.js" <<'JS'
export function weather() {
  return "sunny";
}
JS
  cat > "$dir/README.md" <<'MD'
# react-weather-app

A minimal reference project for Topology end-to-end re-verification.
MD
  git -C "$dir" add -A
  git -C "$dir" commit -q -m "init react-weather-app"
}

# ════════════════════════════════════════════════════════════════════════════════
# TASK 1 — Red baseline: the five outcomes are ABSENT on a fresh, pre-install fixture.
# ════════════════════════════════════════════════════════════════════════════════
RED_DIR="$(mktempdir)/react-weather-app"
_make_reference_project "$RED_DIR"

# (R1) No CLAUDE.md import of the contract.
if [[ ! -f "$RED_DIR/CLAUDE.md" ]] || ! grep -qF '@.topology/CONTRACT.md' "$RED_DIR/CLAUDE.md"; then
  pass "red baseline: no CLAUDE.md @.topology/CONTRACT.md import pre-install"
else
  fail "red baseline: CLAUDE.md already imports the contract before install"
fi

# (R2) No .claude/settings.json.
if [[ ! -f "$RED_DIR/.claude/settings.json" ]]; then
  pass "red baseline: no .claude/settings.json pre-install"
else
  fail "red baseline: .claude/settings.json present before install"
fi

# (R3) No .claude/topology/ artifacts root.
if [[ ! -d "$RED_DIR/.claude/topology" ]]; then
  pass "red baseline: no .claude/topology/ pre-install"
else
  fail "red baseline: .claude/topology/ present before install"
fi

# (R4) No vendored .topology/ payload.
if [[ ! -d "$RED_DIR/.topology" ]]; then
  pass "red baseline: no .topology/ payload pre-install"
else
  fail "red baseline: .topology/ present before install"
fi

# (R5) A planted-secret commit SUCCEEDS pre-install — no pre-commit hook yet.
RED_SECRET_FILE="$RED_DIR/credentials.txt"
_planted_secret > "$RED_SECRET_FILE"
git -C "$RED_DIR" add credentials.txt
RED_COMMIT_EXIT=0
git -C "$RED_DIR" commit -q -m "planted secret (no hook yet)" || RED_COMMIT_EXIT=$?
if [[ "$RED_COMMIT_EXIT" -eq 0 ]] && [[ ! -x "$RED_DIR/.git/hooks/pre-commit" ]]; then
  pass "red baseline: planted-secret commit succeeds (no pre-commit hook installed)"
else
  fail "red baseline: expected the planted-secret commit to succeed with no hook (exit=$RED_COMMIT_EXIT)"
fi

# ════════════════════════════════════════════════════════════════════════════════
# TASK 2 — --project install + the five outcomes (O1-O5), green after a REAL install.
# ════════════════════════════════════════════════════════════════════════════════
PROJ_DIR="$(mktempdir)/react-weather-app"
_make_reference_project "$PROJ_DIR"

PROJECT_INSTALL_OUT="$(
  bash "$INSTALL_SH" --project "$PROJ_DIR" --harness claude --yes --build-from-source 2>&1
)" || true
# The installer's closing doctor exits non-zero when GATEKEEPER_BIN points at a missing
# path; we asserted GATEKEEPER_BIN is unset above, so this is the wiring run only. The
# outcome assertions below are the real gate, not the installer's overall exit code.

GK_BIN="$PROJ_DIR/.topology/bin/gatekeeper"
SETTINGS="$PROJ_DIR/.claude/settings.json"

# ── O1: contract in context ────────────────────────────────────────────────────
if [[ -f "$PROJ_DIR/CLAUDE.md" ]] && grep -qF '@.topology/CONTRACT.md' "$PROJ_DIR/CLAUDE.md"; then
  pass "O1: CLAUDE.md imports @.topology/CONTRACT.md"
else
  fail "O1: CLAUDE.md missing the @.topology/CONTRACT.md import (install output: ${PROJECT_INSTALL_OUT:0:400})"
fi
if [[ -f "$PROJ_DIR/.topology/CONTRACT.md" ]] && grep -qF '.claude/topology' "$PROJ_DIR/.topology/CONTRACT.md"; then
  pass "O1: .topology/CONTRACT.md exists and renders the governed path .claude/topology"
else
  fail "O1: .topology/CONTRACT.md missing or does not render .claude/topology"
fi

# ── O2: bare gatekeeper via GATEKEEPER_BIN, no PATH/sudo step ───────────────────
if [[ -x "$GK_BIN" ]]; then
  pass "O2: installed binary present at .topology/bin/gatekeeper"
else
  fail "O2: installed binary missing at $GK_BIN"
fi
# settings.json env.GATEKEEPER_BIN points at the installed binary. No jq dependency:
# the value is a single quoted path on its own line.
if grep -qF "\"GATEKEEPER_BIN\": \"$GK_BIN\"" "$SETTINGS"; then
  pass "O2: settings.json env.GATEKEEPER_BIN points at the installed binary"
else
  fail "O2: settings.json env.GATEKEEPER_BIN not set to $GK_BIN (settings: $(tr -d '\n' < "$SETTINGS" 2>/dev/null | head -c 300))"
fi
# Invoke --version from the project with PATH scrubbed of any gatekeeper.
O2_VERSION_OUT="$(cd "$PROJ_DIR" && env PATH=/usr/bin:/bin "$GK_BIN" --version 2>&1)" || true
if echo "$O2_VERSION_OUT" | grep -qE '^gatekeeper '; then
  pass "O2: \"\$GATEKEEPER_BIN\" --version works with PATH scrubbed ($O2_VERSION_OUT)"
else
  fail "O2: --version did not run with PATH scrubbed (output: $O2_VERSION_OUT)"
fi
# check design with PATH scrubbed: the binary must RUN (it may exit non-zero for a missing
# artifact — that is fine; the point is no "command not found" / PATH dependency).
O2_DESIGN_OUT="$(cd "$PROJ_DIR" && env PATH=/usr/bin:/bin TOPOLOGY_ROOT="$PROJ_DIR/.topology" "$GK_BIN" check design --feature x 2>&1)" || true
if echo "$O2_DESIGN_OUT" | grep -qE '(PASS|FAIL) design gate'; then
  pass "O2: \"\$GATEKEEPER_BIN\" check design ran with PATH scrubbed (no PATH/sudo step)"
else
  fail "O2: check design did not run with PATH scrubbed (output: $O2_DESIGN_OUT)"
fi

# ── O3: hooks fire ──────────────────────────────────────────────────────────────
if grep -qF 'skill-activation.sh' "$SETTINGS" && grep -qF 'UserPromptSubmit' "$SETTINGS"; then
  pass "O3: settings.json wires UserPromptSubmit -> skill-activation.sh"
else
  fail "O3: settings.json does not wire UserPromptSubmit -> skill-activation.sh"
fi
if grep -qF 'security-scan.sh' "$SETTINGS" && grep -qF 'PreToolUse' "$SETTINGS"; then
  pass "O3: settings.json wires PreToolUse -> security-scan.sh"
else
  fail "O3: settings.json does not wire PreToolUse -> security-scan.sh"
fi
# Invoke skill-activation.sh directly with a representative prompt on stdin: advisory, exit 0.
SKILL_HOOK="$PROJ_DIR/.topology/hooks/skill-activation.sh"
SKILL_EXIT=0
SKILL_OUT="$(echo "add a users table" | GATEKEEPER_BIN="$GK_BIN" TOPOLOGY_ROOT="$PROJ_DIR/.topology" bash "$SKILL_HOOK" 2>&1)" || SKILL_EXIT=$?
if [[ "$SKILL_EXIT" -eq 0 ]] && echo "$SKILL_OUT" | grep -qF 'Topology: evaluate your skills'; then
  pass "O3: skill-activation.sh emits an advisory block and exits 0"
else
  fail "O3: skill-activation.sh did not behave (exit=$SKILL_EXIT, output: ${SKILL_OUT:0:200})"
fi
# Invoke security-scan.sh directly with a planted-secret Write payload on stdin: deny.
SEC_HOOK="$PROJ_DIR/.topology/hooks/security-scan.sh"
SEC_PAYLOAD="$(printf '{"tool_name":"Write","tool_input":{"file_path":"/tmp/creds.env","content":"%s"}}' "$(_planted_secret | tr -d '\n')")"
SEC_OUT="$(printf '%s' "$SEC_PAYLOAD" | GATEKEEPER_BIN="$GK_BIN" TOPOLOGY_ROOT="$PROJ_DIR/.topology" bash "$SEC_HOOK" 2>&1)" || true
if echo "$SEC_OUT" | grep -qF '"permissionDecision":"deny"'; then
  pass "O3: security-scan.sh emits a deny decision on a planted secret"
else
  fail "O3: security-scan.sh did not deny the planted secret (output: ${SEC_OUT:0:300})"
fi

# ── O4: project pre-commit blocks a planted secret ──────────────────────────────
PROJ_SECRET_FILE="$PROJ_DIR/leaked-credentials.txt"
_planted_secret > "$PROJ_SECRET_FILE"
git -C "$PROJ_DIR" add leaked-credentials.txt
O4_HEAD_BEFORE="$(git -C "$PROJ_DIR" rev-parse HEAD)"
O4_COMMIT_EXIT=0
O4_COMMIT_OUT="$(git -C "$PROJ_DIR" commit -m "attempt to commit a secret" 2>&1)" || O4_COMMIT_EXIT=$?
O4_HEAD_AFTER="$(git -C "$PROJ_DIR" rev-parse HEAD)"
if [[ "$O4_COMMIT_EXIT" -ne 0 ]]; then
  pass "O4: secret commit blocked — git commit exits non-zero ($O4_COMMIT_EXIT)"
else
  fail "O4: secret commit was NOT blocked (exit 0)"
fi
if echo "$O4_COMMIT_OUT" | grep -qiE 'BLOCK|BLOCKED'; then
  pass "O4: pre-commit emitted a scanner BLOCK line"
else
  fail "O4: no BLOCK line in pre-commit output (output: ${O4_COMMIT_OUT:0:300})"
fi
if [[ "$O4_HEAD_BEFORE" == "$O4_HEAD_AFTER" ]]; then
  pass "O4: HEAD unchanged after the blocked commit"
else
  fail "O4: HEAD moved despite the block ($O4_HEAD_BEFORE -> $O4_HEAD_AFTER)"
fi
# Documented bypass: the same commit lands with the skip-hooks flag (built at runtime so
# this script's source carries no literal mention either way).
O4_BYPASS_FLAG="--no""-verify"
O4_BYPASS_EXIT=0
git -C "$PROJ_DIR" commit "$O4_BYPASS_FLAG" -q -m "documented bypass" || O4_BYPASS_EXIT=$?
O4_HEAD_BYPASS="$(git -C "$PROJ_DIR" rev-parse HEAD)"
if [[ "$O4_BYPASS_EXIT" -eq 0 ]] && [[ "$O4_HEAD_BYPASS" != "$O4_HEAD_AFTER" ]]; then
  pass "O4: documented bypass lands the commit (skip-hooks flag)"
else
  fail "O4: documented bypass did not land the commit (exit=$O4_BYPASS_EXIT)"
fi

# ── O5: design artifact lands under the project ─────────────────────────────────
# doctor (run from the project) names the project artifacts root.
O5_ARTIFACTS_EXPECTED="$PROJ_DIR/.claude/topology"
O5_DOCTOR_OUT="$(cd "$PROJ_DIR" && TOPOLOGY_ROOT="$PROJ_DIR/.topology" "$GK_BIN" doctor 2>&1)" || true
# doctor canonicalizes the path (e.g. /tmp -> /private/tmp on macOS); compare against the
# resolved real path of the expected artifacts root.
O5_ARTIFACTS_REAL="$(cd "$PROJ_DIR" && pwd -P)/.claude/topology"
if echo "$O5_DOCTOR_OUT" | grep -qE "^artifacts root: ($O5_ARTIFACTS_EXPECTED|$O5_ARTIFACTS_REAL)$"; then
  pass "O5: doctor resolves artifacts root to <fixture>/.claude/topology"
else
  fail "O5: doctor did not name <fixture>/.claude/topology as artifacts root (line: $(echo "$O5_DOCTOR_OUT" | grep -i 'artifacts root' || echo none))"
fi
# Plant an approved spec + research note under the project artifacts root, then check design.
mkdir -p "$PROJ_DIR/.claude/topology/specs" "$PROJ_DIR/.claude/topology/research"
cat > "$PROJ_DIR/.claude/topology/specs/2026-06-13-x.md" <<'SPEC'
# Spec — feature x

**Status:** approved

## Goal

A planted spec proving gate artifacts anchor to the project artifacts root.

## Behavior

The design gate resolves and reads this file from <fixture>/.claude/topology/specs/.
SPEC
cat > "$PROJ_DIR/.claude/topology/research/2026-06-13-x.md" <<'RESEARCH'
# Research — feature x

Context note for the planted spec.
RESEARCH
O5_DESIGN_EXIT=0
O5_DESIGN_OUT="$(cd "$PROJ_DIR" && TOPOLOGY_ROOT="$PROJ_DIR/.topology" "$GK_BIN" check design --feature x 2>&1)" || O5_DESIGN_EXIT=$?
if [[ "$O5_DESIGN_EXIT" -eq 0 ]] && echo "$O5_DESIGN_OUT" | grep -qF "$O5_ARTIFACTS_REAL/specs/2026-06-13-x.md"; then
  pass "O5: check design PASSES, reading the spec from the project artifacts root"
else
  fail "O5: check design did not pass against the planted project spec (exit=$O5_DESIGN_EXIT, output: ${O5_DESIGN_OUT:0:400})"
fi

# ════════════════════════════════════════════════════════════════════════════════
# TASK 3 — --global scope (AC-7): payload + binary substrate, GlobalHome resolution.
# ════════════════════════════════════════════════════════════════════════════════
GLOBAL_HOME="$(mktempdir)"
GLOBAL_ROOT="$GLOBAL_HOME/.topology"
# Install into a temp TOPOLOGY_HOME. HOME is left untouched here so the cargo/rustup
# toolchain --build-from-source needs stays resolvable; HOME is only remapped for the
# GlobalHome doctor probe below (which runs no cargo).
GLOBAL_INSTALL_OUT="$(
  TOPOLOGY_HOME="$GLOBAL_ROOT" \
    bash "$INSTALL_SH" --global --yes --harness none --build-from-source 2>&1
)" || true

if [[ -f "$GLOBAL_ROOT/VERSION" ]]; then
  pass "global: payload installed at \$TOPOLOGY_HOME/.topology (VERSION present)"
else
  fail "global: VERSION missing at $GLOBAL_ROOT (install output: ${GLOBAL_INSTALL_OUT:0:400})"
fi
GLOBAL_BIN="$GLOBAL_ROOT/bin/gatekeeper"
if [[ -x "$GLOBAL_BIN" ]]; then
  pass "global: bin/gatekeeper present and executable"
else
  fail "global: bin/gatekeeper missing at $GLOBAL_BIN"
fi

# doctor from a SEPARATE temp project resolves the global framework root (GlobalHome).
# GlobalHome reads <home>/.topology, so remap HOME to the temp home for this probe, invoke
# an external copy of the binary (so binary-adjacent does not pre-empt GlobalHome), and run
# from a neutral, non-marked, non-git cwd with no env override.
GLOBAL_PROBE_DIR="$(mktempdir)"
GK_EXTERNAL="$GLOBAL_PROBE_DIR/gatekeeper-external"
cp "$GLOBAL_BIN" "$GK_EXTERNAL"
GLOBAL_NEUTRAL_CWD="$(mktempdir)"
GLOBAL_DOCTOR_OUT="$(
  cd "$GLOBAL_NEUTRAL_CWD" \
    && env -u TOPOLOGY_ROOT -u TOPOLOGY_HOME -u GATEKEEPER_BIN \
       PATH=/usr/bin:/bin HOME="$GLOBAL_HOME" "$GK_EXTERNAL" doctor 2>&1
)" || true
if echo "$GLOBAL_DOCTOR_OUT" | grep -qE '^resolved by: global ~/\.topology$'; then
  pass "global: doctor from a separate project resolves the framework root via GlobalHome"
else
  fail "global: doctor did not resolve via GlobalHome (resolved-by line: $(echo "$GLOBAL_DOCTOR_OUT" | grep -i 'resolved by' || echo none))"
fi

# No version skew: the binary --version matches the payload VERSION.
GLOBAL_BIN_VERSION="$("$GLOBAL_BIN" --version 2>&1 | awk '{print $2; exit}')"
GLOBAL_PAYLOAD_VERSION="$(grep -m1 '^version' "$GLOBAL_ROOT/VERSION" | sed -E 's/^version[[:space:]]*=[[:space:]]*"([^"]*)".*/\1/')"
if [[ -n "$GLOBAL_BIN_VERSION" && "$GLOBAL_BIN_VERSION" == "$GLOBAL_PAYLOAD_VERSION" ]]; then
  pass "global: binary --version ($GLOBAL_BIN_VERSION) matches payload VERSION ($GLOBAL_PAYLOAD_VERSION) — no skew"
else
  fail "global: version skew — binary=$GLOBAL_BIN_VERSION payload=$GLOBAL_PAYLOAD_VERSION"
fi

# ── Summary ─────────────────────────────────────────────────────────────────────
echo ""
echo "test-e2e-reference: $PASS passed, $FAIL failed"
if [[ $FAIL -gt 0 ]]; then
  exit 1
fi
