VERDICT: pass
HEAD: 3168693dd3e5aebb7fa38fbf193c6f22efdfd70f
BASE: 2dbd8d19ae66c42fc8fbd138a8c9d5a168c8a7b4

# Review: path-routing Slice 2 — live PostToolUse hook (2026-06-14)

A fresh-context critic (no memory of authoring) reviewed the branch diff and independently verified the tests, the hook mode (`100755`), and that `scan.rs`/`Cargo.*` are absent from the diff. Verdict pass, no blocking findings, three cosmetic/inherited nits.

## Blocking findings
None.

## Non-blocking notes
- `gatekeeper/src/route.rs` `HookEvent`/`ToolInput` omit `#[serde(default)]` (unlike `scan.rs`). Behavior is correct — `Option` fields default to `None` when absent and serde ignores unknown fields — verified by the `no_path`/malformed tests. Purely cosmetic drift from the scanner's style.
- `gatekeeper/src/route.rs` — an empty-string `file_path` passes the `Some(..)` guard and is routed; it matches no glob (harmless/advisory). Not worth a guard.
- `gatekeeper/src/main.rs` `cmd_route` `--hook` — a *malformed* `skill-rules.json` returns exit 1 (a missing file returns empty/exit 0); this mirrors the existing `--paths` branch exactly, and the hook script swallows non-zero with `|| true`, so it never reaches the harness as a block. Consistent parity, not a defect.

## Criteria checked
### Spec/plan
- `route --hook` extracts `file_path` from PostToolUse JSON and routes — satisfied (`route_by_hook_json`; `cli_route::route_hook_routes_from_json` asserts `- security-scanning [require]` for a trigger, no-match line for `README.md`, both exit 0).
- Fail-open, never panics (D2) — satisfied (`let Ok/Some(..) else { Vec::new() }`; malformed JSON + missing path return empty; hook always exits 0).
- PostToolUse wired (matcher `Write|Edit|MultiEdit`, `post-tool-routing.sh`, timeout 30) — satisfied (`adapt.rs build_claude_hooks`; `claude_wires_both_hooks_{in_framework,governed}` assert portable + absolute command forms + matcher, 2 passed).
- New hook executable (`100755`), advisory, mirrors `skill-activation.sh` resolution; `set -uo pipefail` (no `-e`, deliberate) — satisfied.
- Slice-1 behavior preserved — satisfied (`print_routed` is the verbatim extracted print body; bare `route` → exit 2; `--help` → 0; unknown flag → 2).
- Eval harness (Slice 3) — correctly out of scope; plan scopes Slice 2 to Tasks 1-5; verify doc states Slice 3 deferred.

### Standards
- three-language-lanes — JSON parse + path extraction in Rust; hook is thin glue.
- no-deps (ADR-0007) — `Cargo.toml`/`lock` not in diff; reuses serde + dep-free `path_glob_match`.
- surgical — diff reads as exactly the slice; `UserPromptSubmit`/`PreToolUse` entries untouched; no drive-by edits.
- advisory-only — hook always exits 0; `route --hook` returns 0 on every routing outcome; no default flipped, no gate added.
- scan.rs-untouched (D1) — `scan.rs` absent from diff; new code uses its own local deserialize struct.
