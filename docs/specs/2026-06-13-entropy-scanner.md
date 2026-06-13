# Design: entropy scanner (FM5 — unlabeled-secret class fix)

- **Date:** 2026-06-13
- **Feature slug:** entropy-scanner
- **Status:** approved
- **Approval:** self-approved under the maintainer's overnight Phase 15 autonomy grant (decide autonomously + document); decisions recorded below.
- **Research:** [docs/research/2026-06-13-entropy-scanner.md](../research/2026-06-13-entropy-scanner.md) · ROADMAP Phase 15 · plan `2026-06-11-five-failure-modes-roadmap.md:112-113`

## Problem

Every scanner rule is prefix/label-anchored (regex on a known sigil). An *unlabeled* high-entropy
value — a raw 64-hex token, a bare 40-char base64 blob — has no anchor and commits clean (FM5). The
class fix: detect by **Shannon entropy** of candidate tokens, not by label. The bench already reserves
`hex64-unlabeled` and `base64-unlabeled` as `in_scope = false` Phase-2 targets.

## Constraints / non-goals

- **No new deps** (ADR-0007): Shannon entropy and glob matching are pure `std`.
- **Shadow-first.** Entropy rules ship `severity = "warn"` (which already means *emit, exit 0* in
  `scan.rs report()`). They never block in this phase; promotion to `block` is **deferred** behind a
  burn-in FP measurement (out of scope here).
- **Fundamental limit (drives the whole design).** Entropy cannot distinguish a git OID / `Cargo.lock`
  checksum / base64 test vector from a real secret — all are genuinely high-entropy. So FP control is
  *not* "make entropy never fire on benign blobs" (impossible); it is (a) ship `warn` so FPs don't
  block, (b) `[scan] exclude_paths` on path-bearing lanes, (c) measure FP rate in burn-in before any
  `block` promotion.
- **Non-goals:** promoting to `block`; running/committing a synced gitleaks ruleset (we ship the
  *script* + ADR, not its output); any new charset beyond `base64`/`hex`.

## Approaches considered

1. **Token-tokenize + per-token Shannon entropy (chosen).** Walk the text, extract maximal runs of
   `[A-Za-z0-9+/=_-]` of length ≥ `min_length`, compute bits-per-char Shannon entropy of each run,
   flag runs ≥ `threshold_bits_per_char`. Matches the detect-secrets lineage and the plan verbatim.
   Trade-off: O(n) scan, dep-free; the known FP on indistinguishable benign blobs is handled by
   warn+excludes+burn-in, not by the detector.
2. **Vendored gitleaks / `regex`-set of provider patterns only.** Rejected as the *entropy* solution:
   it stays label-anchored (doesn't close the unlabeled class) and adds a dep. (The maintained-ruleset
   *sync script* is a complementary deliverable, not a replacement.)
3. **Block immediately with a tight threshold.** Rejected: no threshold separates a git OID from a hex
   key; blocking on entropy without burn-in would wreck the commit flow. Hence shadow-first.

## Decision

**Approach 1, shipped `warn` (shadow).**

### Schema v2
- `security/rules.toml`: `schema_version = 2`. `scan.rs`/`version.rs`: accept **1 or 2** (replace the
  `!= 1` hard reject at `scan.rs:162`; advertise `SCHEMA_VERSION = 2`; the round-trip test keeps them
  synced). Schema-1 files keep parsing unchanged (back-compat).

### `kind = "entropy"` rule
New `Kind::Entropy` variant; new optional `RawRule` fields used only by entropy rules:
- `charset = "base64" | "hex"` — alphabet size for the bits-per-char normalization (base64 → 64, hex → 16).
- `min_length` — minimum token length to consider (hex `32`, base64 `20`).
- `threshold_bits_per_char` — flag at/above (defaults: base64 `4.5`, hex `3.0`).

Two seed rules in `rules.toml` (both `severity = "warn"`): `hex-high-entropy` (hex, 32, 3.0) and
`base64-high-entropy` (base64, 20, 4.5).

### Entropy lane (`scan.rs`)
Separate from the `RegexSet` one-pass lane. For each entropy rule: scan the data for maximal
`[A-Za-z0-9+/=_-]{min_length,}` runs; for each run compute `H = -Σ p_i·log2(p_i)` over its bytes
(bits per character); flag runs with `H >= threshold_bits_per_char`. Emit a `Finding` (reusing the
existing struct/format) with the rule id, `warn` severity, location, and a redacted hint. Runs through
the existing allowlist (`is_allowed`).

### `[scan] exclude_paths`
New optional `[scan]` table in `rules.toml`: `exclude_paths = ["*.lock", "*.svg", "*.min.js",
"tests/fixtures/"]`. A dep-free matcher supports `*` wildcards and a trailing-`/` directory-prefix.
Applied **only on path-bearing lanes** (`--staged`, `--hook`): if the file path matches any glob, the
entropy lane is skipped for that file (regex rules still run — excludes scope entropy only, to bound
its FPs without weakening labeled detection). `--content`/`--cmd` have no path, so excludes do not
apply there (documented).

### gitleaks sync script
`scripts/sync-gitleaks-rules.sh`: fetch a pinned gitleaks `gitleaks.toml`, translate a curated
provider-prefix subset into our `[[rule]]` schema, write to a *review* file (not `rules.toml`
directly) with a `# synced-from: gitleaks@<sha>` provenance header, and print a diff. **Never
auto-merges** (`rules.toml` is a protected path). Shellcheck-clean; offline-safe (no network in CI —
guarded). ADR-0018 records the constraint-compatible-substitute decision.

## Risks & open questions
- **Bench negatives WARN under `--content` (no path).** Several negatives (cargo-lock hashes, git OIDs,
  base64 vectors) are genuinely high-entropy and will WARN there. Decision: the bench negative
  assertion checks **no `BLOCK`** on negatives (warns are shadow and expected); positives floor rises
  as the two unlabeled cases flip to `in_scope = true` (the harness counts `WARN ` lines as detection).
- **Threshold tuning** is burn-in's job, not this phase's. Defaults are the detect-secrets lineage.
- **Performance:** O(n) single pass over candidate runs; negligible vs. the existing regex sets.

## Acceptance criteria
- Schema-2 `rules.toml` parses; a schema-1 file still parses (back-compat test).
- `hex64-unlabeled` and `base64-unlabeled` bench positives now **detected** (WARN) → in-scope floor met
  (≥10/11); the two flip to `in_scope = true`.
- A `block`-severity finding is NOT produced by any entropy rule (it's `warn`) — exit code unaffected
  by entropy alone.
- `exclude_paths` suppresses the entropy lane on a matching path (`*.lock`) on `--staged`, and does NOT
  suppress labeled/regex rules; an entropy hit on a non-excluded path still WARNs.
- Bench negatives produce **no BLOCK** (warns permitted).
- `shannon_entropy` unit tests: uniform 64-hex ≈ 4.0 bits/char (flagged at hex 3.0); a low-entropy
  repetitive string scores low (not flagged); empty/short tokens skipped.
- `scripts/sync-gitleaks-rules.sh` exists, is shellcheck-clean, has a provenance header, and does not
  write `rules.toml` (writes a review file); CI does not invoke network.
- `cargo test`/`clippy`/`fmt` green; **ADR-0018** written; back-compat for schema-1 preserved.
