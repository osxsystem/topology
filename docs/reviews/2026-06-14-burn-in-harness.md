VERDICT: pass
HEAD: 6cd52099a5cac86251b41c7cfb0627c8a6ec7caa
BASE: 6cbb7efc523d30531703ad614471d1aaa14b201a

# Review: burn-in-harness (2026-06-14)

Two independent fresh-context critics (no memory of authoring) reviewed the branch. The first, on an earlier HEAD, passed with one fidelity finding (the entropy exclude glob over-excluded nested `tests/fixtures/`); that was fixed test-first (commit mirroring the engine's `glob_match`, scan.rs:498-501) and the entropy figure re-measured (20.23 → 21.80). The second critic reviewed this final HEAD and confirmed the fix and the verdict.

## Blocking findings
None.

## Non-blocking notes
- `scripts/burn-in-replay-tdd.sh:22-26` — the `trap` cleans the scratch worktree and runs `git worktree prune`, but the engine's *nested* `gatekeeper-replay/<feature>-<pid>` temp dir is owned by the engine's RAII Drop; on a hard SIGINT to the child it could orphan that directory. Engine-owned path, not a script defect; AC2's `grep -c burnin = 0` evidence wouldn't detect a `gatekeeper-replay/` orphan.
- `scripts/test-burn-in.sh:104-125` — `test_entropy_fixtures_glob_matches_engine` pins the scanned/excluded counts (which uniquely identify correct glob behavior) but does not separately assert the nested file produced a WARN hit. The count assertion is sufficient; a direct `WARN hits: +1` assertion would be marginally stronger.
- `scripts/burn-in-replay-tdd.sh:58` — injected log lines keep the `SHADOW ` prefix; harmless (shadow-stats.sh greps substrings) but cosmetically diverges from the engine's prefix-free file-sink lines.

## Criteria checked
### Spec/plan
- Replay script: bounded `--limit` window, detached scratch worktree + ephemeral `<wt>/docs/config.toml`, dedicated gitignored `docs/logs/burn-in-tdd.jsonl` truncated per run, defers to `shadow-stats.sh` — satisfied (`burn-in-replay-tdd.sh:13-18,43-48,65`; `.gitignore:18`).
- `trap` removes scratch worktree + prunes on exit/interrupt — satisfied for clean exit (`:22-26`); nested-worktree-on-hard-SIGINT is the engine-owned nonblocking note above.
- Gate exit codes captured without aborting under `set -euo pipefail` — satisfied via `|| true` and `if`-guarded `(( ))` (no bare zero-valued arithmetic abort).
- Entropy sweep applies `exclude_paths` itself (engine-faithful, prefix-anchored `tests/fixtures/`), skips >5 MiB as skip-with-warning, prints per-10k rate — satisfied; the primary fidelity finding is **confirmed fixed** (nested negatives now scanned, matching `--staged`; excluded count = 1).
- Zero-data → "0 evaluations", exit 0 (both scripts) — satisfied; `entropy_zero_data` + `replay_zero_data` pass.
- Committed report with eval count, coverage-loss, vacuous-compile-red note, would-block triage traced to merges, number-vs-criterion; flips nothing — satisfied (`docs/burn-in/2026-06-14-burn-in-report.md`).
- Measurement correctness — confirmed: eval counts only when a `check":"replay"` SHADOW line appears, so the 41 no-test-only merges are excluded → 8 evals; awk rate 116/53208×10000 = 21.80; would-block 5/8 = 62.5%. Numbers internally consistent.
- No protected-path edits; diff is scripts + docs only — satisfied (`git diff --name-only` = 10 files, no `.rs`/`.toml`/rules/hooks).

### Standards
- three-language-lanes — conforms: Bash only tallies (`grep -c`, presence) and divides (awk); all detection/severity/verdict originate in the Rust engines; no engine edits.
- no-new-deps (ADR-0007) — conforms: `Cargo.toml` untouched; scripts use only `git`/`awk`/`sed`/`grep`/`wc`/`mktemp`.
- surgical-changes-only — conforms: 10 files, all the requested harness + its docs; no adjacent refactors; recipe mirrors `test-fetch`.
- no-flip — conforms: no `[tdd] mode` change, no `rules.toml` severity change, no engine edit; `docs/logs/` gitignored; report and verify doc both assert the flip stays deferred. ADR-0017/0018 shadow-then-enforce doctrine honored.
