# Plan: Phase 15 burn-in harness

- **Date:** 2026-06-14
- **Feature slug:** burn-in-harness
- **Design:** docs/specs/2026-06-13-burn-in-harness.md (Status: approved)
- **Baseline:** tests green at commit `00174b5` (`cargo test` full suite OK; `cli_scan_bench`/`cli_tdd_replay` included)

## Files

- `scripts/burn-in-entropy-sweep.sh` — new. Current-tree entropy-WARN-per-10k-lines sweep over `git ls-files`, applying `exclude_paths` itself and skipping >5 MiB files. Pure measurement.
- `scripts/burn-in-replay-tdd.sh` — new. Replays the TDD engine over the repo's historical merge pairs into a dedicated gitignored `docs/logs/burn-in-tdd.jsonl`, then runs `shadow-stats.sh` on it.
- `scripts/test-burn-in.sh` — new. Bash test harness (TDD) for both scripts, using a throwaway git repo + a stub `gatekeeper` injected via `GATEKEEPER_BIN`.
- `justfile` — add a `test-burn-in` recipe (mirrors `test-fetch`).
- `CHANGELOG.md` — Unreleased entry under a new heading.
- `docs/burn-in/2026-06-14-burn-in-report.md` — written in the run step (separate task list item #4), not here.

Log file `docs/logs/burn-in-tdd.jsonl` is auto-gitignored (`.gitignore:18` `docs/logs/`); no `.gitignore` edit needed.

## Tasks

### Task 1: Test harness skeleton + entropy zero-data (RED→GREEN)
- **File(s):** `scripts/test-burn-in.sh` (new), `scripts/burn-in-entropy-sweep.sh` (new).
- **Test first:** write `scripts/test-burn-in.sh` with a pass/fail counter (model on `scripts/test-fetch-version.sh`) and one case `entropy_zero_data`: create a throwaway git repo containing only `a.lock` (an excluded glob) with high-entropy content, `cd` into it, run `bash <repo>/scripts/burn-in-entropy-sweep.sh`, assert stdout contains `0 evaluations` and exit code 0. Run `bash scripts/test-burn-in.sh` → **expect RED** (`burn-in-entropy-sweep.sh: No such file or directory`).
- **Change (implement to green):** create `scripts/burn-in-entropy-sweep.sh` with this exact body:
  ```bash
  #!/usr/bin/env bash
  # burn-in-entropy-sweep.sh — entropy-scanner burn-in (ADR-0018): count entropy WARN
  # hits per 10k scanned lines across the working tree. Pure measurement: never blocks,
  # never edits. Glue over `gatekeeper scan --content`. exclude_paths applied here because
  # --content carries no path (scan.rs), so the figure matches the --staged/--hook lanes.
  set -euo pipefail
  REPO_ROOT="$(git rev-parse --show-toplevel)"
  GATEKEEPER="${GATEKEEPER_BIN:-$REPO_ROOT/gatekeeper/target/release/gatekeeper}"
  CAP_BYTES=$((5 * 1024 * 1024))   # scan --content HOOK_INPUT_CAP (scan.rs:25)
  is_excluded() {
    case "$1" in
      *.lock|*.svg|*.min.js) return 0 ;;
      tests/fixtures/*|*/tests/fixtures/*) return 0 ;;
    esac
    return 1
  }
  warn_hits=0; total_lines=0; scanned=0; skipped_excluded=0; skipped_oversize=0
  while IFS= read -r f; do
    if is_excluded "$f"; then skipped_excluded=$((skipped_excluded+1)); continue; fi
    [ -f "$f" ] || continue
    bytes=$(wc -c < "$f")
    if [ "$bytes" -gt "$CAP_BYTES" ]; then
      echo "skip (oversize >5MiB): $f" >&2; skipped_oversize=$((skipped_oversize+1)); continue
    fi
    lines=$(wc -l < "$f"); total_lines=$((total_lines + lines)); scanned=$((scanned+1))
    rc=0; out="$("$GATEKEEPER" scan --content < "$f" 2>&1)" || rc=$?
    hits=$(printf '%s\n' "$out" | grep -cE '^WARN (hex|base64)-high-entropy:' || true)
    warn_hits=$((warn_hits + hits))
  done < <(git -C "$REPO_ROOT" ls-files)
  if [ "$scanned" -eq 0 ]; then echo "no source files scanned (0 evaluations)"; exit 0; fi
  rate=$(awk -v h="$warn_hits" -v l="$total_lines" 'BEGIN{ if(l==0){print "0.00"} else {printf "%.2f", h/l*10000} }')
  echo "Entropy burn-in sweep"
  echo "  files scanned:        $scanned"
  echo "  excluded (path glob): $skipped_excluded"
  echo "  skipped (oversize):   $skipped_oversize"
  echo "  total lines:          $total_lines"
  echo "  entropy WARN hits:    $warn_hits"
  echo "  WARN per 10k lines:   $rate"
  echo "  criterion (ADR-0018): FP <1 per 10k lines (current-tree proxy for full-history)"
  ```
  `chmod +x scripts/burn-in-entropy-sweep.sh`.
- **Test:** `bash scripts/test-burn-in.sh` → expect `entropy_zero_data PASS` and harness exit 0.
- **Commit:** `test(burn-in): entropy sweep skeleton + zero-data case (TDD)`

### Task 2: Entropy detection + exclusion + rate (RED→GREEN)
- **File(s):** `scripts/test-burn-in.sh`.
- **Test first:** add `entropy_detects_and_excludes`: throwaway repo with `secret.txt` containing the literal 64-hex `0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef`, plus `vendor.lock` containing the same hex. `cd` in, run the sweep with `GATEKEEPER_BIN` = the real release binary. Assert: stdout `files scanned:        1` (the `.lock` excluded), `excluded (path glob): 1`, and `entropy WARN hits:` line shows `1` (the hex run WARNs via `hex-high-entropy`; `--content` has no path so the exclude is applied by the script, not the scanner). Run → **expect RED** only if Task 1's impl miscounts; since impl already exists, this is a verification case — if it passes immediately, note that in the commit (the behaviour was built in Task 1; the test now pins it).
- **Change:** none beyond Task 1 unless the assertion fails; if it fails, fix `is_excluded`/grep in `burn-in-entropy-sweep.sh` until green (no other file touched).
- **Test:** `bash scripts/test-burn-in.sh` → `entropy_detects_and_excludes PASS`.
- **Commit:** `test(burn-in): pin entropy detection + path-exclusion counts`

### Task 3: Replay zero-data (RED→GREEN)
- **File(s):** `scripts/test-burn-in.sh`, `scripts/burn-in-replay-tdd.sh` (new).
- **Test first:** add `replay_zero_data`: throwaway repo with a single non-merge commit (no merges), `cd` in, run `bash <repo>/scripts/burn-in-replay-tdd.sh`. Assert stdout contains `0 evaluations` and exit 0. Run → **expect RED** (`burn-in-replay-tdd.sh: No such file or directory`).
- **Change (implement to green):** create `scripts/burn-in-replay-tdd.sh` with this exact body:
  ```bash
  #!/usr/bin/env bash
  # burn-in-replay-tdd.sh — TDD red-green replay burn-in (ADR-0017): replay the engine over
  # the repo's own historical merge pairs (M^1 = base, M^2 = feature tip), accruing tdd/replay
  # verdicts into a dedicated gitignored log, then summarise via shadow-stats.sh. Flips nothing.
  set -euo pipefail
  REPO_ROOT="$(git rev-parse --show-toplevel)"
  GATEKEEPER="${GATEKEEPER_BIN:-$REPO_ROOT/gatekeeper/target/release/gatekeeper}"
  LOG="$REPO_ROOT/docs/logs/burn-in-tdd.jsonl"
  TEST_CMD="${BURN_IN_TEST_CMD:-cargo test --manifest-path gatekeeper/Cargo.toml --quiet}"
  LIMIT="${1:-15}"
  mkdir -p "$(dirname "$LOG")"; : > "$LOG"   # truncate per run → idempotent
  WT="$(mktemp -d "${TMPDIR:-/tmp}/burnin-wt.XXXXXX")"
  cleanup() {
    git -C "$REPO_ROOT" worktree remove --force "$WT" 2>/dev/null || rm -rf "$WT"
    git -C "$REPO_ROOT" worktree prune 2>/dev/null || true
  }
  trap cleanup EXIT INT TERM
  mapfile -t merges < <(git -C "$REPO_ROOT" log --merges --format='%H' -n "$LIMIT")
  if [ "${#merges[@]}" -eq 0 ]; then echo "no merge commits in window (0 evaluations)"; exit 0; fi
  evals=0
  for m in "${merges[@]}"; do
    p1=$(git -C "$REPO_ROOT" rev-parse "$m^1" 2>/dev/null || true)
    p2=$(git -C "$REPO_ROOT" rev-parse "$m^2" 2>/dev/null || true)
    [ -n "$p1" ] && [ -n "$p2" ] || continue
    short=$(git -C "$REPO_ROOT" rev-parse --short "$m")
    git -C "$REPO_ROOT" worktree remove --force "$WT" 2>/dev/null || true; rm -rf "$WT"
    git -C "$REPO_ROOT" worktree add --quiet --detach "$WT" "$p2"
    mkdir -p "$WT/docs"
    printf 'test_command = "%s"\n' "$TEST_CMD" > "$WT/docs/config.toml"
    rc=0; out="$(cd "$WT" && "$GATEKEEPER" check tdd --feature "$short" --base "$p1" 2>&1)" || rc=$?
    if printf '%s\n' "$out" | grep -E '^SHADOW .*"check":"replay"' >> "$LOG"; then
      evals=$((evals+1))
    fi
  done
  echo "replayed ${#merges[@]} merge(s); $evals produced a replay verdict -> $LOG"
  echo
  "$REPO_ROOT/scripts/shadow-stats.sh" "$LOG"
  ```
  `chmod +x scripts/burn-in-replay-tdd.sh`.
- **Test:** `bash scripts/test-burn-in.sh` → `replay_zero_data PASS`.
- **Commit:** `test(burn-in): replay zero-data case + script (TDD)`

### Task 4: Replay idempotency via stub gatekeeper (RED→GREEN)
- **File(s):** `scripts/test-burn-in.sh`.
- **Test first:** add `replay_idempotent`: build a throwaway repo with one real merge commit (two parents): commit `base.txt` on `main`; branch `feat`, commit `test.txt`; `git checkout main`; `git merge --no-ff feat`. Write a stub `gk.sh` that on args `check tdd ...` prints exactly `SHADOW {"ts":1,"gate":"tdd","check":"replay","configured":"default","artifact":null,"command":"x","result":"pass","detail":"stub"}` and exits 0. Run the replay script twice with `GATEKEEPER_BIN=<stub>`. Assert: after each run, `docs/logs/burn-in-tdd.jsonl` has exactly the same number of `"check":"replay"` lines (truncate-per-run, no doubling), and stdout reports a `replay verdict` count. Run → **expect RED** only if truncation is wrong; impl already truncates (`: > "$LOG"`), so this pins it.
- **Change:** none unless red; fix only `burn-in-replay-tdd.sh` truncation/grep if the assertion fails.
- **Test:** `bash scripts/test-burn-in.sh` → `replay_idempotent PASS`; all four cases pass.
- **Commit:** `test(burn-in): pin replay log idempotency (truncate-per-run)`

### Task 5: justfile recipe + shellcheck-clean
- **File(s):** `justfile`.
- **Change:** after the `test-fetch:` recipe block, add:
  ```
  # Run the Phase 15 burn-in harness self-tests (stubbed gatekeeper; no long replay).
  test-burn-in:
      bash scripts/test-burn-in.sh
  ```
  (Match the existing recipe indentation — a leading tab, as in `test-fetch`.)
- **Test:** `just shell` → expect shellcheck exits 0 over `hooks/*.sh scripts/*.sh` (both new scripts + the test harness clean). Then `just test-burn-in` → all cases PASS, exit 0.
- **Commit:** `chore(burn-in): add just test-burn-in recipe; shellcheck-clean`

### Task 6: CHANGELOG entry
- **File(s):** `CHANGELOG.md`.
- **Change:** add at the top, above `## v0.11.0 — 2026-06-13`:
  ```
  ## Unreleased

  Phase 15 burn-in harness (ROADMAP Phase 15) — measurement only; flips nothing.

  - `scripts/burn-in-replay-tdd.sh`: replays the TDD red-green engine over the repo's own
    historical merge pairs into a dedicated gitignored `docs/logs/burn-in-tdd.jsonl`
    (truncated per run), then summarises via `scripts/shadow-stats.sh`. Produces the
    `N/50 evals, <2% false-block` figure the warn→block flip is gated on (ADR-0017).
  - `scripts/burn-in-entropy-sweep.sh`: counts entropy `WARN` hits per 10k working-tree
    lines (applying `exclude_paths` itself; skips >5 MiB files), the FP proxy for ADR-0018.
  - Neither script changes any default, edits any engine, or blocks; `just test-burn-in`
    covers them with a stubbed `gatekeeper`.
  ```
- **Test:** `git diff --name-only 00174b5 -- ':!docs/logs'` lists only `scripts/` and `docs/`/`CHANGELOG.md`/`justfile` paths (no engine/rules edits).
- **Commit:** `docs(changelog): Phase 15 burn-in harness (Unreleased)`

## After the plan gate

Tasks list item #4 (run the burn-in) and the verify/review/finish gates follow once these six tasks are green. The actual `docs/burn-in/2026-06-14-burn-in-report.md` is written from the real run output, not invented here.
