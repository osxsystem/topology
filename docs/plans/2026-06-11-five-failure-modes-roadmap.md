# Operationalizing the Five Failure Modes Fix — Implementation Roadmap

- **Date:** 2026-06-11
- **Status:** draft
- **Scope:** program-level plan; maps to ROADMAP.md phases 13–17
- **Provenance:** adversarial audit of v0.4.0 (five failure modes) → remediation brief → this execution plan

**Target repo:** `osxsystem/topology` · **Substrate:** Rust `gatekeeper` (main.rs 1,354 LOC, hand-rolled dispatch, 80+ tests across 10 integration files), shell hooks (`hooks.json`: UserPromptSubmit + PreToolUse), TOML rules engine (`scan.rs`, 2,175 LOC, `schema_version = 1`), CI (`ci.yml` offline gate + `release.yml` 4-target build, payload tarball).

**The five failure modes:**

1. **FM1** — constant process weight regardless of change size
2. **FM2** — gates verify existence/sequence, not substance; hollow artifacts pass
3. **FM3** — docs promise more than the binary ships (usage/README drift)
4. **FM4** — naive keyword routing misses semantically relevant prompts
5. **FM5** — secret scan is prefix/label-anchored; unlabeled secret values pass

## Operating Doctrine (applies to every phase)

- **Shadow-then-enforce.** Every behavior-changing gate ships behind a config key with the *current* behavior as default. It runs in shadow mode (logs verdict, never blocks) on this repo's own branches for one minor version, then the default flips. This is the only way a self-governing framework can harden itself without deadlocking its own delivery pipeline.
- **Dogfood the ceremony.** Each work item below is a topology-governed branch (research → spec → plan → tdd → verify → review), built in sibling `topology-phaseN` worktrees, coding delegated to subagents, merged branches deleted local+remote. The hollow-artifact fixtures created in Phase 1 are the acceptance tests for everything after.
- **Honor ADR-0007 or supersede it explicitly.** Where the industry tool is a dependency (clap, gitleaks), the plan substitutes a constraint-compatible mechanism and records the trade-off as a new ADR. Dev-dependencies (test-only) are out of ADR-0007's scope — they never ship in the binary.
- **Release mapping:** Phase 0 → `v0.4.1` · Phase 1 → `v0.5.0` · Phase 2 → `v0.6.0` · Phase 3 → `v0.7.0` · Phase 4 → `v0.8.0`. ROADMAP.md gains phases 13–17 mirroring this document (keeps `check docs` R3 green).

---

## Phase 0 — Day-Zero Containment & Baseline (ops + rules patch, no binary changes)

**Effort:** ~0.5–1 day · **Release:** `v0.4.1` (payload-only patch)

### 1. Core Objectives

Stop the demonstrated bleeding on **FM5 (scan gaps)** with pure `rules.toml` additions; enable the free host-side backstop; capture **baseline metrics** so every later KPI has a denominator. No architecture, no risk.

### 2. Step-by-Step Execution

1. **Add structural JWT rule** to `security/rules.toml` (schema 1, `kind = "content"`, `severity = "block"`): pattern `\beyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\b` (`eyJ` = base64url `{"` sigil; three-segment shape). This alone kills the bearer-token-in-verify-artifact leak from the demonstration.
2. **Fix the `sk-proj` evasion**: replace `openai-key` pattern with `\bsk-(?:[A-Za-z0-9]+-)*[A-Za-z0-9]{20,}\b` (tolerates hyphenated segment prefixes like `proj-`, `ant-`).
3. **Add a labeled-assignment generic rule**: `(?i)\b(?:api[_-]?key|auth[_-]?token|password|passwd|bearer)\s*[=:]\s*[\x22\x27]?[A-Za-z0-9+/=_-]{16,}` at `severity = "warn"` (assignment-anchored, so FP-bounded; entropy comes in Phase 2).
4. **Seed the secrets benchmark corpus**: `gatekeeper/tests/fixtures/secrets-bench/` with 10 synthetic positives (JWT, `sk-proj-…`, `ghp_…`, `xoxb-…`, PEM, unlabeled 64-hex, unlabeled base64-40, labeled password, AWS pair, GCP JSON) and 5 negatives (Cargo.lock excerpt, SVG path data, git OIDs, UUID, base64 test vector). Add `tests/cli_scan_bench.rs` asserting current detection — **expected to show ~5/10**; this red test is the Phase 2 acceptance harness.
5. **Enable GitHub push protection**: `gh api -X PATCH repos/osxsystem/topology -f 'security_and_analysis[secret_scanning_push_protection][status]=enabled'`. Verify with `gh api repos/osxsystem/topology --jq '.security_and_analysis'`.
6. **Baseline metrics script** `scripts/metrics.sh`: for each merged branch since `v0.3.0`, emit CSV of production LOC (non-`docs/`, non-`.md` diff), artifact LOC, commit count, lead time (first commit → merge). Commit the baseline CSV to `docs/research/2026-06-11-process-weight-baseline.md`. This is the FM1 denominator — it must precede any tiering work.

