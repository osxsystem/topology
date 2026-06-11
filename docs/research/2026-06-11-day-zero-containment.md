# Research — day-zero containment (FM5 scan gaps, bench corpus, baseline)

## Problem

The v0.4.0 adversarial audit demonstrated three secret-scan misses (failure mode FM5 in the
[remediation roadmap](../plans/2026-06-11-five-failure-modes-roadmap.md), Phase 0): a bearer JWT
pasted into an artifact, a hyphen-segmented `sk-proj-…` provider key, and a plain
`password = "…"` assignment — all pass `gatekeeper scan` today. Phase 0 closes them with a
rules-only payload patch (no binary change), seeds the benchmark corpus that makes scanner
coverage a measured number, enables GitHub push protection, and captures the process-weight
baseline that later phases divide by. This note grounds that work on the current tree.

## What exists today (verified on this tree, 2026-06-11)

- **Ruleset** (`security/rules.toml`, `schema_version = 1`): seven content rules — AWS key id,
  AWS secret assignment, PEM header, GCP service-account marker, GitHub token prefix, Slack token
  prefix, OpenAI `sk-` prefix. The three demonstrated gaps, precisely:
  - **No JWT rule.** Nothing matches `Authorization: Bearer eyJ…` — the audit's exhibit.
  - **`openai-key`** (`security/rules.toml:54`) is `\bsk-[A-Za-z0-9]{20,}\b`: it requires 20+
    alphanumerics *immediately* after `sk-`. A segmented key (`sk-proj-…`, `sk-ant-…`) stops the
    run at the first hyphen (4 chars in) and never matches. Modern provider keys are segmented.
  - **No generic labeled-assignment rule.** The only assignment-anchored rule is
    `aws-secret-access-key` (`security/rules.toml:19`), keyed to a `secret…key` label;
    `password = "…"`, `api_key: …`, `auth_token=…` pass untouched.
- **Scan mechanics** (`gatekeeper/src/scan.rs`):
  - Findings print to **stderr** as `BLOCK <rule-id>: <desc> [<loc>] (redacted: <hint>)` or
    `WARN <rule-id>: …`; exit code is 1 iff at least one **block**-severity finding, else 0
    (`report`, scan.rs:307–328). `Severity` is `Block | Warn` (scan.rs:64–67) — **`warn` is
    already valid under schema 1**; no schema bump is needed for a warn rule.
  - On the **hook path** warns are dropped entirely (`emit_decision`, scan.rs:943–953): a warn
    rule is visible at pre-commit and in CI, but never blocks an agent's tool call. That is the
    right launch posture for a heuristic rule.
  - The allowlist is span-scoped and must match the *whole* finding span (scan.rs:259–273), so
    an allow entry can never exempt a larger secret that contains the allowed text.
  - `scan --content` loads `<framework_root>/security/rules.toml` (scan.rs:358) and applies
    content rules to stdin — the natural bench lane.
- **Test idiom** (`gatekeeper/tests/cli_scan.rs`): a tempdir scratch root with a `skills/` marker
  and a `security/rules.toml`; the binary runs with `current_dir(root)`. Secret-shaped values are
  **assembled by concatenation** (`planted_key()`, cli_scan.rs:79–81) so no matchable literal
  ever lives in the repository. The existing runner nulls stderr; the bench needs a
  stderr-capturing variant because warn findings never reach the exit code.
- **Protected-path workflow**: `security/rules.toml` is on its own integrity list
  (rules.toml:167), so `scan --staged` aborts any commit that stages it. The sanctioned override
  is a **human** `git commit --no-verify` at the terminal (hooks/pre-commit.sh:38) — and the
  command rule `git-commit-no-verify` (rules.toml:121–125) blocks an agent from using that flag.
  Consequence: **the green commit that lands the new rules must be made by the human.**
- **Push protection** is a repository setting (GitHub-side provider-token scanning on push), not
  code; it needs an authenticated `gh` call by the repo owner.
