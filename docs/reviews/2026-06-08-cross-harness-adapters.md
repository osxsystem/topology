VERDICT: pass
HEAD: 13ecbe122e17b4a94e052086c372a16c9e6a239e
BASE: e9926c332cb4e0e117d5893cae8afbb11c0dc77e

# Review: cross-harness-adapters (2026-06-08)

Reviewed by an independent fresh-context critic (a different model — Sonnet — per the code-review skill),
auditing `git diff origin/main...HEAD` against the spec, plan, and repo standards. Tooling-enforced
checks (fmt/clippy/tests) were skipped by design — the finish gate covers them.

## Blocking findings
None.

## Non-blocking notes
- `instinct.rs:308` — the plan (Task 2a) called for a dedicated unit test of `instincts_for_adapt`; it has none, though it is exercised transitively by the `adapt::tests` that call `build_cursor`/`build_opencode`.
- `adapt.rs:96` — `read_skill` does not validate the frontmatter `name` (no equivalent of `instinct::validate_id`); a hostile `name` like `../x` would steer the output path. Latent only — operator content is trusted, and all nine current skill names are safe kebab identifiers.
- `adapt.rs:114` — `description` defaults to an empty string when the key is absent, which would yield a Cursor Agent-Requested rule with an empty `description:`. Latent — every current skill carries a description.

## Criteria checked
### Spec/plan
- AC1 (Codex config loads under `--strict-config`; no denylisted key) — `adapt.rs:31` emits only `project_doc_max_bytes = 1048576`; the unit test at `adapt.rs:383` filters to non-comment assignment lines and asserts exactly that; the verify note shows `codex 0.137.0` parsing it clean. "profile" appears only in a TOML comment.
- AC2 (Cursor: instincts Always, contract Always, skills Agent-Requested with no globs) — `adapt.rs:199` (`build_cursor`); `mdc()` at `adapt.rs:177` omits `globs`; asserted at `adapt.rs:398` and on-disk at `cli_adapt.rs:61`.
- AC3 (OpenCode: valid-JSON `opencode.json` with schema URL + `instructions` incl. `AGENTS.md`; verbatim skill copy; `instincts.md`) — `adapt.rs:232` (`build_opencode`); `serde_json` guarantees valid JSON; `adapt.rs:422`, `cli_adapt.rs:79`, and the verify note's `python3 json.load`.
- AC4 (Claude: valid-JSON `.claude/settings.json` wiring both hooks) — `adapt.rs:263` (`build_claude`); both `UserPromptSubmit` and `PreToolUse` with the `Bash|Write|Edit|MultiEdit` matcher; `cli_adapt.rs:94`.
- AC5 (re-run then `--check` exits 0; mutate then `--check` exits 1) — `apply_or_check` at `adapt.rs:42`; `adapt.rs:450`; `cli_adapt.rs:133`.
- AC6 (unknown/missing `--harness` or missing `AGENTS.md` exits 2; suite green; clippy/fmt clean) — `cmd_adapt` at `adapt.rs:286`; `require_agents_md` at `adapt.rs:146`; `adapt.rs:461`, `cli_adapt.rs:106`; verify note records 141 passed (165 post-merge), 0 warnings, fmt clean.

### Standards
- ADR-0008 §2 / no new Cargo dependencies — `gatekeeper/Cargo.toml` and `Cargo.lock` are unchanged on the branch (`git diff origin/main -- gatekeeper/Cargo.toml gatekeeper/Cargo.lock` is empty); `adapt.rs:1` documents "std + the existing `serde_json` only; no new crates."
- ADR-0008 §1 / pure builders, no templating engine — each harness is a `fn(root) -> Result<Vec<GenFile>, String>`; one `apply_or_check` does all I/O; no external template files.
- ADR-0003 / one Markdown source, outputs are build artifacts — all four builders read only from `AGENTS.md` / `skills/` / `instincts/`; `adapters/README.md:19` states outputs are never hand-edited.
- AGENTS.md three-language lanes — `adapt.rs` is Rust only; skills/instincts stay Markdown; `scripts/install.sh` stays Bash. No lane crossings.
- AGENTS.md surgical-changes-only — 1234 insertions, 16 deletions; all 16 deletions are the Phase 4 status update in `docs/ROADMAP.md`; `main.rs` adds 4 lines, `instinct.rs` adds 15. No adjacent code altered.
- AGENTS.md alphabetical `mod` ordering / clean Phase 3 merge — `main.rs` declares `adapt, instinct, learn, review, scan` in order and wires both the `adapt` and `learn` subcommands after the merge.
- Plan test strategy — `#[cfg(test)] mod` unit tests in `adapt.rs` plus `std::process::Command` + `env!("CARGO_BIN_EXE_gatekeeper")` integration tests in `gatekeeper/tests/cli_adapt.rs`; no `assert_cmd`/`predicates`, mirroring `cli_instinct.rs`.