### 3. Dependencies & Prerequisites

None. Deliberately so — this phase is executable immediately.

### 4. Risk Mitigation

- **JWT rule FPs** (base64 strings starting `eyJ` in fixtures/docs): mitigated by the three-segment shape requirement; residual FPs get span-scoped `[[allow]]` entries with `reason`. Rollback = delete rule, cut `v0.4.2`; rules ship via payload, so rollback is a tag away with zero binary work.
- **Push protection blocking a legitimate push**: GitHub provides an inline bypass URL flow; document it in `docs/USER-GUIDE.md`. No rollback needed — it's a per-push bypass, not a hard wall.

### 5. Success Metrics (KPIs)

- Seeded corpus detection: **5/10 → 8/10** (JWT + sk-proj + labeled-assignment classes now caught; entropy classes still open, by design).
- Push protection: `security_and_analysis.secret_scanning_push_protection.status == "enabled"` (boolean, auditable via API).
- Baseline CSV committed: median commits/branch and artifact-to-production LOC ratio recorded (expect ≈8 and ≈5:1 per the demonstration scenario) — these numbers are what Phase 3 must beat.

---

## Phase 1 — Foundations: Cheap Hollow-Pass Kills + Drift-Proof CLI Surface

**Effort:** ~3–5 days · **Release:** `v0.5.0` · **Neutralizes:** FM3 fully; FM2 partially (verify, design, finish gates)

### 1. Core Objectives

Eliminate **FM3 (doc/binary drift)** structurally, and close the three cheapest **FM2** holes — the ones requiring no git-worktree machinery. Build the **hollow-artifact red-team suite** that defines "done" for FM2 across Phases 1–2.

### 2. Step-by-Step Execution