- **Payload distribution**: the release tarball ships `security/rules.toml`
  (`scripts/build-payload.sh:95–97`, packed by the `payload` job in `release.yml`) — rule
  changes reach consumers on reinstall with no binary rebuild.

## Detection matrix — the ten positive classes

| # | Class | Covering rule today | v0.4.0 | v0.4.1 target |
|---|---|---|---|---|
| 1 | JWT in `Bearer` header | — | miss | **detect** (new `jwt-structural`) |
| 2 | Segmented `sk-proj-…` key | `openai-key` (too narrow) | miss | **detect** (pattern broadened) |
| 3 | GitHub PAT `ghp_…` | `github-token` | detect | detect |
| 4 | Slack `xoxb-…` | `slack-token` | detect | detect |
| 5 | PEM private-key block | `private-key-block` | detect | detect |
| 6 | Unlabeled 64-hex | — | miss | miss — **entropy class, Phase 2** |
| 7 | Unlabeled base64 ≥40 | — | miss | miss — **entropy class, Phase 2** |
| 8 | `password = "…"` assignment | — | miss | **detect** (new warn rule) |
| 9 | AWS key pair | `aws-access-key-id` + `aws-secret-access-key` | detect | detect |
| 10 | GCP service-account JSON | `gcp-service-account` | detect | detect |
| 11 | Segmented `sk-ant-…`, `_` in tail | `openai-key` (too narrow) | miss | **detect** (pattern broadened) |

Today: **5/11**. Phase 0 target: **9/11** — classes 6 and 7 stay open *by design* until the
Phase 2 entropy engine; asserting them now would force a hasty entropy hack into a rules-only
patch. Class 11 was added at review: real Anthropic tails carry underscores — the one shape the
first draft of the broadened pattern would still have missed.

## Proposed patterns — verification and false-positive sweep

- `jwt-structural`: `\beyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\b` — `eyJ` is
  base64url for `{"`, and the three dot-joined base64url segments are structural; random base64
  (no dots, no `eyJ` sigil) cannot satisfy it. Accepted miss: an unsigned `alg=none` token
  (`header.payload.` — empty third segment) does not match; the threat model is accidental paste
  of real, signed tokens, which always carry three segments.
- `openai-key` (broadened): `\bsk-(?:[A-Za-z0-9_]+-)*[A-Za-z0-9_]{20,}\b` — tolerates any number
  of hyphenated segment prefixes (`proj-`, `ant-`) before the long tail, and underscores inside
  segments and tail: real Anthropic tails carry `_` and `-`, and a pure-alnum tail class would
  miss them (bench class 11 pins exactly this). Plain `sk-<20+>` still matches (the group is
  `*`), so this strictly widens; keeping the rule id stable preserves any future allowlist
  references. The FP surface stays bounded by the leading `\b`: hyphenated prose ("task-",
  "risk-", "desk-") has a word character before the `sk`, so no boundary forms and no match
  starts there.
- `labeled-secret-assignment`:
  `(?i)\b(?:api[_-]?key|auth[_-]?token|password|passwd|bearer)\s*[=:]\s*[\x22\x27]?[A-Za-z0-9+/=_-]{16,}`
  at `severity = "warn"`. Known limits, accepted deliberately:
  - `\b` before the label means `OPENAI_API_KEY=…` does **not** match (`_` is a word character,
    so there is no boundary inside `…AI_API…`) — vendor-prefixed env names are the value rules'
    job (class 2 above proves it); this rule is the belt-and-suspenders for *bare* labels.
  - A labeled UUID (`api_key: 550e8400-…`) would warn. At warn severity that is a tolerable —
    arguably correct — outcome for a credential-labeled assignment.
