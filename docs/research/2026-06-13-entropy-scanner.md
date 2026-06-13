# Research — entropy scanner (FM5: unlabeled-secret class fix)

- **Date:** 2026-06-13 · **Feature slug:** `entropy-scanner`
- **Source of truth:** `docs/plans/2026-06-11-five-failure-modes-roadmap.md:112-113` (FM5 class fix) · ROADMAP Phase 15.
- **Method:** fan-out code map by a research subagent (grep/Read primary; context-engine index may be wiped). Every claim carries a `file:line` citation.

## The problem (one line)

The secret scanner is **prefix/label-anchored**: every rule is a regex keyed on a known sigil (`AKIA…`, `eyJ…`, `api_key=…`). An *unlabeled* high-entropy value — a raw 64-hex token, a bare 40-char base64 blob, a live JWT body with no assignment — has no anchor and commits clean. FM5's class fix: detect by **Shannon entropy**, not by label.

## Findings (cited)

### What exists & is reusable
- **Rule pipeline** (`scan.rs`): `RawRule{id,kind,severity,description,pattern}` (45-52); `Kind{Content,Command}` enum (55-60) — **add `Entropy`**; `Severity{Block,Warn}` (62-67); `CompiledRule` (108-114); `parse_rules` (159-233) splits rules by kind into per-kind `RegexSet`s (192-195).
- **Shadow is already built in.** `report()` (307-328): `Block` → emit + exit 1; `Warn` → emit to stderr + **exit 0**. So shipping entropy at `severity = "warn"` *is* shadow mode — findings surface without blocking. The hook path `emit_decision()` (942-952) only denies on `Block`. No separate `emit_shadow` needed for the scanner.
- **Scan surfaces** (`main.rs` `cmd_scan` 349-401): `--hook` (PreToolUse JSON, reconstructs file image), `--cmd`, `--content` (stdin, **no path**), `--staged` (git blobs, **has path**), `--check-path`. `scan_with()` receives the file path on the staged/hook lanes.
- **Severity/exit contract** is exactly what shadow-first needs — no change required.
- **Bench corpus** (`tests/cli_scan_bench.rs`, `tests/fixtures/secrets-bench/`): 11 runtime-assembled positives + 6 literal negatives. Two positives — `hex64-unlabeled` and `base64-unlabeled` — are marked **`in_scope = false`** with `expect_rules = []`, explicitly reserved as the Phase 2 entropy targets. Negatives include `base64-test-vector.txt`, `cargo-lock-excerpt.txt`, `git-log-oids.txt`, `svg-path-data.txt`, `uuid-config.txt` — all high-entropy-but-benign.

### What is net-new
- **`kind = "entropy"`** variant + rule fields `charset = "base64"|"hex"`, `min_length`, `threshold_bits_per_char` (defaults base64 ≥ 4.5, hex ≥ 3.0 — detect-secrets lineage).
- **Shannon entropy** — no existing entropy/log2 code anywhere (grep clean). Pure std: `H = -Σ p_i log2 p_i`. ~10 lines.
- **Entropy lane** — tokenize candidate runs `[A-Za-z0-9+/=_-]{min_length,}`, score each token, flag those over threshold. Separate from the `RegexSet` one-pass lane.
- **`[scan] exclude_paths`** — no `[scan]` table exists in `rules.toml` today; no path-filtering in `scan.rs`. Add an optional `[scan]` section (glob list) + a dep-free wildcard matcher (`*`/prefix), applied on the path-bearing lanes (`--staged`, `--hook`).
- **`scripts/sync-gitleaks-rules.sh`** — fetch gitleaks `gitleaks.toml`, translate a curated provider-prefix subset into our schema with a `# synced-from: gitleaks@<sha>` provenance header; human-reviewed diff, **never auto-merged** (`rules.toml` is a protected path).
- **ADR-0018** (next free; the plan's "ADR-0015" reference is stale — 0015 is plugin-retirement).

### Landmines
1. **Schema-version gate** (`scan.rs:162`, `version.rs` `SCHEMA_VERSION=1`): hard `!= 1` reject. Must accept **1 or 2**; bump the advertised version to 2. The round-trip test `advertised_schema_is_accepted_by_parser` (version.rs:37-47) keeps the two in sync.
2. **FP on benign high-entropy content — the central design problem.** Entropy will fire on `base64-test-vector`, `git-log-oids`, `cargo-lock` hashes, `uuid`. `exclude_paths` mitigates on path-bearing lanes, but **`--content`/`--cmd` have no path**, so excludes can't see the bench negatives. This is why entropy must ship `warn` (shadow) AND why a burn-in FP measurement precedes any promotion to `block`. The negatives' bench handling needs a design decision (see open decisions).
3. **Bench floor coupling.** `cli_scan_bench.rs` asserts in-scope detection. Flipping `hex64-unlabeled`/`base64-unlabeled` to `in_scope=true` raises the floor — must confirm entropy actually catches both, and that warn-severity findings count as "detected" in the harness (it parses `BLOCK `/`WARN ` lines, so warn counts — verify).
4. **No new deps** (ADR-0007): entropy + glob matching are pure std.

## Open decisions (carried to design)
1. **Negatives under `--content` (no path).** Excludes can't fire there. Options: (a) keep entropy `warn` so negatives only WARN (exit 0) and the bench negative-assertion checks *block*-level cleanliness, not warn-silence; (b) add path context to the bench negative lane; (c) tune `min_length`/threshold so the specific benign vectors fall below threshold. Likely (a) + (c).
2. **Default on/off.** Ship the entropy rules present-but-`warn` (shadow), or behind a `[scan]` enable flag? Plan says `warn` + burn-in then promote — so present-and-warn, never `block` in this phase.
3. **gitleaks sync scope.** This phase: write the script + ADR; actually running/committing a synced ruleset is a separate human-reviewed PR (the script is the deliverable, not its output).

## Readiness
Rule pipeline, severity/shadow semantics, scan surfaces, and the bench harness all exist. Net-new is bounded: the `Entropy` kind + fields, a Shannon helper, the tokenizer lane, `[scan] exclude_paths` + a glob matcher, the sync script, and ADR-0018. The schema-version gate is the one must-touch landmine.
