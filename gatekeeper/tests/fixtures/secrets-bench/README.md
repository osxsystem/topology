# secrets-bench — the FM5 scanner scoreboard

Benchmark corpus for `security/rules.toml` coverage, driven by `tests/cli_scan_bench.rs`.
Provenance: [remediation roadmap](../../../../docs/plans/2026-06-11-five-failure-modes-roadmap.md)
Phase 0, [spec](../../../../docs/specs/2026-06-11-day-zero-containment.md) §2.

## Shape

- **11 positives — runtime-assembled, never literal in this repository.** Each secret-shaped
  payload is built by string concatenation inside the bench test (the `planted_key()` idiom from
  `cli_scan.rs`). Literal positive files would be vetoed by this repo's own pre-commit scan,
  blocked by GitHub push protection, and flagged by every downstream scanner forever.
- **6 negatives — literal `*.txt` files under `negatives/`.** Clean by definition; committing
  them makes the repo's own pre-commit a standing false-positive canary.
  `credential-placeholders.txt` directly canaries the labeled-assignment rule (the one heuristic
  rule); `base64-test-vector.txt`, `cargo-lock-excerpt.txt`, and `git-log-oids.txt` are
  entropy-positive shapes *on purpose* — they are the Phase 2 FP fixtures.

## Literal discipline (enforced by review)

No **secret-shaped** source literal in the bench test may be ≥20 consecutive chars of
`[A-Za-z0-9+/=_-]` — long values are assembled from shorter pieces. Stated precisely: every
current and Phase 0 rule needs a shape or label anchor and cannot match the bench source at
all. For the planned Phase 2 entropy tokenizer (candidate runs `{20,}`) the source still
*contains* ≥20-char candidate runs — long snake_case identifiers, unavoidably — so the
guarantee is that **no candidate carries high entropy**, not that no candidate exists;
low-entropy English identifiers sit below the thresholds.

## Detection criterion

A class counts as detected iff `gatekeeper scan --content` exits 1 **or** prints any
`BLOCK `/`WARN ` finding line on stderr — the stderr clause is load-bearing: warn-severity
rules report without flipping the exit code. **In-scope classes additionally require
attribution**: one of the class's `expect_rules` ids must appear in the finding lines, so a
lucky overlap from the wrong rule cannot satisfy the floor. The bench scans against a **copy of
the live `security/rules.toml`**, so it measures the shipped ruleset, not a replica that can
drift.

## Ratchet

| Release | In-scope floor | Notes |
|---|---|---|
| v0.4.0 (baseline) | 5/11 detected | red — four classes lack covering rules |
| v0.4.1 (Phase 0) | 9/11, rule-attributed | entropy classes stay out of scope |
| v0.6.0 (Phase 2) | ≥10/11 | entropy classes flip in scope **and** negatives move lanes |

The Phase 2 flip is **not** just two `in_scope` edits: three negatives are entropy-positive
shapes, and `--content` reads stdin with **no path context**, so the entropy rule's planned
path excludes cannot see them. Flipping the entropy classes in scope therefore also requires
scanning the negatives through a path-aware lane (or per-rule allows). The corpus files
themselves do not change; the harness lane does. This is a deliberate forcing function, not an
oversight.

## Adding a class

Add a `Case` to `positives()` (respect the literal discipline; set `expect_rules` to the rule
ids that legitimately cover it) or a `*.txt` file under `negatives/` (and bump the count assert
in `bench_negatives_stay_clean`), then update the ratchet table here. A new negative must be
clean against the *current* ruleset at the time it lands; if it is not, that is a real
false-positive finding — fix the rule, not the fixture.
