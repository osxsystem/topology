# Spec — day-zero containment (scan-rule patch, v0.4.1)

**Status:** draft

## Goal

Close the three demonstrated FM5 scan gaps — bearer JWTs, segmented `sk-…` provider keys, and
credential-labeled assignments — with a **rules-only payload patch**; seed the secrets benchmark
that turns scanner coverage into a measured, regression-guarded number; enable GitHub push
protection; and capture the process-weight baseline CSV that every later phase's FM1 KPI divides
by. Ships as `v0.4.1` with zero binary changes.

Grounding: [research](../research/2026-06-11-day-zero-containment.md), the
[remediation roadmap](../plans/2026-06-11-five-failure-modes-roadmap.md) (Phase 0), ROADMAP
Phase 13.

## Non-goals

- **No entropy rules and no schema bump** — `schema_version` stays 1 (`warn` severity is already
  valid); unlabeled high-entropy classes (64-hex, base64-40) stay undetected until Phase 2's
  measured burn-in. The bench records them as in-corpus, out-of-scope.
- **No Rust changes** — `scan.rs`/`main.rs` untouched; the bench is test-only code.
- **No allowlist additions** — the empirical sweep found 0 matches for all three patterns on
  this tree; if a future doc trips one, that doc is wrong (use ellipses), not the ruleset.
- **No secret-shaped literals committed** — positives are runtime-assembled (research,
  §bench-corpus design facts); push protection compatibility is a design input, not an
  afterthought.
- Env-var indirection (`$OPENAI_API_KEY`) remains out of scope — threat-model boundary,
  documented in `security/README` wording in Phase 2, not here.

## Design

### 1. Ruleset changes (`security/rules.toml`, schema 1)

Two additions, one pattern replacement — exact TOML:

```toml
[[rule]]
id = "jwt-structural"
kind = "content"
severity = "block"
description = "JWT — three dot-joined base64url segments with the eyJ sigil"
pattern = '\beyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\b'
```

`openai-key` keeps its id (allowlist references stay stable) and broadens its pattern; the
description gains the segmented shape. Underscores are in both classes because real Anthropic
tails carry them (bench class `anthropic-sk-ant` pins this); the leading `\b` keeps hyphenated
prose ("task-", "risk-") out — a word character precedes that `sk`, so no boundary forms:

```toml
pattern = '\bsk-(?:[A-Za-z0-9_]+-)*[A-Za-z0-9_]{20,}\b'
```

```toml
[[rule]]
id = "labeled-secret-assignment"
kind = "content"
severity = "warn"
description = "credential-labeled assignment (api key / auth token / password / bearer)"
pattern = '(?i)\b(?:api[_-]?key|auth[_-]?token|password|passwd|bearer)\s*[=:]\s*[\x22\x27]?[A-Za-z0-9+/=_-]{16,}'
```

Severity posture (deliberate): the two value-shape rules **block** — high precision, they veto
agent tool calls via the hook path. The label heuristic **warns** — visible at pre-commit and in
CI but dropped on the hook path (scan.rs `emit_decision`), so a false positive can never deadlock
an agent; promotion to block is a Phase 2 decision made on bench data, not optimism.

### 2. Secrets bench (red now, green at v0.4.1, ratchet at v0.6.0)

- **`gatekeeper/tests/cli_scan_bench.rs`** — two tests:
  - `bench_positives_meet_phase0_floor`: runs each of the eleven positive classes through
    `gatekeeper scan --content` against a scratch root holding a **copy of the live
    `security/rules.toml`**; a class is *detected* iff exit == 1 or any `BLOCK `/`WARN ` finding
    line appears on stderr, and an in-scope class passes only when one of its **expected rule
    ids** appears in those lines — attribution is asserted, so a lucky overlap from the wrong
    rule cannot satisfy the floor. (The expected ids encode the recommended answer to open
    question 1; a rename is a one-token bench edit.) Asserts all **9 in-scope** classes; prints
    the full 11-row scoreboard with fired rule ids in the failure message. **Red today**:
    exactly `{jwt-bearer, openai-sk-proj, anthropic-sk-ant, password-labeled}` are missed
    (5/11). Green when §1 lands (9/11).
  - `bench_negatives_stay_clean`: the six literal negative fixtures (`*.txt` only, so a stray
    `.DS_Store` cannot break the count) produce **zero findings and exit 0** — green today and a
    standing guard that §1 (and every future rule) adds no false positives on realistic
    non-secret content, including the placeholder-credential file that canaries the labeled
    rule's FP surface directly.
- **Corpus** — positives runtime-assembled (no matchable literal in the tree; no secret-shaped
  literal ≥20 chars of `[A-Za-z0-9+/=_-]`): JWT bearer header, `sk-proj-…` env assignment,
  `sk-ant-…` with underscore-bearing tail *(added at review)*, `ghp_…`, `xoxb-…`, PEM block,
  unlabeled 64-hex *(out of scope until Phase 2)*, unlabeled base64-44 *(idem)*,
  `password = "…"`, AWS id+secret pair, GCP service-account JSON. Negatives as files under
  `gatekeeper/tests/fixtures/secrets-bench/negatives/`: Cargo.lock excerpt, SVG path data, git
  log OIDs, UUID config, RFC-4648-style base64 test vector, placeholder credentials *(added at
  review — FP canary for the labeled rule)*. Contract documented in
  `tests/fixtures/secrets-bench/README.md`.
- **Ratchet:** the in-scope floor is 9/11 at v0.4.1. Phase 2 flips the two entropy classes in
  scope (≥10/11) **and must move the negatives to a path-aware scan lane**: three negatives are
  entropy-positive shapes by design (lock checksums, git OIDs, base64 vectors), and `--content`
  carries no path, so the planned path excludes cannot protect them through stdin. The corpus
  files themselves do not change; the harness lane does.

