# Research: Per-language recognizers (slice #4)

- **Date:** 2026-06-14
- **Feature slug:** per-language-recognizers
- **Origin:** Slice #4 of the portability-first experiment. Pitched (original workflow, rank 4 `[M]`) as:
  *"Ship per-language recognizers as a COMPILED const table in the binary, selected at runtime by
  file-presence (Package.swift→Swift, go.mod→Go). Do NOT emit them into writable config.toml — keeping
  recognizers compiled preserves the trust boundary the current hardcoded regexes enjoy: operator config
  may ADD, never REPLACE, the floor's recognizers."*

## Sub-questions

1. What exactly would per-language recognizers feed, and is it gated?
2. What's the marginal value vs the existing escape hatch?
3. What does building it cost (protected files, drift)?
4. What is its strategic role, and is that role currently live?

## Findings (cited; verified against the working tree)

### 1. The consumer — `parse_test_count`, and it is opt-in-gated

`parse_test_count` (`main.rs:2057-2104`) is the finish gate's runner-summary recognizer. Built-ins:
cargo (`(?m)^test result: \w+\. (\d+) passed`, `main.rs:2062`) and pytest
(`(?m)(\d+) passed[^\n]* in [0-9.]+s`, `main.rs:2075`), then user `finish_extra_count_patterns`
(`main.rs:2087`). First-match-wins; sums capture group 1.

It feeds `apply_finish_floor` (`main.rs:2107-2189`). **Crucially the floor is gated:**
`finish_require_test_count` defaults **false** (`config.rs:205,318`). With it off, `apply_finish_floor`
emits a SHADOW line only and PASS depends **solely on exit code 0** (`main.rs:2162-2165,2187`). The
floor blocks only `if cfg.finish_require_test_count && !floor_pass` (`main.rs:2168`). **So a non-Rust
stack is never blocked by an unrecognized summary by default** — `parse_test_count` returning
`Unrecognized` is inert unless the operator opted into the floor.

### 2. Marginal value vs the existing escape hatch

For an opt-in user (`require_test_count = true`) on a non-cargo/pytest stack, the gap is real but
**already covered by `finish_extra_count_patterns`** (`main.rs:2087-2101`): a one-line regex
(`Executed (\d+) test` for Swift XCTest, `Tests:.*?(\d+) passed` for jest) makes their summary
recognized. Per-language recognizers would save that one regex — convenience, not capability. (The one
genuinely-hard case is a **count-less** runner like `go test`'s bare `ok`, which no count-regex can
express; but go is also the case where a "marker = success" semantic, not a count recognizer, would be
needed — out of this slice's "recognizer count table" shape.)

### 3. Cost — a PROTECTED-file edit, plus drift

`parse_test_count` and `apply_finish_floor` live in `main.rs`, which is in `[integrity].protected_paths`
(`security/rules.toml:223`). Any recognizer addition — even consulting a const table defined in a
non-protected module — requires editing `parse_test_count`/`apply_finish_floor` (signature change to get
the project root for file-presence selection, plus the table-iteration loop). So slice #4 **cannot avoid
a protected `main.rs` edit** (human `--no-verify`). The protection exists precisely to gate changes to
the enforcement core behind human review — spending it on opt-in polish is a poor trade. The original
workflow also flagged the maintenance cost: a curated in-tree const table *"drifts, needs releases, but
stays trusted."*

### 4. Strategic role — prerequisite for a flip that is currently DEFERRED

The transcript frames #4 as the enabler for flipping the `require_test_count` / `design.approval`
defaults **ON**: *"move toward default-ON ONLY after (a) per-language markers exist (#4, else the Swift
case hard-fails day one) AND (b) burn-in clears <2% would-block."* But the Phase-15 burn-in harness
already determined **both flips STAY DEFERRED** on independent evidence (TDD-replay 62.5% would-block;
entropy WARN). So building #4 now is building a prerequisite for a flip that is deferred on grounds #4
does not change. It is **premature** relative to its own strategic justification.

## Conclusion — the thinnest, most-gated, highest-cost slice

| Dimension | Slice #4 |
|---|---|
| Blocks anyone by default? | **No** — `require_test_count` off by default; unrecognized summary is inert |
| Escape hatch for opt-in users? | **Yes** — `finish_extra_count_patterns` (one regex) |
| Build cost | **Protected `main.rs` edit** (human `--no-verify`) + ongoing table drift |
| Strategic payoff | Prerequisite for a flip that is **currently deferred** on independent burn-in evidence |

Unlike #1 (real security fix), #2 (real collision detector), #3 (real enforcement-backed config tax),
slice #4 changes nothing for any user by default, has a working escape hatch, costs a protected-core
edit, and enables only a deferred flip. This is the strongest "defer and re-measure" candidate of the
experiment.

## Open decision (carried to design — the maintainer's call)

Defer #4 and proceed to the re-measure that closes the experiment; build the full compiled recognizer
table (Swift/Go/jest) as a deferred-flip enabler (accepting the protected-`main.rs` cost); or build a
minimal Swift-only recognizer (the field-report language). Surfaced before any code.
