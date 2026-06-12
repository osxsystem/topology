#!/usr/bin/env bash
# Install Topology: acquire gatekeeper binary, wire symlinks, hooks, and optionally a harness.
#
# Works two ways:
#   1. Piped/sourced — no BASH_SOURCE file: downloads (or re-downloads) the distribution
#      payload tarball from GitHub releases, verifies its SHA-256 checksum, and unpacks it
#      into the target directory.
#   2. Inside a checkout — BASH_SOURCE resolves to a real file: builds the payload from the
#      checkout for both scopes (no clone; the checkout is never used as the install root).
#
# Options:
#   --harness <claude|codex|cursor|opencode|none>
#                         Wire the named harness (default: ask, or 'claude' when
#                         a project is being wired non-interactively).
#   --global              Install globally in ${TOPOLOGY_HOME:-$HOME/.topology} (default).
#   --project <path>      Install locally: vendor payload at <path>/.topology and
#                         wire <path>; mutually exclusive with --global.
#   --yes                 Accept defaults for any unanswered prompt.
#   --build-from-source   Skip the prebuilt download; build with cargo instead.
#
# Test seams:
#   TOPOLOGY_HOME                 Override the global install destination (default $HOME/.topology).
#   TOPOLOGY_RELEASE_BASE_URL     Override the binary/payload download URL prefix (file:// works).
#   TOPOLOGY_VERSION              Override the pinned version.
#   PROMPT_INPUT_FD               Override the fd used for interactive prompts (default /dev/tty).
set -euo pipefail

# ─── Helpers ─────────────────────────────────────────────────────────────────

usage() {
  cat >&2 <<'USAGE'
Usage: install.sh [OPTIONS]

Options:
  --harness <claude|codex|cursor|opencode|none>  Harness to wire (default: ask or 'claude')
  --global                                        Install globally in ~/.topology (default)
  --project <path>                                Vendor topology into <path>/.topology and wire it
  --yes                                           Accept all defaults non-interactively
  --build-from-source                             Build gatekeeper from source instead of downloading
USAGE
}

# Interactive prompts read from /dev/tty by default; the PROMPT_INPUT_FD test seam
# (already used by the stale-PATH repair) substitutes a readable path so the e2e
# suite can exercise interactive branches without a tty. Production behavior is
# unchanged when the variable is unset.
can_prompt() {
  if [[ -n "${PROMPT_INPUT_FD:-}" && -r "${PROMPT_INPUT_FD}" ]]; then
    return 0
  fi
  ( : < /dev/tty ) 2>/dev/null
}

# Read one line from /dev/tty (or $PROMPT_INPUT_FD when set); if unavailable, echo
# the default. Usage: ask "question" "default"  → prints result to stdout
ask() {
  local question="$1"
  local default="$2"
  local answer
  if [[ -n "${PROMPT_INPUT_FD:-}" && -r "${PROMPT_INPUT_FD}" ]]; then
    printf '%s [%s]: ' "$question" "$default" >&2
    read -r answer < "$PROMPT_INPUT_FD" || answer=""
  else
    printf '%s [%s]: ' "$question" "$default" > /dev/tty
    read -r answer < /dev/tty
  fi
  echo "${answer:-$default}"
}

# ─── Arg parsing ─────────────────────────────────────────────────────────────

BUILD_FROM_SOURCE=0
HARNESS_FLAG=""
SCOPE_GLOBAL=0
SCOPE_PROJECT=""
YES=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --build-from-source) BUILD_FROM_SOURCE=1; shift ;;
    --harness)
      if [[ $# -lt 2 ]]; then
        echo "error: --harness requires a value" >&2
        usage
        exit 2
      fi
      HARNESS_FLAG="$2"; shift 2 ;;
    --global) SCOPE_GLOBAL=1; shift ;;
    --project)
      if [[ $# -lt 2 ]]; then
        echo "error: --project requires a value" >&2
        usage
        exit 2
      fi
      SCOPE_PROJECT="$2"; shift 2 ;;
    --yes) YES=1; shift ;;
    *)
      echo "error: unexpected argument '$1'" >&2
      usage
      exit 2 ;;
  esac
done

if [[ $SCOPE_GLOBAL -eq 1 && -n "$SCOPE_PROJECT" ]]; then
  echo "error: --global and --project are mutually exclusive" >&2
  usage
  exit 2
fi

# ─── 1. Scope resolution ─────────────────────────────────────────────────────

