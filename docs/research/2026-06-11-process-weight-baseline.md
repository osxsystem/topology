# FM1 Process-Weight Baseline

**Status:** captured (pre-Track-3 changes)

## Method

Enumeration follows the method specified in
[`docs/specs/2026-06-11-day-zero-containment.md`](../specs/2026-06-11-day-zero-containment.md)
§4. The script [`scripts/metrics.sh`](../../scripts/metrics.sh) is POSIX sh + awk + git plumbing
with no external dependencies. It walks `main` first-parent since tag `v0.3.0`, emits one CSV row
per merge commit (branch name parsed from the merge subject), and aggregates the four
direct-to-main commits into a single labeled residual row. `production_loc` counts added+deleted
lines from `git diff --numstat <m>^1...<m>^2` excluding `docs/**` and `*.md` files;
`artifact_loc` is the complement of the same diff. `commits` is `git rev-list --count
<m>^1..<m>^2`. `lead_time_hours` is the author-date of the merge minus the author-date of the
earliest branch commit, expressed in hours to one decimal place.

## CSV output

```csv
branch,merge_commit,production_loc,artifact_loc,commits,lead_time_hours
feat/issue-29-project-config,29a338364c040e4e3ffac85587116f4f8ac71f13,949,35,2,0.5
fix/issue-27-subcommand-flags,25ebc9cb5cfec36590e5e96a4a3e5d31c348070b,676,0,3,0.9
fix/issue-28-installer-commit-hint,793a1d23eae97f3f6b890cc22b1115c91a9b9703,114,2,1,0.7
fix/issue-26-aws-secret-scan,9337346254518b9800ddb4283dfa5c39a5f8f4d9,207,23,2,1.0
fix/issue-25-design-approval,1c346c0487fa054a7208712227dfb7057ea9c963,242,28,2,1.2
fix/issue-24-tdd-gate,61868792fbd77bd24741ffb3c29f45f86ef8d648,600,3,2,1.3
fix/issue-23-activate-word-boundary,c75d909e4424790eaf464b5c0b640a568d64b18b,193,0,2,1.5
feat/installer-payload-vendoring,de4850c8ca8d41e884564bee2be9964ee1e1ed64,487,92,6,0.6
fix/governed-install,bc259b740ca7580b28e45fc733d7eb437e6f5f59,314,0,2,0.0
chore/release-v0.3.1,6b2bfb852e2cec69be19bb2fe41dcadccbdd1f62,8,0,1,0.0
feat/distribution-payload,90a2577a39c08b7425a38039c226fdf05231d6b8,2614,985,20,5.6
(direct-to-main),,9,1186,4,
```

## Headline numbers (merge rows only, excluding residual)

**Median commits/branch: 2**

**Aggregate artifact:production LOC ratio: 0.2:1**
(total artifact_loc 1168 / total production_loc 6404)

The roadmap expected approximately 8 commits per branch and approximately 5:1; both actuals are
well below those thresholds, reflecting the short, tightly scoped fix branches typical of this
project's history so far.
