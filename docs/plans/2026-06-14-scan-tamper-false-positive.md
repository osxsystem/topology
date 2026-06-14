# Plan: Scan tamper false-positive — Approach 3 (tokenized detection)

- **Date:** 2026-06-14
- **Feature slug:** scan-tamper-false-positive
- **Design:** `docs/specs/2026-06-14-scan-tamper-false-positive.md` (Approach 3, approved)
- **Supersedes** the Approach-1 plan (regex). Approach-1 working-tree regex in `rules.toml` is replaced here.

## Baseline

`env -u TOPOLOGY_ROOT cargo test --quiet --manifest-path gatekeeper/Cargo.toml --test cli_scan` → 45 passed
(with the Approach-1 widened regex). The new A3 tests below fail against that regex (RED), then pass once
the tokenizer lands (GREEN).

## Integration points (verified, `gatekeeper/src/scan.rs`)

- `Kind` enum `:75-91` (`#[serde(rename_all="lowercase")]`, `as_str`). Add `PathMutation` → `"path-mutation"`.
- `RawRule` `:56-73` (`#[serde(deny_unknown_fields)]`, optional fields `#[serde(default)]`). Add
  `#[serde(default)] protected: Option<Vec<String>>`.
- `parse_rules` dispatch `:257-297` (`match r.kind`). Add a `Kind::PathMutation` arm building a
  `CompiledPathMutationRule { id, severity, description, protected: Vec<String> }`.
- `Rules` struct `:202-221`. Add `path_mutation: Vec<CompiledPathMutationRule>`.
- `scan_with`→`Finding` `:406-435`; `Finding` `:345-352`; `report` `:529-550` (the `BLOCK <id>: <desc>
  [<loc>] (redacted: …)` line). The detector returns `Vec<Finding>` in the same shape.
- `--cmd` path `scan_cmd_cmd` `:693-721` and hook Bash path `:1327-1351`: both call `scan_with(command…)`.
  Add a sibling call to the new detector on the same `cmd`/`joined` bytes, extending `findings`.

## Tasks

### Task 1 — RED: A3 acceptance tests (main loop, this commit)
Append `real_ruleset_tokenizes_command_structure` to `gatekeeper/tests/cli_scan.rs` (drives the shipped
rules via `real_rules_toml()`), asserting (exit 1 = block, 0 = allow):
- BLOCK (currently bypass the widened regex): `if true; then cp /tmp/x security/rules.toml; fi`;
  `for f in a; do rm gatekeeper/src/scan.rs; done`; `case $x in y) cp /tmp/z security/rules.toml ;; esac`;
  `! tee security/rules.toml < /tmp/x`; `cp /tmp/x security//rules.toml` (path-normalization).
- ALLOW (quote-aware + residual): `grep "rm -rf x" security/rules.toml`; `grep "fn cp" gatekeeper/src/scan.rs`;
  `d=security/rules.toml; cp /tmp/x $d` (variable-built path — documented residual);
  `cat docs/memory/h.handoff.md`.
- **Run:** `env -u TOPOLOGY_ROOT cargo test --manifest-path gatekeeper/Cargo.toml --test cli_scan real_ruleset_tokenizes`
- **Expect RED:** the five BLOCK cases return 0.

### Task 2 — GREEN: new rule kind (delegated, `scan.rs`)
Add `Kind::PathMutation` (`:75-91`); add `protected: Option<Vec<String>>` to `RawRule` (`:56-73`); add
`CompiledPathMutationRule` and `Rules.path_mutation`; add the `parse_rules` arm (`:257-297`) — error if
`protected` is absent/empty for a `path-mutation` rule (mirror the existing "requires a 'pattern'" error).
- **Run:** `env -u TOPOLOGY_ROOT cargo build --manifest-path gatekeeper/Cargo.toml` → compiles.

### Task 3 — GREEN: the tokenizer + detector (delegated, `scan.rs`)
Add `fn detect_path_mutation(cmd: &[u8], rules: &[CompiledPathMutationRule]) -> Vec<Finding>`:
1. Lex `cmd` into words with quote/escape awareness (single-quote: literal; double-quote: literal except
   the word stays one token; backslash escapes next char), recording redirect operators
   (`>`,`>>`,`>|`,`>&`, fd-prefixed like `2>`) and their immediately-following target word.