1. **Hollow fixtures first (red):** new `gatekeeper/tests/cli_hollow.rs` building tempdir git repos (reuse the harness pattern from `cli_check.rs`) with seven adversarial fixtures: (a) spec containing only `Status: approved`; (b) empty verify file; (c) `assert!(true)` test-only commit before production commit; (d) review with `### Standard` / `Looks fine.`; (e) `test_command = "true"`; (f) plan that dodges the six-phrase denylist with synonyms; (g) zero-tests-executed finish run. Assert each **fails** its gate. Initially `#[ignore]`-tagged per fixture; un-ignore as each fix lands. This suite is the FM2 scoreboard.
2. **Single dispatch table (FM3, ADR-0014):** refactor `main.rs:64–108` — replace the hand-rolled match and the nine `USAGE_*` constants with one `static SUBCOMMANDS: &[SubcommandSpec]` (`name`, `usage`, `synopsis`, `handler: fn(&[String]) -> i32`). Both the dispatcher and `print_help()` iterate the same table; drift becomes unrepresentable. **No clap** — zero new runtime deps, ADR-0007 intact; record the "table over clap" decision as ADR-0014. Preserve the 0/1/2 exit-code contract; `cli_help_flags.rs` (360 LOC) is the characterization safety net — run it before and after, byte-identical output required except the corrected `check` usage.
3. **README↔binary sync test (FM3):** new `tests/cli_doc_sync.rs` — spawns `gatekeeper --help` and `gatekeeper check --help` via `std::process::Command` (zero new deps; `trycmd` as a dev-dep is the sanctioned upgrade path if golden-file ergonomics warrant it), extracts the gate list, and diffs against the gate table parsed from `README.md` and `docs/USER-GUIDE.md`. The v0.4.0 escape (`39710a0` missing the tag) becomes a CI failure class.
4. **Release-path enforcement (FM3):** add the doc-sync test to the `version-guard` job in `release.yml` — drift is now caught *at the tag*, where it escaped last time.
5. **Verify gate — evidence replay (FM2):** extend `check verify` (`main.rs` gate dispatch ~467–583): parse fenced blocks tagged ` ```evidence ` containing `$ <command>` lines and optional `# expect: <regex>` lines; execute each command from project root; require exit 0 and regex match. Config: `[verify] mode = "presence" | "replay"` (default `presence` for one version), `replay_timeout_secs = 300`, and `allowed_command_prefixes = ["cargo ", "just ", "git "]` — **fail-closed on non-allowlisted commands** (an artifact must not become an arbitrary-execution vector; this is the same fail-closed posture as `security-scan.sh`). The existing artifact format (`docs/verify/2026-06-06-security-scanning.md` already embeds commands + output) means this codifies practice, not invents it.
6. **Design gate — out-of-band approval (FM2):** extend `check design`: `[design] approval = "status-line" | "human-commit"` (default `status-line` for one version). In `human-commit` mode, locate the commit that last modified the `Status:` line (`git log -L` on the spec path) and require its trailer set to **exclude** `Co-Authored-By: Claude` — i.e., a human flipped draft→approved in their own commit. Document the residual risk honestly: this defends against sycophantic self-approval (the actual threat model), not against a malicious operator forging authorship.
7. **Finish gate — zero-test floor (FM2):** capture the test command's stdout/stderr; parse runner summaries (`cargo test`: `(\d+) passed`; `pytest`: `(\d+) passed`; `go test`/`jest` equivalents); fail if executed-test count is 0 or summary is unrecognized **and** `[finish] require_test_count = true` (default false for one version). `test_command = "true"` and `cargo check` stop counting as verification.

### 3. Dependencies & Prerequisites

- Phase 0's fixtures/metrics (the hollow suite extends the same fixture idiom).
- **Decision required before step 2:** confirm ADR-0014 (table over clap). Everything else in this phase is independent and parallelizable across worktrees — steps 5/6/7 touch disjoint gate functions.

### 4. Risk Mitigation

- **Dispatch refactor regressions:** characterization tests (`cli_help_flags.rs`) run first; the refactor PR must show zero behavioral diff. Rollback = `git revert` of one commit; no config involved.
- **Evidence replay executes a hostile command:** the allowlist-prefix fail-closed design caps blast radius; replay also runs *after* the PreToolUse scan layer in agent flows. Kill switch: `[verify] mode = "presence"`.
- **`git log -L` portability** (older git): guard with a capability probe in `doctor`; fall back to `status-line` mode with a stderr warning rather than hard-failing.
- **Self-deadlock:** all three gate hardenings default to old behavior; this repo's own Phase 2 branches run them in shadow (`GATEKEEPER_SHADOW=1` env → log-only) before the v0.6.0 default flip.

### 5. Success Metrics (KPIs)

- Hollow suite: fixtures (a), (b), (e), (g) **rejected** (4/7 un-ignored and green); (c), (d), (f) remain for Phase 2/4.
- `grep -c 'pub const USAGE' gatekeeper/src/main.rs` → **0** (all usage text derived from the table).
- Doc-sync test in both `ci.yml` and `release.yml`; doc/binary escapes at tag time since v0.5.0: **0** (vs 1 at v0.4.0).
- Evidence replay shadow log over this repo's own branches: ≥ **90%** of existing verify artifacts replay green without edits (proves the format codification matched practice).

---

## Phase 2 — Substance Engines: Replay TDD, Entropy Scanning, Path-Triggered Routing

**Effort:** ~6–10 days · **Release:** `v0.6.0` · **Neutralizes:** FM2 core, FM4, FM5 fully (in-scope classes)

### 1. Core Objectives

