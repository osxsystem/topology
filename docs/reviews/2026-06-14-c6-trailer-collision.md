VERDICT: pass
HEAD: 5ab57b73b8dfcc27234166a046c0416b2d29cabb
BASE: d3cb7217bde5c699111193548ecd27be6ed83d06

# Review: c6-trailer-collision (2026-06-14)

Fresh-context critic panel (no memory of authoring), three independent lenses — **correctness**,
**simplicity**, **threat-model** — each inspecting `git diff d3cb721…<head>` of
`gatekeeper/src/doctor.rs` plus the design/plan/verify artifacts. Run in two rounds:

- **Round 1** (commit `fa45318`): no blocking, no major findings. One *minor* (threat-model): the probe
  read `%(trailers:unfold)` while the gate it mirrors reads folded `%(trailers)` (`main.rs:1710`) — safe
  (over-warn only, never under-warn) but an undocumented divergence from "mirrors exactly".
- **Fix:** switched the probe to folded `%(trailers)`, softened the WARN "will FAIL" → "would be read as
  agent self-approval and FAIL", and aligned all three trailer-format mentions in code + design + plan.
- **Round 2** (commit `be7c187`): no blocking, no major, no minor *code* defects; the panel confirmed
  the unfold divergence "IS RESOLVED in the executable path". It surfaced one residual *minor*: a stale
  doc comment at `doctor.rs:820` still said `%(trailers:unfold)` (2 of 3 mentions had been fixed).
- **Final fix** (this `HEAD` `5ab57b7`): corrected that doc comment. The sole delta from the
  round-2-reviewed `be7c187` is this **non-executable one-word doc edit** — `cargo` produces byte-identical
  behavior, and `clippy`/`fmt`/the 6 unit tests + full suite stay green. No further code review required
  for a comment-only correction of the panel's own finding.

## Summary

The probe is correct, surgical, and honestly framed. Independently re-verified by the panel against the
code (not assumed): the pure matcher `approval_trailer_collision` is mechanically identical to the gate
(`main.rs:1731-1759`) — case-insensitive `co-authored-by:` key, value = trimmed remainder after the
15-byte prefix, per-pattern `regex::Regex::new().is_match`, bad-regex-skipped; the 15-byte slice is
panic-proof (exhaustive Unicode scan: no codepoint lowercases into the prefix alphabet, so the original
line's first 15 bytes are guaranteed single-byte ASCII); the probe is advisory-only (returns `()`, call
site at `doctor.rs:419` discards it, cannot reach `failures` — a live `gatekeeper doctor` run emitted the
WARN yet exited 0); the scope gate reads the **resolved** config so it stays `n/a` under the
`status-line` default and auto-activates at the planned `human-commit` flip; folded `%(trailers)` is the
identical rendering the gate reads, so the matchable line set cannot diverge; both sites read the same
`design_agent_trailer_patterns`, so the pattern set cannot drift.

## Blocking findings
None.

## Non-blocking notes
- **Inlined matcher (not a shared helper):** accepted by all three lenses. The only dedup target is the
  gate matcher in **protected** `main.rs` (`security/rules.toml:223`); sharing would force a human
  `--no-verify` on a security file to save ~6 trivial lines — a net simplicity loss. Drift is bounded
  (shared config) and pinned by a unit test, mirroring the `version_skew` pure-fn precedent.
- **Sampling window (last 20 non-merge commits, not THE approval commit):** a deliberate, documented
  policy-sniff (design §"What this check is — and is NOT"), not a per-spec verdict — which is exactly why
  it is a never-escalating WARN. False-negative tail (approval commit older than the window, or
  rule-not-yet-stamped) and false-positive tail (recent agent-paired commits but a solo approval commit)
  are both acceptable for an advisory and acknowledged.
- **`n/a (approval=status-line…)` hardcodes "status-line":** consciously accepted. It is accurate for the
  only non-`human-commit` variant that exists today; future-proofing it for a hypothetical third
  `DesignApproval` variant would trade present informativeness for speculation, and adding such a variant
  would touch this line anyway.
- **Verbose WARN string + the 4-line empty-vs-failure comment:** both load-bearing per all three lenses —
  the WARN remediation text is the probe's entire deliverable, and the comment records the intentional,
  more-correct deviation from the design's first-draft "empty → n/a" prose (the bug the verify gate
  caught and fixed).

## Criteria checked
### Spec/plan
- **Detection strategy = git-history, scope-gated to resolved `human-commit`** (design Decision): PASS —
  verified live: default `status-line` repo prints `n/a`; forced `human-commit` + Claude-trailered commit
  fires the WARN with the live value and `(?i)claude`.
- **Reproduce-then-resolve (verify gate):** PASS — A→WARN, B→n/a, C→ok, D(`copilot[bot]`)→WARN; the
  empty-success→`ok` bug found at the verify gate is fixed and re-confirmed.
- **No protected files touched:** PASS — change confined to unprotected `gatekeeper/src/doctor.rs`; no
  `main.rs`/`scan.rs`/`rules.toml`/hooks/`Cargo.*` edits; committed without `--no-verify`.
- **No new config key:** PASS — reuses typed `design_approval` / `design_agent_trailer_patterns`
  (already in `KNOWN_DESIGN_KEYS`).

### Standards
- **Correctness:** PASS — matcher mirrors the gate line-for-line; panic-proof slice; folded `%(trailers)`
  parses faithfully with `.lines()`; advisory-only contract intact (exit code unaffected).
- **Simplicity:** PASS — one pure fn + one advisory probe + one call site + 6 tests; no dead code, no
  speculative abstraction, no config knob; a staff engineer would approve it as surgical.
- **Threat-model / honesty:** PASS — config-collision *forecast*, not an integrity attestation; `ok`/WARN
  do not over-claim; does not read, satisfy, suppress, or weaken the `approval_provenance` gate;
  cry-wolf risk controlled by `human-commit` scope-gating.
- **Tests / lint:** PASS — full suite 585 passed / 0 failed (+6); `fmt --check` clean; `clippy
  --all-targets -D warnings` clean.
