# Research: Hardened portability config layer (slice #3)

- **Date:** 2026-06-14
- **Feature slug:** hardened-config-layer
- **Origin:** Slice #3 of the portability-first experiment. Pitched as: widen `ProjectConfig` with
  `[project]` `test_globs` / `test_success_markers` / `fmt_command` / `lint_command` (+ per-language
  default profiles), under a hardening invariant — *operator config may only TIGHTEN, never loosen, a
  gate* (`test_globs` add-only; markers additive, still `count>0 ∧ exit-0`; broad/self-matching globs
  rejected; config file stays protected).

## Sub-question

For each pitched field: **is there an existing gate consumer / real Rust-monoculture hardcoding to
parameterize, or would it be config with no enforcement behind it?** (A config knob with no Rust
consumer violates the three-language-lanes rule — enforcement-via-Markdown — and the simplicity rubric.)

## Findings (cited; verified against the working tree by a 4-reader research workflow)

### What already exists

`ProjectConfig` (`config.rs:57-94`) already carries: `base_branch`, `test_command`,
`[verify] allowed_command_prefixes`, `[finish] extra_count_patterns`, `[tdd] replay_test_command`, etc.
So the runner command and additive count-patterns are **already configurable**.

### Pitched field 1 — `fmt_command` / `lint_command`: **NO consumer**

gatekeeper **never spawns a formatter or linter** in any gate (finish only runs `test_command` / the
CLI command via `sh -c`) or doctor probe (`probe_version` only runs `gatekeeper --version`). Wiring
these fields to do anything would require **also building a new fmt/lint gate** — scope creep beyond an
"add-only config" slice. Adding the fields alone = enforcement-via-Markdown (relies on the human still
running `just check` by hand, exactly as today). **Verdict: drop — speculative, no consumer.**

### Pitched field 2 — `test_success_markers`: **mostly redundant**

The finish gate's recognizer `parse_test_count` (`main.rs:2057-2104`) has two built-in patterns — cargo
(`^test result: \w+\. (\d+) passed`, `main.rs:2062`) and pytest (`main.rs:2075`) — then folds in
`finish_extra_count_patterns` (`main.rs:2087`). That escape hatch already lets **any** runner feed a
numeric count (jest `Tests:.*?(\d+) passed`, swift `Executed (\d+) test`). The only genuine gap is a
**count-less** runner (go's bare `ok`, a `PASS` token) — but `parse_test_count` PASS hinges on
`count>0` (`main.rs:2131`), so a string-marker field would add semantics the count hatch cannot express.
**However:** `finish_require_test_count` defaults **off** (`config.rs:205/318`), so a non-Rust stack
passes on exit-code-0 alone (`main.rs:2187`) and is **never blocked by default**. **Verdict: marginal —
duplicates `finish_extra_count_patterns` except for count-less runners, and only bites opt-in.**

### Pitched field 3 — `test_globs`: **real consumer, but the classifier is already polyglot**

The TDD gate is the sole file classifier. `is_test_path` (`tdd.rs:31-78`) and `is_artifact_path`
(`tdd.rs:91-107`) are hardcoded but **already language-agnostic**: `tests/`, `test/`, `__tests__/`,
`spec/`, `*_test.*`, `*Test*.*` (Java/Swift — `FooTests.swift` matches), `*.test.*`/`*.spec.*` (JS/TS),
`test_*.py`. `classify` (`tdd.rs:116-121`) defines prod as the **negation** (`!test && !artifact`) —
there is **no hardcoded `.rs`/`src/`** prod glob to remove. Classification is load-bearing (drives the
"first prod commit must be preceded by a test-only commit" verdict, `tdd.rs:407-433`, and the replay
checkout set, `tdd.rs:178-189`) but takes only `&str`, no config. **Verdict: a `test_globs` field would
have a live consumer, but its only value is letting a project *extend* already-broad conventions — there
is no Rust-monoculture to fix here.**

### The real monoculture tax — **NOT in the pitch**: the cargo-centric replay allowlist

`default_allowed_prefixes` (`config.rs:180-190`) ships `["cargo test", "cargo run", "just",
"git diff/log/show/status"]`. This list **fail-closes** the verify-replay gate (`verify.rs:471-475,666,
980-988`) **and** the TDD-replay gate (`tdd.rs:307-310` routes through `verify::execute_step` →
`is_command_allowed`, `verify.rs:82`). A Swift/SwiftUI user who enables `[verify] mode=replay` **or**
`[tdd] mode=replay` and does not override the allowlist has `swift test` / `xcodebuild` / `xcrun`
**silently rejected → Indeterminate** (`verify.rs:310`) — the gate can never establish red/green. It is
technically overridable via `[verify] allowed_command_prefixes` (`config.rs:282-290`), but: (a) the
default ships cargo+just, not a neutral set; (b) a non-Rust user must *know* to override it; (c) there is
**no `[tdd]`-scoped allowlist** — TDD-replay borrows verify's list, a cross-gate coupling wart.
**Verdict: this is the one real, enforcement-backed portability tax — and it was absent from the
original slice pitch.**

### Secondary (P1) — `parse_test_count` cargo+pytest only

Same recognizer as field 2; only bites when `require_test_count=true` (off by default). Escape hatch
exists. Polish, not a wall.

## Conclusion — the slice premise is partially falsified

Of the four pitched fields, **two have no/redundant consumers** (`fmt_command`/`lint_command`,
`test_success_markers`), **one parameterizes an already-polyglot classifier** (`test_globs`), and the
**actual monoculture tax with real enforcement** (the cargo-centric replay allowlist + missing
`[tdd]` knob) **was not in the pitch at all**. Building the pitched layer verbatim would add speculative
config surface; the high-value, surgical, evidence-backed work is the replay-allowlist portability fix.

## Open decision (carried to design — genuinely the maintainer's call)

The finding overturns the slice as specified, so scope is a decision, not a default: build only the P0
replay-allowlist fix; P0 + `test_globs`; the full pitched layer (accepting the speculative surface); or
defer slice #3 and record this falsification. Surfaced to the maintainer before any code.
