VERDICT: pass
HEAD: 35951923932d7e43826db0099663d35113f845b9
BASE: 12a3d19ebf7595c1506e02d65b27241e7a0549ea

# Review: path-routing Slice 1 (2026-06-14)

A fresh-context critic (no memory of authoring) reviewed the branch diff. Verdict pass, no blocking findings, four nonblocking nits. Nit #3 (the parity tripwire pinned only 4 cases) was acted on — the `path_glob_match_parity` test now pins the load-bearing edge cases (trailing-`/` dir match + sibling/nested non-match, exact-glob negative, middle-`*` anchoring) so the R3 drift guard vs `scan.rs:498-527` can actually catch a regression. The remaining nits are inherited/cosmetic and recorded below.

## Blocking findings
None.

## Non-blocking notes
- `gatekeeper/src/main.rs` `cmd_route` — a `--paths` value beginning with `-` (e.g. `-weird.txt`) is misread as an unknown flag → exit 2. Rare for real paths; consistent with other subcommands' arg handling.
- `gatekeeper/src/route.rs:76` — `route_by_paths` calls `.dedup()` where the mirrored `route()` does not; skill names are unique JSON keys so it is a harmless no-op.
- `gatekeeper/src/route.rs` `path_glob_match` byte-slices `path[pos..]`, which could panic on a multibyte path at a non-char boundary — inherited verbatim from `scan.rs:498-527` (same risk in production today); paths are near-always ASCII. Not a new defect.
- Nit #3 (parity test breadth) — **resolved** in commit `3595192` (edge cases added).

## Criteria checked
### Spec/plan
- `pathTriggers` schema added; keyword routing unchanged — satisfied. `hooks/skill-rules.json` adds `pathTriggers.globs` to `security-scanning`; `route()` untouched; full suite green confirms back-compat.
- `route --paths`/`--staged-paths` print routed skills (activate grammar); unknown flag → 2; `--help` → 0 — satisfied. `cmd_route` reuses `check_help_or_unknown`; output matches `cmd_activate`; `cli_route.rs` (4 tests) asserts each and passes.
- New unprotected `route.rs` with parity-tested glob; scanner untouched — satisfied (`route.rs` new; `scan.rs` not in diff; design D1 honored).
- `cli_doc_sync` green; no new deps — satisfied (`USER-GUIDE.md` rows added; `Cargo.toml`/`lock` not in diff).
- Slices 2 (PostToolUse hook) & 3 (eval harness) — correctly deferred per plan; out of scope.

### Standards
- three-language-lanes — conforms: matching logic in Rust (`route.rs`); `skill-rules.json` is pure data; Slice 1 ships no bash.
- no-deps (ADR-0007) — conforms: reuses dep-free glob + serde_json; `Cargo.toml`/`lock` unchanged.
- surgical — conforms: `main.rs` gains only `mod route;` + one `SubcommandSpec` + `cmd_route`; no drive-by edits.
- advisory-not-blocking — conforms: `cmd_route` returns 0 on match and no-match; nonzero only on bad flags (2) or git/JSON failure (1); flips no default, blocks no tool call.
- scan.rs-untouched — conforms: the protected scanner is not in the diff (design D1 / approach 1).
