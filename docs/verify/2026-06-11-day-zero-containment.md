# Verify — day-zero containment

**Feature:** day-zero-containment
**Date:** 2026-06-11
**Spec:** [docs/specs/2026-06-11-day-zero-containment.md](../specs/2026-06-11-day-zero-containment.md)
**Verified by:** main-loop agent (Fable 5), re-running every acceptance criterion at the branch
head (`c1be336`) after the rules landed — the red evidence is the captured pre-rules run, not a
reconstruction.

## AC-1 — bench red before §1, green after, red-then-green history

Red (rules absent, captured before `c1be336`):

```
secrets-bench floor not met: 5/11 detected, in-scope misses:
  jwt-bearer (expected rule: ["jwt-structural"])
  openai-sk-proj (expected rule: ["openai-key"])
  anthropic-sk-ant (expected rule: ["openai-key"])
  password-labeled (expected rule: ["labeled-secret-assignment"])
```

Exactly the four new-rule classes, none other. Green at head:

```
$ cargo test --test cli_scan_bench
test result: ok. 2 passed
```

History order: `4a9dbae` (bench, red) precedes `c1be336` (rules, green) on the branch —
red-then-green confirmed by `git log`; the `tdd` gate independently agrees (AC-7 block).

## AC-2 — 9/11 in-scope by expected rule ids, no v0.4.0 regression

Machine-checked by `bench_positives_meet_phase0_floor`'s attribution assert (a class passes only
when one of its *expected* rule ids fired). The red-run scoreboard doubles as the regression
proof: the five v0.4.0 classes were already attributed to their existing rules —
`github-pat via github-token`, `slack-bot via slack-token`, `pem-private-key via
private-key-block`, `aws-key-pair via aws-access-key-id,aws-secret-access-key`,
`gcp-service-account via gcp-service-account` — and the same assert holds in the green run.
The two entropy classes (`hex64-unlabeled`, `base64-unlabeled`) remain `phase-2`/missed by
design (approved decision 3: 9/11 floor).

## AC-3 — negatives 0/6, before and after

`bench_negatives_stay_clean` passed in the pre-rules run (`1 passed` alongside the red positive
test) and at head. All six `*.txt` fixtures — Cargo.lock excerpt, SVG path data, git OIDs, UUID
config, base64 test vector, placeholder credentials (the labeled rule's FP canary) — produce
exit 0 and zero findings under the new ruleset.

## AC-4 — new rules fire on zero repo content

```
$ git diff main...HEAD | gatekeeper scan --content
exit 0, no findings
```

The whole branch diff (all seven pre-artifact commits, including `security/rules.toml`'s own
regex source and the bench's assembled fragments) scans clean. **Disclosed gap:** the per-commit
`scan --staged` enforcement the criterion assumes did not run live, because this dev clone has
no `.git/hooks/pre-commit` installed — `scan --check-path security/rules.toml` confirms the
protected-path veto *would* apply had the hook been present. Filed as a dogfooding gap
(see review note); the whole-diff scan above is the substance of the criterion.

## AC-5 — push protection enabled + bypass note

```
$ gh api repos/osxsystem/topology --jq '.security_and_analysis'
{"secret_scanning":{"status":"enabled"},
 "secret_scanning_push_protection":{"status":"enabled"}, …}
```

`docs/USER-GUIDE.md` (Security scanning section) carries the bypass-URL note: per-push, never a
standing exemption.

## AC-6 — baseline CSV, method §4, reproducible

`docs/research/2026-06-11-process-weight-baseline.md` covers all 11 first-parent merges since
`v0.3.0` plus the labeled `(direct-to-main)` residual row (4 commits). Headlines: median
**2 commits/branch**, aggregate artifact:production **0.2:1** (1168/6404) — both stated in the
note, with the divergence from the roadmap's demo-scenario expectation (≈8, ≈5:1) recorded
verbatim. Reproducibility: two consecutive `sh scripts/metrics.sh` runs diff byte-identical to
each other and to the CSV embedded in the note; spot-check
`git rev-list --count bc259b7^1..bc259b7^2` = 2 matches its row.

## AC-7 — gates and suite at head

```
PASS research gate · PASS design gate · PASS plan gate
PASS tdd gate: failing-test-first history confirmed
check docs: ok
$ cargo test            →  358 passed, 2 ignored (12 suites)
```

Version manifests agree at 0.4.1 (Cargo.toml, Cargo.lock, plugin.json, marketplace.json —
the `version-guard` release job asserts the same trio at the tag). The `v0.4.1` tag itself and
the CHANGELOG-bearing release execute post-merge; the tag also ships the stranded `39710a0`
usage-text fix per spec §5.