SCOPE="global"
PROJECT_PATH=""

if [[ -n "$SCOPE_PROJECT" ]]; then
  SCOPE="local"
  PROJECT_PATH="$(cd "$SCOPE_PROJECT" 2>/dev/null && pwd)" || PROJECT_PATH=""
  if [[ -z "$PROJECT_PATH" ]]; then
    echo "error: --project '$SCOPE_PROJECT' does not exist" >&2
    exit 1
  fi
elif [[ $SCOPE_GLOBAL -eq 1 ]]; then
  SCOPE="global"
elif [[ $YES -eq 1 ]] || ! can_prompt; then
  SCOPE="global"
  echo "assumed: scope=global (use --project <path> to override)"
else
  # Let the user know which directory they are in before asking about scope —
  # this matters when piping curl | bash because cwd can be surprising.
  printf 'Current directory: %s\n' "$(pwd)" > /dev/tty
  answer=$(ask "Install scope — (g)lobal or (l)ocal?" "g")
  case "$answer" in
    l|local) SCOPE="local" ;;
    *) SCOPE="global" ;;
  esac
fi

if [[ "$SCOPE" == "local" && -z "$PROJECT_PATH" ]]; then
  if [[ $YES -eq 1 ]] || ! can_prompt; then
    PROJECT_PATH="$(pwd)"
    echo "assumed: project path=$(pwd) (use --project <path> to override)"
  else
    answer=$(ask "Project path" "$(pwd)")
    PROJECT_PATH="$(cd "$answer" 2>/dev/null && pwd)" || PROJECT_PATH=""
    if [[ -z "$PROJECT_PATH" ]]; then
      echo "error: project path '$answer' does not exist" >&2
      exit 1
    fi
  fi
fi

if [[ "$SCOPE" == "local" ]]; then
  if [[ ! -e "$PROJECT_PATH/.git" ]]; then
    echo "error: $PROJECT_PATH does not contain a .git repository" >&2
    exit 1
  fi
fi

# Detect incompatible combination early — before any downloading or unpacking.
# A payload-based local install has no Rust source, so --build-from-source requires
# a real checkout (BASH_SOURCE resolves to a file). Fail now rather than after the
# payload download so the user's directory is not left half-installed.
if [[ "$SCOPE" == "local" && $BUILD_FROM_SOURCE -eq 1 ]] \
   && ! [[ -n "${BASH_SOURCE[0]:-}" && -f "${BASH_SOURCE[0]}" ]]; then
  echo "error: --build-from-source with a piped install has no Rust source tree." >&2
  echo "  Remedy: clone https://github.com/osxsystem/topology and run" >&2
  echo "    scripts/install.sh --project <path> --build-from-source" >&2
  echo "  from inside the checkout." >&2
  exit 1
fi

# ─── 2. Shared payload helpers ───────────────────────────────────────────────
# Hoisted above the scope branch so both global and local paths share the same
# download/verify/unpack and root-handling machinery.

_unpack_payload() {
  # $1 = tarball path (must exist and be readable)
  # Unpacks into $ROOT (caller must mkdir -p $ROOT beforehand).
  local tarball="$1"
  tar -xzf "$tarball" -C "$ROOT"
}

