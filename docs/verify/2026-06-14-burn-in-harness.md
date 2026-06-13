# Verify — Phase 15 burn-in harness

- **Date:** 2026-06-14 · **Feature slug:** burn-in-harness
- **Design:** [docs/specs/2026-06-13-burn-in-harness.md](../specs/2026-06-13-burn-in-harness.md) · **Report:** [docs/burn-in/2026-06-14-burn-in-report.md](../burn-in/2026-06-14-burn-in-report.md)

Each acceptance criterion from the design, demonstrated with a command and its actual output.

## AC1 — replay script exists, replays a bounded window into a dedicated truncated log, defers to shadow-stats

`bash scripts/burn-in-replay-tdd.sh 49`:
```
replayed 49 merge(s); 8 produced a replay verdict -> .../docs/logs/burn-in-tdd.jsonl
gate  check   evals  pass  fail  skip  static  would-block%
tdd   replay      8     3     5     0       0         62.5%
```
The dedicated `docs/logs/burn-in-tdd.jsonl` is read by `scripts/shadow-stats.sh` (invoked, not modified — see AC8). ✔

## AC2 — `trap` removes scratch + nested worktrees; no leak

After both the 15- and 49-merge runs: `git worktree list | grep -c burnin` → `0`. No worktree leaked. ✔

## AC3 — gate exit codes captured without aborting under `set -euo pipefail`

The replay completed exit 0 across all 49 merges even though `check tdd` returned non-zero on the 5 would-block merges (`|| true` capture). The run reaching its final `shadow-stats` summary is the proof. ✔

## AC4 — entropy sweep exists, applies `exclude_paths` itself, skips >5 MiB, prints a rate

`bash scripts/burn-in-entropy-sweep.sh`:
```
files scanned:        256
excluded (path glob): 8
skipped (oversize):   0
total lines:          52887
entropy WARN hits:    107
WARN per 10k lines:   20.23
```
8 files excluded by glob (the entropy lane the script applies because `--content` carries no path); a per-10k rate printed. ✔

## AC5 — zero-data → "0 evaluations", exit 0 (both scripts)

`bash scripts/test-burn-in.sh` (`entropy_zero_data` = only an excluded `*.lock`; `replay_zero_data` = a repo with no merges):
```
  entropy_zero_data PASS
  replay_zero_data PASS
```
Both print `0 evaluations` and exit 0. ✔

## AC6 — committed burn-in report with the required fields

[docs/burn-in/2026-06-14-burn-in-report.md](../burn-in/2026-06-14-burn-in-report.md) records, per engine: eval count, would-block count, **vacuous-compile-red note**, the 5 would-blocks traced to source merges (#61/#44/#43/#42/#40), and each number vs its criterion (TDD `8/50`, 62.5% vs `<2%`; entropy `20.23` vs `<1`/10k). It flips nothing. ✔

## AC7 — no protected-path edits; diff is scripts + docs only

`git diff --name-only $(git merge-base main HEAD)..HEAD`:
```
CHANGELOG.md
docs/burn-in/2026-06-14-burn-in-report.md
docs/plans/2026-06-14-burn-in-harness.md
docs/research/2026-06-13-burn-in-harness.md
docs/specs/2026-06-13-burn-in-harness.md
justfile
scripts/burn-in-entropy-sweep.sh
scripts/burn-in-replay-tdd.sh
scripts/test-burn-in.sh
```
No `scan.rs`/`tdd.rs`/`verify.rs`/`rules.toml`/`gatekeeper/`/`hooks/` change. ✔

## AC8 — `shadow-stats.sh` reused, not modified

`git diff --name-only <base>..HEAD -- scripts/shadow-stats.sh gatekeeper/ security/ hooks/` → empty (`untouched (good)`). ✔

## AC9 — idempotent re-run; verdicts carry ts + merge

`replay_idempotent` (run twice): `n1=1, n2=1` (truncate-per-run, no doubling), `has_ts=yes`, `has_merge=yes`. The would-block triage section now renders with source merges. ✔

## AC10 — shellcheck clean; full suite green; no new deps

- `shellcheck hooks/*.sh scripts/*.sh` → clean.
- `cargo test --manifest-path gatekeeper/Cargo.toml` → **553 passed, 0 failed** across 22 binaries (unchanged from the `00174b5` baseline — no Rust was touched).
- No dependency added (`Cargo.toml` untouched). ✔

## The original symptom, reproduced-then-resolved

**Symptom (research gate):** `docs/logs/shadow.jsonl` held 0 TDD-replay and 0 entropy evals — the flip was un-evidenced and unmeasurable. **Resolved:** the harness now produces both numbers on demand (8 TDD evals @ 62.5% would-block; 20.23 entropy WARN/10k), and the report states each against its criterion. The flip remains correctly deferred — on evidence, not on a feeling.