- **Empirical sweep (2026-06-11):** all three patterns grepped over this tree (ERE equivalents,
  `.git`/`target` excluded) — **0 matches each**, re-confirmed after the bench and these docs
  were drafted. The ERE forms drop `\b`, making them *strictly broader* than the shipped Rust
  regexes, so 0 matches is conservative rather than approximate. No allowlist entries are needed
  at day zero, and the repo's own pre-commit scan stays green after the rules land. (Docs,
  including this note, write key shapes with ellipses precisely so this stays true.)

## Bench-corpus design facts

The plan's letter says "fixtures with 10 synthetic positives"; the repo's own hygiene forbids
that reading, and the deviation matters:

- **Positives cannot be literal files.** Five of the eleven classes are detected *today* — the
  repo's own `scan --staged` would veto the commit that adds them; GitHub push protection
  (enabled in this same phase) would block the push; and every downstream scanner would flag
  this repo forever. The house idiom (`planted_key()`) already answers this: **positives are
  assembled at runtime by concatenation**, never literal in the tree.
- **Literal discipline:** no *secret-shaped* source literal in the bench may be ≥20 chars drawn
  from `[A-Za-z0-9+/=_-]`; long values are assembled from shorter pieces. Stated precisely:
  every current and Phase 0 rule needs a shape or label anchor and cannot match the bench source
  at all. The planned Phase 2 entropy tokenizer (candidate runs `{20,}`) will still see ≥20-char
  candidates in the source — long snake_case identifiers, unavoidably — so the guarantee there
  is that **no candidate carries high entropy**, not that no candidate exists; low-entropy
  English identifiers sit below the thresholds.
- **Negatives are literal files** (`tests/fixtures/secrets-bench/negatives/`): they are clean by
  definition, and committing them makes the repo's own pre-commit a standing FP canary — any
  future rule that flags a Cargo.lock checksum, SVG path data, git OIDs, a UUID, a base64 test
  vector, or placeholder credentials (`password = "changeme"`, `<YOUR_API_KEY>` — the direct FP
  canary for the labeled rule) fails the bench *and* annoys the maintainer immediately. Three of
  the six are entropy-positive shapes *on purpose*: they are Phase 2's FP fixtures. And because
  `scan --content` reads stdin with **no path context**, Phase 2's planned path excludes cannot
  see them — flipping the entropy classes in scope therefore also requires moving the negatives
  to a path-aware scan lane. Deliberate forcing function, recorded in the corpus README.
- **Detection criterion:** a class counts as detected iff the scan exits 1 *or* prints any
  `BLOCK `/`WARN ` finding line — required because the labeled rule ships at warn (exit stays 0).
- The bench copies the **live** `security/rules.toml` into its scratch root, so it measures the
  shipped ruleset, not a test replica that can drift.

## Threat-model check ([[match-threat-model-before-importing-hardening]])

The adversary is a sycophantic or careless agent pasting values into artifacts, logs, and shell
commands — not a malicious insider. So: value-shape rules at `block` (high precision, agent-facing
veto), the heuristic label rule at `warn` (its FP cost lands on the human; block would deadlock
honest work), push protection as the free provider-side backstop (GitHub detects exactly the
provider-token class), and **no entropy scanning yet** — that rule class is the classic FP storm
and gets a measured burn-in in Phase 2, not a day-zero gamble. Env-var indirection
(`$OPENAI_API_KEY` references) is out of scope as a threat-model boundary, to be documented, not
silently ignored.

## Conclusion

Go. Three `rules.toml` additions (two block, one warn) under the existing schema 1, a
runtime-assembled 11-positive / 6-literal-negative bench that is red today at 5/11, green at
9/11 when the rules land, and rule-attributed (an in-scope class passes only via its own rule),
push protection via one human `gh` call, and a read-only metrics script for the FM1 baseline. No binary change, no new dependencies
([ADR-0007](../adr/0007-security-scanner-dependencies.md) untouched), one human `--no-verify`
commit for the protected rules file. Spec: [day-zero containment](../specs/2026-06-11-day-zero-containment.md).
