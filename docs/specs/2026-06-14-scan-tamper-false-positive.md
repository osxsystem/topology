# Design: Scan `tamper-security-wiring` false-positive on read-only inspection

- **Date:** 2026-06-14
- **Feature slug:** scan-tamper-false-positive
- **Status:** approved
- **Approved by:** maintainer (Do Viet Hung), 2026-06-14 — wrapper set `sudo`/`env`/`xargs` accepted as proposed

## Problem

The deterministic security floor over-blocks. The two command rules in `security/rules.toml`
that guard the security wiring —

- `tamper-security-wiring` (`security/rules.toml:169`)
- `tamper-memory-artifacts` (`security/rules.toml:183`)

— both share the prefix:

```
(>>?|\btee\b|\bcp\b|\bmv\b|\bln\b|\bchmod\b|\brm\b|\bdd\b|\binstall\b|\btruncate\b|sed\s+-[A-Za-z]*i)[^\n]*(<protected-path-token>)
```

This fires `block` whenever a mutating verb **or a bare `>`/`>>` redirect** appears *anywhere*
before a protected-path token — regardless of whether the command actually writes the protected
path. Two benign read-only patterns are therefore vetoed:

1. **Verb-as-search-string.** `grep -n "tee" security/rules.toml` matches `\btee\b … security/rules\.toml`
   — the word `tee` is grep's *pattern*, not a command. Likewise `grep -rn install gatekeeper/src/main.rs`
   (the word `install`).
2. **Co-occurring redirect.** A `>`/`2>`/`>/dev/null` redirect on the same line as a protected-path
   token matches `>>? … <protected>` even though the redirect target is not the protected file.

**Evidence (reproduced live).** During the design study, three read-only `grep` commands auditing
this very subsystem were vetoed as `tamper-security-wiring`. The failure mode is self-defeating:
you cannot `grep` the security scanner's own rules or source without the scanner blocking you.
This is the same coarse-boundary disease class as the open issue **F-001** (quoted-token fail-open),
applied here to fail *closed* (over-block).

Success: a read-only command that merely *names* or *reads* a protected path is allowed, while every
command that actually *writes* a protected path stays blocked. No false-negative is introduced on the
security floor.

## Constraints

- **Rust `regex` crate: no lookaround, no backreferences.** The fix must be expressible as a plain
  regular expression. Negative-lookbehind approaches ("verb not preceded by a quote") are unavailable.
- **Three-language lanes.** Prefer a data-only change in `security/rules.toml` (Markdown/data is the
  source of truth); do not move enforcement logic into a new lane unless required.
- **Security floor — false-negatives are the expensive kind.** Tightening the rule must not let any
  real write to a protected path through. Every existing true-positive assertion in
  `real_ruleset_blocks_bash_tampering_with_wiring` and
  `real_ruleset_blocks_bash_writes_into_memory_artifacts` must stay `block`.
- **Heuristic posture is unchanged.** Both rules are already documented as "raises, does not close"
  the residual (variable-built paths, `eval`, interpreter writes still evade). This change keeps that
  posture; it does not claim to close it.
- **Non-goals.** Not rewriting the scanner; not touching `scan.rs`; not addressing F-001 itself (a
  separate slice); not changing which paths are protected.

## Approaches considered

1. **Precision-tighten both rules in-place (regex-only) — RECOMMENDED.**
   Two changes to the shared prefix, applied identically to both rules:
   - **Redirect → target-bound.** Replace the bare `>>?` branch with `>>?\s*<protected>`: a redirect
     counts only when its *target* is a protected path. This drops the entire `2>/dev/null` /
     `>/dev/null` co-occurrence class, and is *more* precise on real writes (e.g. `cmd 2> security/rules.toml`,
     a stderr redirect that overwrites the rules file, is still caught).
   - **Verbs → command-position.** Require the mutating verb to sit at a command boundary —
     start-of-string or immediately after a separator (`;`, `|`, `&`, `&&`, `||`, `(`, `` ` ``, `$(`),
     with optional `sudo`/`env`/`xargs` wrappers — so a verb word appearing as a *search pattern* or a
     bare *argument* (`grep -n tee file`, `grep "tee" file`) no longer fires. A verb preceded only by a
     space (an argument) is not command position.

   Trade-offs: pure data change, lowest blast radius, stays in the Markdown/data lane. Command-position
   anchoring is itself a heuristic — obscure evasions (`eval "tee …"`, built paths) remain, but those
   are the *already-documented, unchanged* residual.

2. **Redirect-only fix (conservative minimum).** Apply only the target-bound redirect change; leave the
   verb branch matching anywhere. Trade-offs: smaller diff, but **under-fixes** the reported problem —
   `grep "tee" security/rules.toml` (verb-as-search-string) would still block. Rejected: it doesn't
   resolve the live symptom.

3. **Move detection into `scan.rs` (argument-aware parsing).** Tokenize the command, identify the verb
   and its operands / redirect targets, test those against the protected set. Trade-offs: robust and
   closes more of the residual, but it is Rust logic in a *protected* file (`scan.rs`), a much larger
   blast radius and review surface — over-engineered for this slice, and it violates
   "weakest-enforcement-that-works." Deferred; noted as the likely long-term answer to the broader
   F-001 token-boundary class, not this fix.

## Decision

**Approach 1.** It resolves both false-positive classes, keeps every existing true-positive blocking,
adds precision on real writes, stays a data-only change in the correct lane, and preserves the
documented heuristic posture. Both `tamper-security-wiring` and `tamper-memory-artifacts` receive the
identical treatment so they cannot drift.

## Risks & open questions

- **Risk: a tightening opens a hole.** A wrong anchor could let a real write slip (false-negative on the
  security floor). Mitigation: TDD with paired cases — every existing block-case stays `block`, plus new
  block-cases for the precise forms (`cmd 2> security/rules.toml`, `… | tee gatekeeper/src/scan.rs`,
  `sudo tee …`) — watched red→green, and a fresh-context review specifically tasked to find a bypass.
- **Open question (for approval): how many command-position wrappers?** `sudo`/`env`/`xargs` cover the
  realistic privileged/batched writes; `eval`/backtick-built paths stay in the residual. Is that the
  right line, or should the wrapper set be wider/narrower?
- **Residual unchanged:** indirectly-built paths and interpreter writes (`python -c "open(...)"`) still
  evade both rules, exactly as today. This slice does not claim to close that.

## Acceptance criteria

Driven against the **shipped** `security/rules.toml` via the `cli_scan.rs` `real_ruleset_*` harness
(`gatekeeper scan --cmd <string>` → exit 0 = allow, exit 1 = block):

- **AC1 — read-only inspection is allowed (the bug, red→green):**
  - `grep -n "tee" security/rules.toml` → exit 0
  - `grep -rn install gatekeeper/src/main.rs` → exit 0
  - `grep -rn "fn scan" gatekeeper/src/scan.rs 2>/dev/null` → exit 0
- **AC2 — real writes still block (no regression):** every assertion in
  `real_ruleset_blocks_bash_tampering_with_wiring` and
  `real_ruleset_blocks_bash_writes_into_memory_artifacts` remains exit 1, plus new precise block-cases:
  - `echo x 2> security/rules.toml` → exit 1 (fd-redirect overwriting the rules file)
  - `cat x | tee gatekeeper/src/scan.rs` → exit 1 (piped `tee` write)
  - `sudo tee security/rules.toml < /tmp/x` → exit 1 (privileged write)
- **AC3 — suite & lint clean:** `cargo test` (full suite) green; `cargo fmt --check` and
  `cargo clippy -- -D warnings` clean.