The three mechanically deep fixes: **red-green replay** (FM2's center of gravity), the **entropy + synced-ruleset scanner** (FM5's class fix, not instance fix), and **artifact-based routing + a measured router** (FM4). Also: flip Phase 1's shadow defaults to enforcing.

### 2. Step-by-Step Execution

1. **Flip Phase 1 defaults:** `[verify] mode = "replay"`, `[design] approval = "human-commit"`, `[finish] require_test_count = true`. One-line config migration note in CHANGELOG; old values remain valid opt-outs per project.
2. **TDD red-green replay (FM2, ADR-0016):** in `tdd.rs`, add `mode = "history" | "replay"` under `[tdd]`. Replay algorithm: identify merge-base `B` and first test-only commit `T` (classifier already exists, `tdd.rs:24–103`); `git worktree add <tmp> B`; `git -C <tmp> checkout T -- <test paths from T's diff>`; run `test_command` with `replay_timeout_secs` — **require nonzero exit (red at base)**; HEAD-green is already the finish gate's job. Cleanup worktree unconditionally (guard against leaks). Document the known soft spot: a test that's red-at-base via compile error (referencing a not-yet-existing API) passes replay while asserting nothing — that residual is Phase 4's mutation-testing target. `assert!(true)` choreography, the demonstrated hole, dies here (it's green at base → gate fails).
3. **Entropy rule kind (FM5, schema v2):** bump `security/rules.toml` to `schema_version = 2`; teach `version.rs`/`scan.rs` to accept 1 and 2. New `kind = "entropy"` rule fields: `charset = "base64" | "hex"`, `min_length`, `threshold_bits_per_char` (defaults from the detect-secrets lineage: base64 ≥ 4.5, hex ≥ 3.0). Implementation in `scan.rs`: tokenize candidate runs `[A-Za-z0-9+/=_-]{20,}`, compute Shannon entropy per token, flag spans over threshold. Ship at `severity = "warn"` (shadow) with a `[scan] exclude_paths` config (`*.lock`, `*.svg`, `*.min.js`, `tests/fixtures/`) — measure FP rate for one burn-in cycle on this repo (scan full history via a blob walk), then promote to `block`.
4. **Maintained-ruleset sync (FM5):** `scripts/sync-gitleaks-rules.sh` — fetch gitleaks' default `gitleaks.toml`, translate a curated subset (provider-prefix rules absent from ours) into `rules.toml` schema with a provenance header (`# synced-from: gitleaks@<sha>`), open a diff for human review (never auto-merge a scanner change — `rules.toml` is itself a protected path). Run quarterly via a scheduled workflow. This is the constraint-compatible substitute for vendoring gitleaks: maintained coverage, zero runtime deps, ADR-0015 records it.
5. **Path-triggered routing (FM4):** add `pathTriggers: {"globs": [...]}` per skill in `hooks/skill-rules.json` (e.g. `security-scanning`: `["**/auth/**", "**/logging/**", "security/**", "**/*secret*"]`). Two deterministic enforcement points: (a) **PostToolUse hook** (new `hooks.json` entry, matcher `Write|Edit|MultiEdit`) → `gatekeeper route --paths <file>` → emits `additionalContext`: *"path matches security-sensitive trigger: security-scanning skill is required before finish"*; (b) **pre-commit**: `pre-commit.sh` additionally calls `gatekeeper route --staged-paths`, printing required-skill reminders next to the existing scan verdict. Security routing now keys on what the diff *touches* — prompt phrasing becomes irrelevant for `require`-level security skills.
6. **Router eval harness (FM4):** `gatekeeper/tests/fixtures/routing-eval.jsonl` — ≥50 labeled prompts (prompt → expected skill set), seeded with the demonstrated misses ("mask bearer tokens…", "logs are leaking Authorization headers") plus paraphrase clusters per skill. New `tests/cli_routing_eval.rs` computes per-skill precision/recall and **fails CI under threshold** (recall ≥ 0.90 for `require` skills, precision ≥ 0.80 overall). Keyword-list edits now move a measured number, not an anecdote. An embedding/LLM semantic layer is **explicitly rejected** for this binary (offline-first, four-dep constraint; the host agent already routes semantically by reading skill descriptions — topology's router is the deterministic backstop and must stay deterministic). Recall gaps surface as eval failures and are closed by mining real prompts into keyword lists.

### 3. Dependencies & Prerequisites

- Phase 1 merged (hollow suite exists; config plumbing patterns established; shadow-flag convention proven).
- Replay requires `test_command` configured — `doctor` gains a preflight warning when absent.
- Step 1 (default flips) requires Phase 1's shadow logs showing <2% false-block on this repo's own branches; if not met, hold the flip, fix, re-measure — flips are data-gated, not calendar-gated.

### 4. Risk Mitigation

- **Replay wall-clock cost** (full suite at base per check): acceptable for this repo (`cargo test` ≈ seconds); for slow-suite projects, `[tdd] replay_test_command` allows a scoped runner (e.g. `cargo test --test <new>`). Kill switch: `mode = "history"`. Worktree leaks: cleanup guard + `doctor` detects orphaned `gatekeeper-replay-*` worktrees.
- **Entropy FP storm** is the classic failure of this rule class: that's exactly why it ships as `warn` with measured burn-in and path excludes *before* `block`. Rollback: per-rule `severity` downgrade in a payload patch — no binary release needed.
- **Ruleset sync poisoning** (upstream rule breaks our scan): human-reviewed diff + `rules.toml` already on the protected-paths integrity list; CI runs the full bench corpus against every sync PR.
- **PostToolUse latency:** `route --paths` is a glob match against one path — microseconds; reuses the capped hook plumbing.

### 5. Success Metrics (KPIs)

- Hollow suite: **7/7 fixtures rejected** (replay kills (c); (d) review-substance and (f) plan-substance accepted as Phase 4 judge targets — if still open, 5/7 with the remaining two explicitly carried, not silently dropped).
- Secrets bench: **≥ 9/10** in-scope positives detected; env-var indirection remains a *documented* out-of-scope miss (threat-model boundary, stated in `security/README`); FP rate < **1 per 10k lines** on full-history replay; **0/5** negatives flagged post-allowlist.
- Router eval: recall ≥ **0.90** (require skills), precision ≥ **0.80**, enforced in CI; both demonstration prompts route correctly.
- Path triggers: hook integration test proves **100%** of edits under trigger globs inject the context line; pre-commit prints required-skill lines for staged protected paths.

---

## Phase 3 — Risk-Tiered Gate Profiles

**Effort:** ~4–6 days · **Release:** `v0.7.0` · **Neutralizes:** FM1

### 1. Core Objectives

Make ceremony proportional to measured risk — **only now**, because tiering hollow gates would have been tiering theater. After Phases 1–2, every gate that remains in a reduced profile verifies substance, so waiving the others is a calculated trade, not an abdication.

### 2. Step-by-Step Execution

1. **ADR-0017 — tier policy:** three profiles: `docs` (gates: finish[docs-lint only]), `patch` (plan + tdd + verify + finish), `feature` (all seven). Spec the classifier: tier from **cumulative merge-base diff** (never per-commit, so slicing a feature into "patches" can't downgrade it): `docs` iff zero production-classified paths (reuse `tdd.rs:24–103` classifiers); `patch` iff production diff ≤ N lines (default 50, configurable) across ≤ M files (default 3) touching no protected path; else `feature`. **Protected paths force `feature` unconditionally** — reuse the scan integrity list plus `[profiles] force_full_globs`.
2. **`gatekeeper profile --base <ref>` subcommand** (new module `profile.rs`, registered in the Phase 1 dispatch table): prints tier + machine-readable reasons (`tier=patch diff=38L files=2 protected=0`). Exit codes 0/2 per convention.
3. **Gate integration:** each `check_*` resolves the tier first; a gate outside the active profile exits 0 with `PASS (waived: profile=patch)` on stdout — waivers are *visible in transcript*, never silent. The review gate, when it runs, records the tier and the classifier's reason string in the review artifact (audit trail).
4. **Monotonic escalation:** tier recomputed at every gate invocation from the live cumulative diff — a branch that grows past the patch threshold mid-flight escalates to `feature`; previously-waived gates are now demanded before finish. No sticky caching, no downgrade path.
5. **Config:** `[profiles] enabled = false` (default for v0.7.0), `patch_max_lines`, `patch_max_files`, `force_full_globs`. Flip to `enabled = true` in v0.8.0 after burn-in on this repo.
6. **Audit query:** extend `scripts/metrics.sh` to join merged branches against their recorded tier (from review artifacts), emitting the FM1 KPI table per release.

### 3. Dependencies & Prerequisites

- Phases 1–2 enforced by default (waiver safety argument depends on remaining gates having teeth).
- Phase 0 baseline CSV (the before/after comparison is the phase's entire proof of value).
- ADR-0017 approved — this is the one phase that changes the framework's *doctrine* ("trivial tasks still pass through the gates"), so the spec must be human-approved under the Phase 1 `human-commit` rule. The doctrine line in METHODOLOGY.md is amended in the same PR: gates are never *skipped silently*; they are *waived visibly by measured tier*.

### 4. Risk Mitigation

- **Classifier gamed by an agent:** cumulative-diff measurement + protected-path force-full + monotonic escalation close the three obvious vectors (slicing, path-dodging, late growth). Residual: mislabeled "docs" paths that execute (e.g. CI YAML) — add `.github/**` to `force_full_globs` default.
- **Misclassification skips a needed gate:** tier reasons are logged in the review artifact; a post-merge audit (metrics.sh) flags any protected-path branch that merged on a reduced tier — target zero, alert otherwise. Kill switch: `[profiles] enabled = false` restores v0.6.0 uniform behavior instantly; that *is* the rollback plan, which is why the default ships off.
- **Tiering misread as license to skip verification:** the `finish` gate is constitutionally un-waivable in every profile — encode that as a unit test, not a convention.

### 5. Success Metrics (KPIs)

- Ceremony for patch-tier work: **8 commits / 5 artifacts → ≤ 4 commits / ≤ 2 artifacts** (the demonstration-scenario class: plan + verify only).
- Median lead time, patch-tier branches: **≥ 40% reduction** vs Phase 0 baseline CSV (measured, not asserted).
- Distribution sanity: **30–60%** of merged branches classify below `feature` (0% means thresholds too tight; >80% means too loose — both trigger threshold review).
- Invariant (hard): **0** protected-path branches merged on a reduced tier, verified by the post-merge audit every release.

---

## Phase 4 — Measurement, Depth, and Ratchets

**Effort:** ~5–8 days · **Release:** `v0.8.0` · **Hardens:** FM2 residuals, FM4 recall, all KPIs made self-reporting

### 1. Core Objectives

Close the two carried FM2 residuals (test *quality* beyond red-green; prose substance), and convert every Phase 0–3 KPI from "script someone runs" into **self-reporting instrumentation** — because unmeasured gates regress exactly the way unmeasured docs drifted.

### 2. Step-by-Step Execution

1. **`gatekeeper stats --since <tag>`:** productionize `scripts/metrics.sh` into a subcommand emitting the full KPI table (process-to-payload ratio, commits/branch, lead time per tier, tier distribution, waiver counts) as markdown + JSON. Wire into the release checklist; the numbers land in each release's notes.
2. **Mutation testing on the diff (FM2 residual c′):** scheduled weekly CI job (not per-PR — cost discipline): `cargo install cargo-mutants && cargo mutants --in-diff <(git diff origin/main...HEAD)` against open branches plus a monthly full run on `main`. Surviving mutants on changed lines file as repo issues with the mutant diff inline (the Google-Critique pattern, adapted to a solo repo). Target steady-state: **caught-mutant ratio ≥ 80%** on changed lines; the ratchet — CI warns when a release's ratio drops below the previous release's.
3. **Prose-substance judge pilot (FM2 residuals d, f):** new `check design --judge` / `check plan --judge` advisory mode: shells out to the host agent CLI (`claude -p` with a pinned model and a fixed rubric prompt; temperature 0; structured verdict JSON with quoted evidence spans). **Advisory until calibrated:** build a 20-artifact human-graded calibration set from this repo's history; promote judge to `require` only at ≥ 90% agreement, and *never* as sole blocker — it gates alongside, not instead of, the mechanical checks. Offline behavior: skip with a visible warning (fail-open for availability, because the deterministic gates remain the floor). ADR-0018 records the boundary: the binary stays dependency-pure; the judge is an *optional host-provided* layer.
4. **Router recall ratchet (FM4):** grow `routing-eval.jsonl` from real session transcripts each release (mine prompts that preceded gate activity); raise the recall threshold 0.90 → 0.95 once the set exceeds 150 labeled prompts. The eval set, like the scanner ruleset, is maintained data — schedule it with the quarterly gitleaks sync.
5. **Close the loop on the hollow suite:** fixtures (d) and (f) move from `#[ignore]` to judge-advisory assertions; the suite's final state is the standing regression wall — any future gate refactor must keep all seven rejections.

### 3. Dependencies & Prerequisites

- Phase 3 merged with `profiles.enabled = true` flipped (stats needs tier data to report).
- Judge pilot needs the calibration set graded by the human maintainer (~2 hours of artifact grading — the one task in this plan that cannot be delegated to an agent without circularity).

### 4. Risk Mitigation

- **cargo-mutants runtime explosion:** `--in-diff` scoping + weekly cadence + `--timeout` per mutant; it never blocks a PR, so worst case is a stale report, not a stuck pipeline.
- **Judge sycophancy/nondeterminism:** pinned model + temp 0 + rubric with mandatory quoted evidence + calibration threshold + advisory-only default. The known failure mode (judge approves hollow prose) is bounded: mechanical gates still reject fixtures (a)–(c), (e), (g) regardless of judge verdict.
- **Metrics gaming** (optimizing the ratio by padding production LOC): the stats subcommand reports the raw inputs alongside every ratio; ratios are review aids, not gates — only the invariants (zero protected-path waivers, finish un-waivable) ever block.

### 5. Success Metrics (KPIs)

- `gatekeeper stats` output embedded in v0.8.0+ release notes (self-reporting achieved; boolean).
- Mutation caught-ratio on changed lines: ≥ **80%**, ratcheting, never regressing release-over-release.
- Judge–human agreement on calibration set: ≥ **90%** before any `require` promotion; until then it appears in artifacts as advisory verdicts with evidence quotes.
- Router eval: ≥ 150 labeled prompts, recall ≥ **0.95** on require skills.
- Hollow suite: **7/7 standing**, zero `#[ignore]` remaining.

---

## KPI Scoreboard (the whole program on one screen)

| Failure mode | Baseline (v0.4.0) | Target (v0.8.0) | Measured by |
|---|---|---|---|
| FM1 process weight | 8 commits / 5 artifacts / ≈5:1 LOC ratio per patch | ≤4 / ≤2 / patch lead time −40% | `gatekeeper stats`, Phase 0 CSV diff |
| FM2 hollow gates | 7/7 adversarial fixtures pass | **0/7 pass** | `tests/cli_hollow.rs` in CI |
| FM3 doc drift | 1 escape at tag (v0.4.0) | 0 escapes; 0 hand-written usage consts | doc-sync test in `release.yml` |
| FM4 routing | unmeasured; demo prompts misroute | recall ≥0.95 / precision ≥0.80; 100% path-trigger coverage | `cli_routing_eval.rs`, hook tests |
| FM5 scan gaps | 5/10 bench corpus; no entropy; no push protection | ≥9/10 in-scope; FP <1/10k lines; push protection on | `cli_scan_bench.rs`, GitHub API check |

**Standing invariants (never waived, any phase):** finish gate runs in every profile · protected paths force the full lifecycle · every waiver is visible in the transcript and recorded in the review artifact · every enforcement flip is data-gated by its shadow-mode numbers, not by the calendar.

---

**Sequencing logic, compressed:** Phase 0 stops the demonstrated leak for the cost of a TOML patch; Phase 1 makes drift unrepresentable and builds the adversarial suite that defines success; Phase 2 gives gates teeth; Phase 3 — only after the teeth exist — makes the ceremony proportional; Phase 4 makes the whole system report on itself so the fixes can't silently rot the way `USAGE_CHECK` did.