2. Split the word stream into simple-commands at unquoted separators: `;`, `&`, `&&`, `||`, `|`, newline,
   `(`, `)`, `{`, `}`, backtick, `$(`.
3. Per simple-command, skip leading prefix tokens: shell keywords
   `{if,then,elif,else,fi,for,while,until,do,done,case,esac,select,in,function,time,coproc,!}`; wrapper
   commands `{sudo,doas,env,command,builtin,exec,nohup,setsid,timeout,nice,ionice,stdbuf,xargs}`;
   `VAR=val` assignments; option flags (`-…`). First remaining word = the **verb**.
4. **Block** (push a `Finding{ rule_id, severity, description, redacted, location: "offset N" }` per the
   rule whose `protected` matched) iff: the verb ∈ `{tee,cp,mv,ln,chmod,rm,dd,install,truncate}` (or `sed`
   with an in-place `-i…` flag) AND any operand word — path-normalized: collapse repeated `/`, strip
   leading `./`, drop surrounding quotes — has a `protected` substring; **OR** any redirect target word
   (same normalization) has a `protected` substring.
5. The mutating-verb / wrapper / keyword sets are `const` arrays in `scan.rs` (shell grammar, universal).
- Add `#[cfg(test)] mod` unit tests for the lexer/normalizer (quote split, `//` collapse, redirect target).
- **Run:** `env -u TOPOLOGY_ROOT cargo test --manifest-path gatekeeper/Cargo.toml --lib scan` → unit tests pass.

### Task 4 — GREEN: wire detector into the two command paths (delegated, `scan.rs`)
In `scan_cmd_cmd` (`:693`) and the Bash arm of `scan_hook` (`:1327`), after the existing
`scan_with(command…)` call, add `findings.extend(detect_path_mutation(&cmd, &rules.path_mutation));`
(and `&joined` in the hook path).
- **Run:** build + `cargo test --test cli_scan` (still uses the OLD regex rules.toml until Task 5).

### Task 5 — GREEN: convert the two rules in `rules.toml` (delegated; PROTECTED file)
Replace `tamper-security-wiring` and `tamper-memory-artifacts` with `kind = "path-mutation"`, dropping
`pattern`, adding `protected = [ … ]`:
- wiring: `protected = [".git/hooks/", "hooks/pre-commit.sh", "hooks/security-scan.sh", "security/rules.toml", "gatekeeper/src/", "gatekeeper/Cargo.", ".claude/settings"]`
- memory: `protected = ["docs/memory/", ".claude/topology/memory/"]`
Update the inline comments to describe the tokenizer + residual.
- **Run:** `env -u TOPOLOGY_ROOT cargo test --manifest-path gatekeeper/Cargo.toml --test cli_scan` →
  **GREEN**: all prior `real_ruleset_*` tests + the eight Approach-1 guards + the new
  `real_ruleset_tokenizes_command_structure` pass; total = 46.

### Task 6 — REFACTOR/lint
`env -u TOPOLOGY_ROOT cargo fmt --manifest-path gatekeeper/Cargo.toml` then `-- --check` clean; and
`cargo clippy --manifest-path gatekeeper/Cargo.toml --all-targets -- -D warnings` clean.

## Commit (one cycle)
Tests + `scan.rs` + `rules.toml` together (TDD cycle). `scan.rs`/`rules.toml` are PROTECTED → commit with
authorized `--no-verify`, documenting the maintainer's full-autonomy grant in the message.

## Verify / review / finish (after the cycle)
- **Verify:** re-run the keyword/`)`/`!`/normalization writes through the built binary (exit 1) + the
  quote-aware reads (exit 0) + full `cli_scan` suite green. Record `docs/verify/…`.
- **Review:** fresh-context critic, tasked to find a bypass of the tokenizer (focus: quote/escape edge
  cases, separators, redirect forms, path normalization). Artifact `docs/reviews/…`.
- **Finish:** `just check` (full suite + fmt + clippy).
