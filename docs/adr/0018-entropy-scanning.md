# 0018 — Entropy scanning: detect unlabeled secrets by Shannon entropy (schema v2, shadow-first)

- **Status:** 🟢 Accepted
- **Date:** 2026-06-13
- **Spec:** [entropy-scanner](../specs/2026-06-13-entropy-scanner.md) · ROADMAP Phase 15 · plan `2026-06-11-five-failure-modes-roadmap.md:112-113`

## Context

Every secret-scan rule was prefix/label-anchored: a regex keyed on a known sigil (`AKIA…`, `eyJ…`,
`api_key=…`). An *unlabeled* high-entropy value — a bare 64-hex token, a 40-char base64 blob — has no
anchor and committed clean (FM5). The class fix is to detect by **Shannon entropy** of candidate
tokens rather than by label.

The fundamental obstacle: entropy **cannot distinguish a genuine secret from a benign high-entropy
blob** — a git OID, a `Cargo.lock` checksum, a base64 test vector, and a real API key all have
near-maximal entropy. A detector keyed on entropy alone therefore has an irreducible false-positive
rate on real codebases. This shapes the entire decision.

## Decision

1. **Rules schema v2.** `security/rules.toml` advertises `schema_version = 2`; the parser accepts **1
   or 2** (schema-1 files keep parsing unchanged — back-compat). A new `kind = "entropy"` rule carries
   `charset = "base64" | "hex"`, `min_length`, and `threshold_bits_per_char`.

2. **Detection lane.** `scan.rs` walks maximal candidate runs of the rule's charset alphabet
   (`[0-9a-fA-F]` for hex, `[A-Za-z0-9+/=_-]` for base64) of length ≥ `min_length`, computes the
   Shannon entropy `H = -Σ p_i·log2(p_i)` (bits per character) of each run, and flags runs with
   `H ≥ threshold_bits_per_char`. Pure `std` (`f64::log2`), no new dependency (ADR-0007). Findings
   reuse the existing `Finding`/emission and pass through the allowlist. Seed rules: `hex-high-entropy`
   (hex, 32, 3.0) and `base64-high-entropy` (base64, 20, 4.5) — detect-secrets lineage.

3. **Shadow-first.** Entropy rules ship `severity = "warn"`, which already means *emit, exit 0* — they
   surface findings without ever blocking. The default→`block` promotion is **deferred** behind a
   burn-in false-positive measurement (a later phase), never the calendar — consistent with the Track 3
   doctrine and the gate shadow rollouts.

4. **`[scan] exclude_paths`.** A new `[scan]` table lists globs (`*.lock`, `*.svg`, `*.min.js`,
   `tests/fixtures/`) matched by a dep-free matcher (`*` wildcard + trailing-`/` directory prefix).
   Excludes suppress the **entropy lane only**, and **only on path-bearing lanes** (`--staged`,
   `--hook`) — regex/labeled rules always run (excludes never weaken labeled detection), and
   `--content`/`--cmd` have no path so excludes do not apply there.

5. **Maintained-ruleset sync — constraint-compatible substitute for vendoring gitleaks.**
   `scripts/sync-gitleaks-rules.sh` fetches a pinned upstream `gitleaks.toml`, translates a curated
   provider-prefix subset into a **review file** (`security/rules.gitleaks-review.toml`) with a
   `# synced-from: gitleaks@<sha>` provenance header, and prints which providers are new vs. shipped.
   It **never** writes `rules.toml` (a protected path) and is **never auto-merged**; a human copies
   wanted rules by hand. Offline-guarded (no-op exit 0 without network, never runs in CI).

## Consequences

- The unlabeled-secret class is now *visible*: a bare 64-hex / 40-char-base64 token produces a
  `WARN hex-high-entropy` / `WARN base64-high-entropy` finding where it previously committed clean.
- **False positives are expected, not eliminated.** High-entropy benign content (lockfile hashes, git
  OIDs, base64 vectors) WARNs. This is acceptable *because* entropy ships shadow (warn, exit 0); FP
  rate is what burn-in measures before any `block` promotion. The secrets bench encodes this: negatives
  are asserted **BLOCK-clean** (warns permitted), and the two unlabeled positives are now in-scope and
  detected via WARN.
- A self-referential example of the FP: `base64-high-entropy` WARNs on the **canonical AWS-docs example
  secret** that is embedded inside an allowlist *pattern* in `rules.toml` itself (the allowlist entry is
  rule-scoped to `aws-secret-access-key`, so it does not suppress the entropy rule). Left as documented
  shadow noise — the kind of FP burn-in measures, not a real secret.
- The gate stays **CLI/scan-only**; entropy findings on the PreToolUse hook lane are warn and dropped by
  `emit_decision` (only `block` denies), matching the existing hook posture.
- No new dependency; offline-safe; deterministic.
