VERDICT: pass
HEAD: 3f806dd4b6b4e0eb2cd942e1ea08ab3e74619da2
BASE: 7486ab13efa1032d3eb9047e998d994ab45663d5

# Review: day-zero-containment (2026-06-11)

## Blocking findings
None.

## Non-blocking notes
- **Process deviation, on record:** the spec's execution notes reserve two commits for the
  human's own terminal (the `Status:` flip `7b70de0` and the protected-path rules commit
  `c1be336`). Both were executed by the agent at the maintainer's explicit in-session direction,
  with the delegation documented in each commit message. The substance the pattern defends
  (human review + human authorization of the exact content) is satisfied — the maintainer
  approved the spec's exact TOML and all three open-question decisions in session — but Phase
  14's `human-commit` design-gate mode, once enforcing, would flag this shape. Accepted for
  v0.4.1; do not treat as precedent once `[design] approval = "human-commit"` ships.
- **Dogfooding gap (file an issue):** this dev clone has no `.git/hooks/pre-commit`, so the
  protected-path veto in `hooks/pre-commit.sh` never runs for framework-repo commits —
  `c1be336` landed without tripping it even though `scan --check-path security/rules.toml`
  exits 1 (protected). Users get the hook via the installer; the framework repo itself does not.
  Phase 14's release-path enforcement assumes a guard that is currently absent here.
- `scripts/metrics.sh` parses `git diff --numstat` paths as awk `$3`: paths containing spaces or
  rename-detection arrows would misclassify. No such paths exist in this history; acceptable for
  a baseline script, would need hardening before any gate consumes it.
- Baseline actuals invert the roadmap's expectations (median 2 commits/branch vs ≈8; 0.2:1
  artifact:production vs ≈5:1 — the expectations came from the adversarial demo scenario, not
  aggregate history). Phase 3's KPI targets ("8 commits / 5 artifacts → ≤4 / ≤2", "lead time
  −40%") must be recalibrated against this measured denominator or they will be trivially met.
- `bench_positives_meet_phase0_floor` prints `hits` counting any detection (attributed or not)
  while asserting only attributed in-scope detections — display-only divergence, harmless.
- The labeled-assignment rule's `[\x22\x27]?` accepts an unquoted value; `password = correct…`
  without quotes also fires. Wider than the bench exercises, consistent with the rule's warn
  posture.

## Criteria checked

### Spec/plan
- §1 ruleset: committed TOML diff (`c1be336`) matches the spec's three blocks byte-for-byte —
  `jwt-structural` (block, three-segment `eyJ` shape), `openai-key` broadened pattern with id
  retained (approved decision 1), `labeled-secret-assignment` at warn (approved decision 2).
  Severity posture matches the spec's deliberate block/warn split.
- §2 bench: attribution-checked floor, 11 runtime-assembled positives (no secret-shaped literal
  ≥20 chars of the candidate charset in the tree — verified by scanning the branch diff itself),
  6 literal `*.txt` negatives with the count asserted, scratch root copies the *live*
  `security/rules.toml` so the bench cannot drift from the shipped ruleset. Red-then-green
  history confirmed (`4a9dbae` → `c1be336`), tdd gate agrees.
- §3 push protection: enabled and read back via the API; bypass note in USER-GUIDE.
- §4 metrics: first-parent enumeration, docs/`*.md` exclusion rule, labeled residual row, lead
  time from earliest branch commit author date — all per spec; deterministic across runs.
- §5 release mechanics: CHANGELOG.md created with the v0.4.1 entry; 0.4.1 across all three
  version-guard manifests + Cargo.lock; payload tarball packs `security/rules.toml`
  (scripts/build-payload.sh, re-confirmed on this branch).
- Non-goals respected: no `scan.rs`/`main.rs` changes (diff touches no Rust source, only a test),
  schema_version stays 1, no allowlist entries added, env-var indirection untouched.

### Standards
- Commit messages follow repo convention (type-prefixed, body explains why); red commit message
  states the by-design failure and its green condition.
- Test code matches the harness idiom of `cli_scan.rs` (EPIPE tolerance, `CARGO_BIN_EXE_`,
  scratch roots keyed by pid); fixtures documented by a corpus README contract.
- Docs gate (`check docs: ok`) green after CHANGELOG + USER-GUIDE additions; full suite
  358 passed / 2 ignored, no new warnings introduced by the branch (test-only Rust changes).
