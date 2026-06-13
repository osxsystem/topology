VERDICT: pass
HEAD: a09c143c031f8a4f4c364c53f01efaa3e9ee0cd5
BASE: 7796b261c0f2fff54d8aa756045e4ea0a64bb4c0

# Review: adapt-cross-tree (2026-06-13)

## Blocking findings
None.

## Non-blocking notes
- None remaining. The follow-up note from the prior fresh-context review (parameter name `in_framework` no longer described the sibling-clone case) is resolved by this commit's `in_framework → portable` rename and corrected doc comments.

## Criteria checked
### Spec/plan
- Re-binding scope (delta only) — `git diff 5e6f3e5..a09c143` touches one file, `gatekeeper/src/adapt.rs` (+8 -10), and `git diff -w` is identical to the full diff (no whitespace-hidden change). The delta is exactly: (a) parameter `in_framework → portable` on `build_claude_hooks` and its single in-closure `if` use; (b) doc-comment rewrite on `build_claude_hooks`; (c) doc-comment `in_framework → use_portable` on `build_claude`. No logic, control-flow, signature-beyond-the-param-name, or test changes — the only `fn` line in the diff is `build_claude_hooks` itself; no `#[test]` lines appear.
- Portable-path selection intact (cmd_adapt) — `adapt.rs:856` `let use_portable = !roots_differ || project_has_root_hooks(write_root);` drives both `build_claude_hooks(read_root, use_portable)` (`:864`) and `bin_opt` (`:876` `if use_portable`). Cross-tree dogfood predicate unchanged.
- Cross-tree predicate unchanged — `project_has_root_hooks` (`:526`) returns true only when BOTH root hook files (`hooks/skill-activation.sh` AND the security-scan hook) exist on `write_root`; the `&&` is preserved.
- Governed/acceptance coverage unweakened — `build_claude_hooks_governed_uses_absolute` (`:1129`), `claude_wires_both_hooks_governed` (`:1155`), the dogfood-portable cases (`:1139`, `:1175`), and the #54 predicate tests (`:1581` true-for-clone, `:1595` false-without, `:1602` false-with-partial M1 guard) are all present and untouched by the delta.

### Standards
- fmt clean — `env -u GATEKEEPER_BIN -u TOPOLOGY_ROOT cargo fmt --check --manifest-path gatekeeper/Cargo.toml` exits 0.
- clippy clean — `cargo clippy --all-targets -- -D warnings` exits 0 (no warnings/errors).
- tests green — `cargo test --manifest-path gatekeeper/Cargo.toml` exits 0: all suites pass (lib 289, integration suites all 0 failed; total 0 failed, 5 ignored).
- Rename is non-semantic — `portable` is a positional bool argument; renaming the parameter and the in-function `if` binding cannot change behavior, and the call sites pass the value positionally (`use_portable`, and literal `true`/`false` in tests). Diff traceability holds: every changed line serves the "rename + doc-comment correction" follow-up note and nothing else.
