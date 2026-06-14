# Research: Scan `tamper-security-wiring` false-positive

- **Date:** 2026-06-14
- **Feature slug:** scan-tamper-false-positive

## Sub-questions

1. What exactly does the over-blocking rule match, and where is it?
2. What are the concrete false-positive classes?
3. What is the regression boundary — which true-positives must keep blocking?
4. What implementation constraints bound the fix?

## Findings (cited; verified against the working tree)

### 1. The rule

`security/rules.toml:165-169`, rule `tamper-security-wiring` (`kind = "command"`, `severity = "block"`):

```
pattern = '(>>?|\btee\b|\bcp\b|\bmv\b|\bln\b|\bchmod\b|\brm\b|\bdd\b|\binstall\b|\btruncate\b|sed\s+-[A-Za-z]*i)[^\n]*(\.git/hooks/|hooks/(pre-commit|security-scan)\.sh|security/rules\.toml|gatekeeper/(src/|Cargo\.)|\.claude/settings)'
```

A sibling rule `tamper-memory-artifacts` (`security/rules.toml:178-183`) shares the identical
`(>>?|\btee\b|…)` prefix, differing only in the protected-path group
(`docs/memory/|\.claude/topology/memory/`). **Both rules carry the same bug.**

The structure is `(verb-or-redirect)[^\n]*(protected-token)`: the mutation signal need only appear
*somewhere before* a protected token on the line, not as the operation actually targeting it.

### 2. False-positive classes

- **Verb-as-search-string.** `grep -n "tee" security/rules.toml` matches `\btee\b … security/rules\.toml`;
  `grep -rn install gatekeeper/src/main.rs` matches `\binstall\b … gatekeeper/(src/)`. The verb word is
  grep's *pattern/argument*, not a command.
- **Co-occurring redirect.** A bare `>`/`>>`/`2>` on a line that also names a protected token matches
  `>>? … <protected>` even when the redirect target is unrelated (e.g. `/dev/null`).

Reproduced live: during the prior design study, three read-only `grep` commands auditing this subsystem
were vetoed as `tamper-security-wiring`. Same coarse token-boundary class as open issue **F-001**
(there it fails *open*; here it fails *closed*).

### 3. Regression boundary (must keep blocking)

`gatekeeper/tests/cli_scan.rs`:

- `real_ruleset_blocks_bash_tampering_with_wiring` (`cli_scan.rs:614-649`): asserts block for
  `echo 'exit 0' > .git/hooks/pre-commit`, `rm .git/hooks/pre-commit`, `cp /dev/null hooks/pre-commit.sh`,
  `chmod -x .git/hooks/pre-commit`, `:> security/rules.toml`; and allow for `cat .git/hooks/pre-commit`,
  `rm -rf node_modules`.
- `real_ruleset_blocks_bash_writes_into_memory_artifacts` (`cli_scan.rs:651-707`): asserts block for
  `>`, `cp`, `tee`, `mv` writes into `docs/memory/` and `.claude/topology/memory/`; allow for
  `cat docs/memory/…` and `rm -rf node_modules`.

**Key observation:** every true-positive *redirect* assertion targets the protected path directly
(`> .git/hooks/pre-commit`, `:> security/rules.toml`, `echo body > docs/memory/x`). So a redirect rule
bound to a protected *target* preserves all of them.

Tests drive the **shipped** rules via `real_rules_toml()` (`cli_scan.rs:292`) copied into a scratch
root, exercised through `run()` (`cli_scan.rs:57`) → `gatekeeper scan --cmd <s>` (exit 1 = block).

### 4. Implementation constraints

- **Rust `regex` crate has no lookaround/backreferences** — the fix must be a plain regex; a negative
  lookbehind ("verb not inside quotes") is not available.
- The rules file is **protected** (`security/rules.toml` ∈ `[integrity].protected_paths`,
  `security/rules.toml:204-205`) — edits route through the human `ask` guard, and the commit needs an
  authorized `--no-verify`.
- Per three-language-lanes, a data-only change to `rules.toml` is preferable to moving logic into
  `scan.rs`.

## Top-risk verification

Cross-checked the regression claim by reading `cli_scan.rs:614-707` directly: confirmed all
true-positive redirect cases target the protected path, so redirect-target binding cannot regress them.
Confirmed `regex` crate usage in the scanner does not rely on lookaround (the existing patterns are all
plain regex).

## Open unknowns (carried to design)

- The precise command-position anchor set (which `sudo`/`env`/`xargs`/`$()` wrappers to include).
- Whether to also tighten `tamper-memory-artifacts` in the same change (recommended: yes, shared shape).