_rescue_legacy_clone_local() {
  # Local-scope rescue: copies known in-tree project-state files to the canonical
  # artifacts root ($PROJECT_PATH/.claude/topology/) so they survive the checkout
  # deletion. Never overwrites an existing destination.
  local ledger_src="$ROOT/docs/learn/ledger.md"
  local ledger_dst="$PROJECT_PATH/.claude/topology/learn/ledger.md"
  if [[ -f "$ledger_src" ]]; then
    if [[ ! -f "$ledger_dst" ]]; then
      mkdir -p "$(dirname "$ledger_dst")"
      cp "$ledger_src" "$ledger_dst"
      echo "    rescued: $ledger_src → $ledger_dst"
    else
      echo "    skipped (already exists): $ledger_dst"
    fi
  fi
  # Memory handoffs — copy any *.handoff.md files from docs/memory/.
  local memory_src="$ROOT/docs/memory"
  local memory_dst="$PROJECT_PATH/.claude/topology/memory"
  if [[ -d "$memory_src" ]]; then
    local handoff
    for handoff in "$memory_src"/*.handoff.md; do
      [[ -e "$handoff" ]] || continue
      local dst_file
      dst_file="$memory_dst/$(basename "$handoff")"
      if [[ ! -f "$dst_file" ]]; then
        mkdir -p "$memory_dst"
        cp "$handoff" "$dst_file"
        echo "    rescued: $handoff → $dst_file"
      else
        echo "    skipped (already exists): $dst_file"
      fi
    done
  fi
}

_rescue_legacy_clone_global() {
  # Global-scope rescue: a global clone has no project context, so there is no
  # canonical artifacts root to rescue into. Instead, copy the in-tree state into
  # a timestamped sibling backup: ${ROOT}-backup-<YYYYmmdd-HHMMSS>/.
  # Never overwrites an existing backup file; prints every rescued path.
  # Deliberately does NOT write into /.claude/topology (PROJECT_PATH is empty in
  # global scope — guard against that path bug).
  local ts
  ts="$(date '+%Y%m%d-%H%M%S')"
  local backup_dir="${ROOT}-backup-${ts}"
  local ledger_src="$ROOT/docs/learn/ledger.md"
  local ledger_dst="$backup_dir/docs/learn/ledger.md"
  local rescued=0
  if [[ -f "$ledger_src" ]]; then
    if [[ ! -f "$ledger_dst" ]]; then
      mkdir -p "$(dirname "$ledger_dst")"
      cp "$ledger_src" "$ledger_dst"
      echo "    rescued: $ledger_src → $ledger_dst"
      rescued=$((rescued + 1))
    else
      echo "    skipped (already exists): $ledger_dst"
    fi
  fi
  local memory_src="$ROOT/docs/memory"
  local memory_dst="$backup_dir/docs/memory"
  if [[ -d "$memory_src" ]]; then
    local handoff
    for handoff in "$memory_src"/*.handoff.md; do
      [[ -e "$handoff" ]] || continue
      local dst_file
      dst_file="$memory_dst/$(basename "$handoff")"
      if [[ ! -f "$dst_file" ]]; then
        mkdir -p "$memory_dst"
        cp "$handoff" "$dst_file"
        echo "    rescued: $handoff → $dst_file"
        rescued=$((rescued + 1))
      else
        echo "    skipped (already exists): $dst_file"
      fi
    done
  fi
  if [[ $rescued -gt 0 ]]; then
    echo "    backup: $backup_dir ($rescued file(s) rescued)"
  else
    echo "    nothing to rescue (no ledger or handoffs found in legacy clone)"
    # Remove the empty backup dir if nothing was written.
    rmdir "$backup_dir" 2>/dev/null || true
  fi
}

_handle_existing_root() {
  # $ROOT already exists; decide whether and how to replace it. Callers invoke
  # this only AFTER the replacement payload is in hand (built or downloaded +
  # verified), so a failed acquisition never destroys a working install.
  if [[ -f "$ROOT/VERSION" ]]; then
    # Payload install: safe to replace in-place because project state is elsewhere.
    echo "==> Upgrading existing payload at $ROOT"
    rm -rf "$ROOT"
  elif [[ -d "$ROOT/.git" ]]; then
    # Legacy clone: attempt best-effort rescue of in-tree state before removing.
    echo "==> Legacy clone detected at $ROOT; rescuing any in-tree state before replacing."
    if [[ "$SCOPE" == "global" ]]; then
      _rescue_legacy_clone_global
    else
      _rescue_legacy_clone_local
    fi
    # Ask permission before deleting the checkout.
    if [[ $YES -eq 1 ]] || ! can_prompt; then
      echo "WARNING: replacing legacy clone at $ROOT with the payload (--yes assumed)"
      rm -rf "$ROOT"
    else
      answer=$(ask "replace legacy clone at $ROOT with the payload?" "N")
      case "$answer" in
        y|Y|yes|Yes) rm -rf "$ROOT" ;;
        *)
          echo "Aborted: legacy clone left intact at $ROOT." >&2
          exit 1
          ;;
      esac
    fi
  else
    echo "error: $ROOT exists but contains neither a VERSION file nor a .git directory." >&2
    echo "  Cannot determine safe upgrade path; refusing to touch it." >&2
    echo "  Remove $ROOT manually and re-run." >&2
    exit 1
  fi
}

_download_and_verify_payload() {
  # Downloads topology-payload.tar.gz + SHA256SUMS into $TMPDIR_DL and verifies the
  # checksum. Sets TMPDIR_DL as a side effect; caller must clean it up.
  # Uses TOPOLOGY_RELEASE_BASE_URL, TOPOLOGY_VERSION (both optional env overrides).
  OS="$(uname -s)"
  case "$OS" in
    Darwin) SHASUM_CMD="shasum -a 256" ;;
    Linux)  SHASUM_CMD="sha256sum" ;;
    *)
      echo "error: unsupported OS '$OS' for payload download" >&2
      exit 1
      ;;
  esac

  # BASE is the release *download* prefix (ends in /download for the GitHub default).
  # The latest-release URL is derived by stripping /download and appending
  # /latest/download — for the GitHub default this yields
  # https://github.com/osxsystem/topology/releases/latest/download/...,
  # and for a file:///tmp/release test override (no /download suffix) it yields
  # file:///tmp/release/latest/download/..., matching the test fixture layout.
  BASE="${TOPOLOGY_RELEASE_BASE_URL:-https://github.com/osxsystem/topology/releases/download}"
  local payload_url sums_url
  if [[ -n "${TOPOLOGY_VERSION:-}" ]]; then
    payload_url="$BASE/v$TOPOLOGY_VERSION/topology-payload.tar.gz"
    sums_url="$BASE/v$TOPOLOGY_VERSION/SHA256SUMS"
  else
    local latest_base="${BASE%/download}/latest/download"
    payload_url="$latest_base/topology-payload.tar.gz"
    sums_url="$latest_base/SHA256SUMS"
  fi

  TMPDIR_DL="$(mktemp -d)"
  # Register cleanup immediately — a curl failure under set -e must not leak the
  # temp dir. The EXIT trap also covers the error paths below and the unpack itself;
  # callers clear it (trap - EXIT) once the payload is installed.
  cleanup_dl() { rm -rf "$TMPDIR_DL"; }
  trap cleanup_dl EXIT

  echo "==> Downloading payload from $payload_url" >&2
  curl -fsSL --max-time 60 -o "$TMPDIR_DL/topology-payload.tar.gz" "$payload_url"

  echo "==> Downloading SHA256SUMS from $sums_url" >&2
  curl -fsSL --max-time 60 -o "$TMPDIR_DL/SHA256SUMS" "$sums_url"

  # Filter to only the payload line, then verify — chatter goes to stderr.
  local sums_line
  sums_line="$(grep "topology-payload.tar.gz" "$TMPDIR_DL/SHA256SUMS" || true)"
  if [[ -z "$sums_line" ]]; then
    echo "error: topology-payload.tar.gz not found in SHA256SUMS" >&2
    exit 1
  fi
  echo "$sums_line" > "$TMPDIR_DL/SHA256SUMS.single"
  (cd "$TMPDIR_DL" && $SHASUM_CMD -c SHA256SUMS.single >&2) || {
    echo "error: payload checksum verification failed" >&2
    exit 1
  }
}

# ─── 3. Locate or create the framework tree ──────────────────────────────────

if [[ "$SCOPE" == "global" ]]; then
  ROOT="${TOPOLOGY_HOME:-$HOME/.topology}"
  # Strip any trailing slash: ${ROOT}-backup-<ts> would otherwise be a CHILD of
  # ROOT, and the legacy-clone rescue's backup would be deleted with the clone.
  ROOT="${ROOT%/}"

  # Fail early for piped+--build-from-source (no Rust source available in a
  # payload install). Same rule as local scope.
  if [[ $BUILD_FROM_SOURCE -eq 1 ]] \
     && ! [[ -n "${BASH_SOURCE[0]:-}" && -f "${BASH_SOURCE[0]}" ]]; then
    echo "error: --build-from-source with a piped global install has no Rust source tree." >&2
    echo "  Remedy: clone https://github.com/osxsystem/topology and run" >&2
    echo "    scripts/install.sh --global --build-from-source" >&2
    echo "  from inside the checkout." >&2
    exit 1
  fi

  if [[ -n "${BASH_SOURCE[0]:-}" && -f "${BASH_SOURCE[0]}" ]]; then
    # Checkout mode: build the payload from the checkout and install into ROOT,
    # mirroring --project checkout behavior. The checkout is NOT used as ROOT itself
    # (dev workflows self-govern via resolution step 2 regardless of ~/.topology).
    SRC_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

    echo "==> Installing topology payload globally at $ROOT (built from checkout)"
    TMPDIR_BUILD="$(mktemp -d)"
    TMPDIR_STAGE_G="$(mktemp -d)"
    cleanup_build_global() { rm -rf "$TMPDIR_BUILD" "$TMPDIR_STAGE_G"; }
    trap cleanup_build_global EXIT
    PAYLOAD_TARBALL="$(cd "$TMPDIR_BUILD" && bash "$SRC_ROOT/scripts/build-payload.sh" "$TMPDIR_STAGE_G")"
    if [[ -e "$ROOT" ]]; then
      _handle_existing_root
    fi
    mkdir -p "$ROOT"
    _unpack_payload "$PAYLOAD_TARBALL"
    cleanup_build_global
    trap - EXIT

  else
    # Piped mode: download the payload tarball from GitHub releases and verify its
    # checksum before unpacking (same machinery as local piped mode — hoisted above;
    # the helper registers the cleanup trap itself).
    _download_and_verify_payload

    if [[ -e "$ROOT" ]]; then
      _handle_existing_root
    fi

    echo "==> Installing topology payload globally at $ROOT (downloaded)"
    mkdir -p "$ROOT"
    _unpack_payload "$TMPDIR_DL/topology-payload.tar.gz"
    cleanup_dl
    trap - EXIT
  fi

else
  # Local scope: vendor the distribution payload at <project>/.topology.
  # The payload is a curated, read-only operator snapshot (hooks, skills, instincts,
  # scan rules, scripts/fetch-gatekeeper.sh, AGENTS.md, VERSION) — no source tree, no
  # docs, no .git. All project state lives under <project>/.claude/topology/ (ADR-0013),
  # so replacing the payload on upgrade cannot delete handoffs or the learn ledger.
  ROOT="$PROJECT_PATH/.topology"

  if [[ -n "${BASH_SOURCE[0]:-}" && -f "${BASH_SOURCE[0]}" ]]; then
    # Dev-checkout mode: build the payload locally so the installed tree is always
    # in sync with the sources being tested (no version/checksum mismatches in CI).
    SRC_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

    echo "==> Vendoring topology payload at $ROOT (built from checkout)"
    TMPDIR_BUILD="$(mktemp -d)"
    TMPDIR_STAGE="$(mktemp -d)"
    cleanup_build() { rm -rf "$TMPDIR_BUILD" "$TMPDIR_STAGE"; }
    trap cleanup_build EXIT
    PAYLOAD_TARBALL="$(cd "$TMPDIR_BUILD" && bash "$SRC_ROOT/scripts/build-payload.sh" "$TMPDIR_STAGE")"
    if [[ -e "$ROOT" ]]; then
      _handle_existing_root
    fi
    mkdir -p "$ROOT"
    _unpack_payload "$PAYLOAD_TARBALL"
    cleanup_build
    trap - EXIT

  else
    # Piped mode: download the payload tarball from GitHub releases and verify its
    # checksum before unpacking (the helper registers the cleanup trap itself).
    _download_and_verify_payload

    if [[ -e "$ROOT" ]]; then
      _handle_existing_root
    fi

    echo "==> Vendoring topology payload at $ROOT (downloaded)"
    mkdir -p "$ROOT"
    _unpack_payload "$TMPDIR_DL/topology-payload.tar.gz"
    cleanup_dl
    trap - EXIT
  fi

  # Append .topology/ to project .gitignore if absent.
  GITIGNORE="$PROJECT_PATH/.gitignore"
  if ! grep -qxF '.topology/' "$GITIGNORE" 2>/dev/null; then
    echo '.topology/' >> "$GITIGNORE"
    echo "==> Appended .topology/ to $GITIGNORE"
  fi
fi

cd "$ROOT"

# ─── 4. Manifest tracking ────────────────────────────────────────────────────

MANIFEST=()
note() { MANIFEST+=("$1"); }

if [[ "$SCOPE" == "local" ]]; then
  note "$ROOT (vendored payload)"
  if grep -qxF '.topology/' "$PROJECT_PATH/.gitignore" 2>/dev/null; then
    note "$PROJECT_PATH/.gitignore (.topology/ entry)"
  fi
fi

# ─── 5. Acquire the binary ───────────────────────────────────────────────────

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

  # Cargo fallback: the payload has no Rust source tree. Piped + --build-from-source
  # is rejected early for both scopes, so we only reach here in checkout mode (where
  # section 2 set SRC_ROOT) when the prebuilt download failed or was skipped.
  if [[ -n "${BASH_SOURCE[0]:-}" && -f "${BASH_SOURCE[0]}" ]]; then
    if ! command -v cargo >/dev/null 2>&1; then
      echo "error: cargo (Rust) not found and prebuilt download failed." >&2
      echo "  Fix one of:" >&2
      echo "    1. Install Rust from https://rustup.rs and re-run." >&2
      echo "    2. Ensure network access for the prebuilt download." >&2
      exit 1
    fi
    ( cd "$SRC_ROOT/gatekeeper" && cargo build --release )
    mkdir -p "$ROOT/bin"
    cp "$SRC_ROOT/gatekeeper/target/release/gatekeeper" "$BIN"
    echo "    built + copied: $BIN"
    note "$BIN"
  else
    # Unreachable belt-and-suspenders: piped + --build-from-source is caught early.
    echo "error: no Rust source tree available in a downloaded payload install." >&2
    echo "  Remedy: clone https://github.com/osxsystem/topology and run" >&2
    echo "    scripts/install.sh [--global|--project <path>] --build-from-source" >&2
    echo "  from inside the checkout." >&2
    exit 1
  fi
fi

if [[ ! -x "$BIN" ]]; then
  echo "error: could not produce a gatekeeper binary via either path." >&2
  echo "  Remedies:" >&2
  echo "    1. Run: bash scripts/install.sh --build-from-source" >&2
  echo "    2. Run: cd gatekeeper && cargo build --release" >&2
  exit 1
fi

# ─── 6. CLAUDE.md → AGENTS.md symlink ────────────────────────────────────────

echo "==> Linking CLAUDE.md -> AGENTS.md"
ln -sf AGENTS.md CLAUDE.md
note "$ROOT/CLAUDE.md"
echo "    $ROOT/CLAUDE.md -> AGENTS.md"

# ─── 7. Mark scripts executable ──────────────────────────────────────────────

echo "==> Marking scripts executable"
chmod +x hooks/*.sh scripts/*.sh
note "hooks/*.sh scripts/*.sh (made executable)"
echo "    done"

# ─── 8. Git pre-commit hook ───────────────────────────────────────────────────

echo "==> Installing the git pre-commit hook"
# The hook must guard the repo the developer COMMITS to. For a --project install that is the
# project repo, NOT the vendored framework at <project>/.topology — installing into the
# payload's own (absent) .git would print success while the project's commits go entirely unscanned.
HOOK_REPO="$ROOT"
if [[ -n "$PROJECT_PATH" ]]; then
  HOOK_REPO="$PROJECT_PATH"
fi
if [[ -d "$HOOK_REPO/.git" ]]; then
  # COPY, do not symlink: the active hook must not be the same mutable worktree file it guards.
  cp "$ROOT/hooks/pre-commit.sh" "$HOOK_REPO/.git/hooks/pre-commit"
  chmod +x "$HOOK_REPO/.git/hooks/pre-commit"
  note "$HOOK_REPO/.git/hooks/pre-commit"
  echo "    copied hooks/pre-commit.sh -> $HOOK_REPO/.git/hooks/pre-commit (stable copy; re-run install to update)"
else
  echo "    (no .git dir at $HOOK_REPO; wire hooks/pre-commit.sh into your VCS manually)"
fi

# ─── 9. Harness wiring ───────────────────────────────────────────────────────

HARNESS=""
if [[ -n "$HARNESS_FLAG" ]]; then
  HARNESS="$HARNESS_FLAG"
elif [[ "$SCOPE" == "local" ]]; then
  # Wiring a project — default to claude.
  if [[ $YES -eq 1 ]] || ! can_prompt; then
    HARNESS="claude"
    echo "assumed: harness=claude (use --harness <h> to override)"
  else
    HARNESS=$(ask "Harness to wire (claude|codex|cursor|opencode|none)" "claude")
  fi
else
  # Global scope — default to none (print the one-liner instead).
  if [[ $YES -eq 1 ]] || ! can_prompt; then
    HARNESS="none"
    echo "assumed: harness=none for global install (use --harness <h> with --project to wire)"
  else
    HARNESS=$(ask "Harness to wire, or 'none' to skip (claude|codex|cursor|opencode|none)" "none")
  fi
fi

case "$HARNESS" in
  claude|codex|cursor|opencode)
    if [[ "$SCOPE" == "local" ]]; then
      echo "==> Wiring harness: $HARNESS"
      if ! WIRING_OUTPUT="$(cd "$PROJECT_PATH" && TOPOLOGY_ROOT="$ROOT" "$BIN" adapt --harness "$HARNESS" 2>&1)"; then
        echo "$WIRING_OUTPUT" >&2
        echo "error: harness wiring failed (adapt --harness $HARNESS)" >&2
        exit 1
      fi
      echo "$WIRING_OUTPUT"
      # Note each generated file in the manifest.
      while IFS= read -r line; do
        case "$line" in
          "wrote "*)
            rel="${line#wrote }"
            note "$PROJECT_PATH/$rel"
            ;;
        esac
      done <<< "$WIRING_OUTPUT"
    else
      # Global scope, no project given — print the command to run later.
      cat <<EOF

==> To wire a project later, run from inside it:
    TOPOLOGY_ROOT="$ROOT" "$BIN" adapt --harness $HARNESS
EOF
    fi
    ;;
  none)
    cat <<EOF

==> Hook config — paste into the PROJECT-LOCAL .claude/settings.json.
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
    ;;
  *)
    echo "warning: unknown harness '$HARNESS'; skipping wiring" >&2
    ;;
esac

# ─── 10. Stale-PATH repair ─────────────────────────────────────────────────────
#
# PROMPT_INPUT_FD test seam: the interactive repair branch reads confirmation
# from this fd (default /dev/tty). CI and tests set it to a file/pipe
# containing the answer so the tty is not required. The production default
# stays /dev/tty — no code path prompts when /dev/tty cannot be opened
# (can_prompt() guards the interactive branch; --yes / non-tty take the
# warning-only path without reading from this fd at all).
: "${PROMPT_INPUT_FD:=/dev/tty}"

repair_stale_path() {
  local new_bin="$1"
  local new_version
  new_version="$("$new_bin" --version 2>/dev/null | awk '{print $2; exit}')"
  local found
  found="$(command -v gatekeeper 2>/dev/null || true)"
  [[ -z "$found" ]] && return 0
  # Skip if the found binary IS the new install.
  local real_found real_new
  real_found="$(realpath "$found" 2>/dev/null || echo "$found")"
  real_new="$(realpath "$new_bin" 2>/dev/null || echo "$new_bin")"
  [[ "$real_found" == "$real_new" ]] && return 0
  local old_version
  old_version="$("$found" --version 2>/dev/null | awk '{print $2; exit}' || true)"
  [[ "$old_version" == "$new_version" ]] && return 0

  if [[ $YES -eq 1 ]] || ! can_prompt; then
    echo ""
    echo "WARNING: stale gatekeeper on PATH"
    echo "  path:        $found"
    echo "  stale:       ${old_version:-unknown}"
    echo "  installed:   $new_version"
    echo "  remedy:      cp \"$new_bin\" \"$found\""
    echo "               or remove $found from PATH"
  else
    printf '\nreplace %s (%s) with %s? [y/N]: ' "$found" "${old_version:-?}" "$new_version" > /dev/tty
    local ans
    read -r ans < "$PROMPT_INPUT_FD" || ans=""
    if [[ "$ans" == "y" || "$ans" == "Y" ]]; then
      cp "$new_bin" "$found"
      note "$found (overwritten with $new_version)"
      echo "    replaced $found"
    else
      echo "    kept $found (${old_version:-unknown}); cp \"$new_bin\" \"$found\" to update manually"
    fi
  fi
}

# ─── 11. Adapt notes (global scope, harness=none) ────────────────────────────

if [[ "$SCOPE" == "global" && "$HARNESS" == "none" ]]; then
  cat <<EOF

==> Optional: generate a harness's native config from this one Markdown source.
    Outputs are build artifacts — re-run to update; add --check to verify they are current (CI-friendly).
    "$BIN" adapt --harness codex      # .codex/config.toml      (AGENTS.md carries the contract)
    "$BIN" adapt --harness cursor     # .cursor/rules/*.mdc      (instincts=Always, skills=Agent Requested)
    "$BIN" adapt --harness opencode   # opencode.json + .opencode/skills/
    "$BIN" adapt --harness claude     # .claude/settings.json    (precise generator of the wiring above)
EOF
fi

# ─── 12. Stale-PATH check ────────────────────────────────────────────────────
# Runs before the summary so an accepted overwrite appears in the printed manifest.

repair_stale_path "$BIN"

# ─── 13. Post-install summary ────────────────────────────────────────────────

echo ""
echo "==> Installed gatekeeper"
"$BIN" --version
echo "    path: $BIN"

echo ""
echo "==> Files created or modified:"
for f in "${MANIFEST[@]}"; do
  echo "    $f"
done

# ─── 13a. Commit hint (local scope only) ─────────────────────────────────────
# The installer writes governance files (.claude/settings.json, .gitignore, ...)
# into the project repo but leaves them uncommitted. The review gate's cleanliness
# check will fail with "uncommitted changes" until they are committed. Detect which
# manifest files are untracked/modified in the project git repo and offer to commit
# them (tty) or print a hint (non-tty / --yes).
#
# Derivation: walk MANIFEST; keep only files that (a) live inside PROJECT_PATH, (b)
# are NOT inside the vendored payload (.topology/), and (c) appear as untracked (??)
# or modified ([ M]) in `git status --porcelain`. That way the hint is always tied
# to what this install actually did — no hardcoded file list.
if [[ "$SCOPE" == "local" && -n "$PROJECT_PATH" ]]; then
  UNCOMMITTED_WIRING=()
  for f in "${MANIFEST[@]}"; do
    # Strip any trailing annotation (e.g. " (.topology/ entry)").
    raw_f="${f%% (*}"
    # Must be inside the project and not inside .topology/.
    [[ "$raw_f" == "$PROJECT_PATH/"* ]] || continue
    rel="${raw_f#"$PROJECT_PATH/"}"
    [[ "$rel" == .topology/* ]] && continue
    # Must exist as a file (not a glob description like "hooks/*.sh").
    [[ -f "$raw_f" ]] || continue
    # Check git status in the project repo.
    status_line="$(git -C "$PROJECT_PATH" status --porcelain -- "$rel" 2>/dev/null || true)"
    [[ -z "$status_line" ]] && continue
    UNCOMMITTED_WIRING+=("$rel")
  done

  if [[ ${#UNCOMMITTED_WIRING[@]} -gt 0 ]]; then
    FILES_LIST="${UNCOMMITTED_WIRING[*]}"
    if [[ $YES -eq 0 ]] && can_prompt; then
      # Interactive — offer to commit; default NO so the user is never surprised.
      echo ""
      echo "==> Uncommitted governance files detected"
      echo "    The installer created/modified the following files in your project repo"
      echo "    but left them uncommitted. The review gate's cleanliness check will fail"
      echo "    until they are committed."
      for rel in "${UNCOMMITTED_WIRING[@]}"; do
        echo "      $rel"
      done
      printf 'Commit them now with "chore: wire topology governance"? [y/N]: ' > /dev/tty
      read -r _commit_ans < /dev/tty || _commit_ans=""
      if [[ "$_commit_ans" == "y" || "$_commit_ans" == "Y" ]]; then
        git -C "$PROJECT_PATH" add -- "${UNCOMMITTED_WIRING[@]}"
        git -C "$PROJECT_PATH" commit -m "chore: wire topology governance"
        echo "    committed."
      else
        echo ""
        echo "    Hint: run this when ready:"
        echo "      git add $FILES_LIST && git commit -m \"chore: wire topology governance\""
      fi
    else
      # Non-interactive (piped install or --yes): print a clearly-worded hint.
      # --yes keeps the default (no commit) and prints the hint.
      echo ""
      echo "==> Action needed: commit the governance wiring files"
      echo "    The installer created/modified these files in your project repo:"
      for rel in "${UNCOMMITTED_WIRING[@]}"; do
        echo "      $rel"
      done
      echo "    Without committing them, the review gate will fail with"
      echo "    \"uncommitted changes outside .claude/topology/reviews/\"."
      echo "    Run:"
      echo "      git add $FILES_LIST && git commit -m \"chore: wire topology governance\""
    fi
  fi
fi

echo ""
echo "==> Health check"
# Run doctor from the PROJECT directory for a --project install: that is the cwd every later
# gatekeeper invocation will use, so probing from inside the framework payload validates the
# wrong layout (and masks broken project wiring). TOPOLOGY_ROOT is passed explicitly so the
# check also holds for binaries that predate vendored-root autodetection.
if [[ -n "$PROJECT_PATH" ]]; then
  (cd "$PROJECT_PATH" && TOPOLOGY_ROOT="$ROOT" "$BIN" doctor)
else
  "$BIN" doctor
fi