### 3. Push protection (human, one command)

```bash
gh api -X PATCH repos/osxsystem/topology \
  -f 'security_and_analysis[secret_scanning_push_protection][status]=enabled'
gh api repos/osxsystem/topology --jq '.security_and_analysis'   # verify: "enabled"
```

`docs/USER-GUIDE.md` gains a three-line note on the inline bypass-URL flow for a blocked
legitimate push. Independent of the branch; can run any time.

### 4. Baseline metrics (`scripts/metrics.sh` → research note)

Read-only git plumbing (POSIX sh + awk, no dependencies), with the method pinned so the CSV is
reproducible and the Phase 3 KPI it feeds is falsifiable:

- **Enumeration:** walk `main` first-parent since `v0.3.0`; every merge commit is one branch row
  (branch name parsed from the merge subject). **Direct-to-main commits** (the history has them)
  are aggregated into one labeled residual row — counted, never silently dropped — so the rows
  sum to the whole delta since the tag.
- **Columns:** `branch, merge_commit, production_loc, artifact_loc, commits, lead_time_hours`.
- **Counting:** `production_loc` = added + deleted lines from `git diff --numstat <m>^1...<m>^2`
  excluding `docs/**` and `*.md`; `artifact_loc` = the complement of the same diff; `commits` =
  `git rev-list --count <m>^1..<m>^2`.
- **Lead time:** author date of the earliest branch-only commit → author date of the merge
  commit, in hours.

Output committed as `docs/research/2026-06-11-process-weight-baseline.md` with the median
commits/branch and artifact:production ratio stated — the FM1 denominator, captured **before**
any Track 3 change moves the numbers.

### 5. Release mechanics

CHANGELOG entry (three rules = the payload change, bench = the guard), tag `v0.4.1`. Rules ship
via the payload tarball (`scripts/build-payload.sh:95–97` packs `security/rules.toml`; verified
2026-06-11), so consumers get them by reinstall with no binary rebuild; the tag also finally
ships the stranded `39710a0` usage-text fix.

## Execution notes (commit order on `feat/day-zero-containment`)

1. Docs commit: this spec + the research note (normal commit).
2. **Red commit**: bench test + fixtures + corpus README — touches only `gatekeeper/tests/`
   (unprotected; normal commit). Branch CI is red between this commit and the next; the branch
   merges green.
3. **Green commit — human action**: the `security/rules.toml` edit stages a protected path, so
   the pre-commit gate aborts it by design; you commit it at your terminal with
   `git commit --no-verify` (hooks/pre-commit.sh:38). The agent is rule-blocked from that flag —
   correctly.
4. Metrics script + baseline note; CHANGELOG + USER-GUIDE note; merge; tag.

Spec approval = you flip **Status: draft → approved** in your own commit — incidentally the exact
human-commit pattern Phase 1 will codify for the design gate.

## Acceptance criteria

1. `cargo test --test cli_scan_bench` **red before** §1 (missing exactly the four new-rule
   classes), **green after**; the branch history shows red-then-green order.
2. Scoreboard at v0.4.1: all 9 in-scope classes detected **by their expected rule ids** —
   `jwt-structural` on the bearer class, broadened `openai-key` on both segmented-sk classes,
   `labeled-secret-assignment` on the password class — and the five v0.4.0 classes still
   attributed to their existing rules (no regression). Machine-checked by the bench's
   attribution assert.
3. Negatives: 0/6 flagged, before and after.
4. Repo-wide: `scan --staged` stays green for every non-protected commit on the branch — the new
   rules fire on zero repo content outside the bench's runtime-assembled payloads.
5. Push protection status reads `"enabled"` via the API, and `docs/USER-GUIDE.md` contains the
   bypass-flow note from §3.
6. Baseline CSV committed covering every first-parent merge since `v0.3.0` **plus** the
   direct-to-main residual row, using exactly the §4 method; median commits/branch and
   artifact:production ratio stated.
7. Full existing suite stays green; `v0.4.1` tagged with the CHANGELOG entry.

## Risks & rollback

- **JWT rule FP** (a real `eyJ…`-shaped triple in some future doc): three-segment structure makes
  this rare; remedy is a span-scoped `[[allow]]` with `reason`, or ellipsize the doc. Rollback of
  any rule = delete it, tag `v0.4.2` — payload-only, minutes.
- **Label-rule FP noise**: bounded by `warn` (never blocks an agent, never fails a scan exit);
  if pre-commit noise annoys in practice, the severity is one line to change. Promotion to
  `block` only on Phase 2 bench evidence.
- **Push protection blocks a legitimate push**: GitHub's inline bypass URL; documented in the
  USER-GUIDE note. Per-push, not a wall.
- **Bench rot**: the corpus is the regression wall — `bench_negatives_stay_clean` fails the
  moment any future rule over-matches realistic content, on every CI run.

## Decisions (resolved at approval, 2026-06-11)

1. **Rule id stays `openai-key`** for the broadened pattern — preserves rule-id stability and
   avoids allowlist/report churn; the bench's expected-id table needs no edit.
2. **`labeled-secret-assignment` ships at `warn`** — the rule is a heuristic, so `warn` gives
   visibility without risking agent deadlock; promotion to `block` is a Phase 2 decision made
   on bench data.
3. **9/11 floor confirmed** — the two entropy classes defer to Phase 2 because entropy detection
   needs schema/path-aware handling and a measured FP burn-in before it can block.
