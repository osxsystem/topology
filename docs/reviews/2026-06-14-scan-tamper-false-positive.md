VERDICT: pass
HEAD: 83089c2ee93e1a73524b9391b1679cf634926da0
BASE: c00fc03c1d91d43d454fb0e97cd202b7e7dfa225

# Review: scan-tamper-false-positive (2026-06-14)

Fresh-context critic (no memory of authoring), eighth and final round, scoped to the floor's documented
threat model ("mistakes, not a determined evader"). Diff reviewed: `git diff c00fc03…83089c2`
(`gatekeeper/src/scan.rs`, `security/rules.toml`, `gatekeeper/tests/cli_scan.rs`, plus the
research/design/plan/verify artifacts). Earlier rounds (1–7) each FAILED and drove out a distinct real
bypass; this round found none in scope.

## Blocking findings
None.

The mistake-class write surface is comprehensively handled: every verb in the A3 set
(`tee cp mv ln chmod rm dd install truncate` + `sed -i`) and every redirect form blocks across all
reachable command positions (`&&`/`||`, subshells, brace groups, backticks, newlines, `if/then`,
`for/do`, `case )`, `!`, no-space redirect targets, fd-redirects, pipes, process-sub incl. nested and
quoted-`)`, flag-carried targets `cp -tDIR`/`--target-directory=`, `dd of=`, `tee` multi-target,
path-qualified verbs `/bin/cp`/`./rm`, and `//`/`./`/`../` normalization). No new false-positive: all
read/inspection verbs and verb-as-grep-pattern forms are allowed. All 14 `real_ruleset_*` floor tests
pass — no regression of an existing block-case.

## Non-blocking notes
- Substring `.contains()` over-blocks suffixed paths (`rm security/rules.toml.bak`) — pre-existing
  fail-closed behavior, identical under the old regex; not a new FP. The tokenizer is strictly more precise.
- Conservative read-source over-block (`cp security/rules.toml /tmp/backup` blocks) — explicitly
  documented and accepted in the spec's Residual section.
- Verbs outside the A3 set (`touch`, `mkdir`, `patch`, `git checkout -- <path>`) are not detected — not a
  regression (the old regex didn't catch them either); a possible future widening, flagged for the maintainer.
- Runtime-resolved forms (variable/command-sub/glob/brace/tilde/`eval`/interpreter) and arg-taking
  wrappers (`timeout 5 …`) remain allowed — documented out-of-scope residual.
- Deliberate adversarial shell obfuscation is out of the floor's threat model (an agent with a shell can
  `git commit --no-verify` regardless) — documented, not a defect.

## Criteria checked
### Spec/plan
- **A3-1 (reads allowed):** PASS — `grep "tee" security/rules.toml`, `cat docs/memory/h.md`,
  `command grep tee …` all exit 0; the originally-reported false-positive is fixed at the root.
- **A3-2 (writes block, no FN regression):** PASS — all keyword/`)`/`!`/clobber/wrapper/process-sub/
  flag-carried/path-qualified mistake-class forms block; 14/14 `real_ruleset_*` floor tests green.
- **A3-3 (quote-awareness):** PASS — verb-as-quoted-argument allowed; quoted verb in command position blocks.
- **A3-4 (path-normalization):** PASS — `//`, `./`, interior `/./` and `/../` all resolve and block.
- **A3-5 (residual documented, not closed):** PASS — runtime forms allowed, asserted with comments.
- **A3-6 (suite & lint clean):** PASS — `cargo test` 579 passed/0 failed; `cargo fmt --check` clean;
  `cargo clippy --all-targets -- -D warnings` clean.
### Standards
- **Three-language lanes (AGENTS.md / DEVELOPMENT.md):** PASS — detection logic (lexer, verb/wrapper/
  keyword sets, normalizer) is Rust in `scan.rs`; `security/rules.toml` carries only the `protected`
  token lists for `kind = "path-mutation"` (no regex `pattern`). Shell grammar correctly lives in Rust.
- **Surgical / diff-traceability:** PASS — the diff replaces exactly the two regex rules with the
  tokenizer + adds the A3 regression guards and artifacts; no unrelated churn.
- **Simplicity:** PASS — one lexer + one `detect_path_mutation` walk; no speculative config knobs. The
  conservative-over-block decision (no per-verb argument semantics) is explicitly justified in the spec
  as avoiding bug surface for one uncommon false-positive.
