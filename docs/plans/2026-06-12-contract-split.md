# Plan — contract split (Phase 10)

Executes the [spec](../specs/2026-06-12-contract-split.md); grounding in the
[research note](../research/2026-06-12-contract-split.md) and ROADMAP Phase 10.
Branch: `feat/contract-split` (worktree `topology-phase10`). Coding delegated to a Sonnet
subagent; the main loop plans and reviews. Protected/sensitive paths this phase:
`gatekeeper/src/main.rs` (if touched), `scripts/build-payload.sh` is not protected but
`AGENTS.md` edits may trip the scan hook — any blocked commit carries the documented
`--no-verify` override per the Track 2 grant.

| # | Task | Files | Acceptance |
|---|------|-------|------------|
| 1 | Red fixtures commit (test-only, precedes all production edits — tdd gate counts only `gatekeeper/tests/`-style paths): integration test `gatekeeper/tests/cli_contract_render.rs`, `#[ignore]`-tagged, calling the binary's new surface `adapt --contract <framework\|project>` in a tempdir fixture: framework render contains `docs/` + zero `.claude/topology`; project render the inverse; unknown-placeholder template → exit 2 + stderr names the placeholder; `AGENTS.md` byte-equality with `adapt --contract framework` output. Plus an `#[ignore]` shell assertion block in `scripts/test-build-payload.sh` behind `PHASE10_RED=1`: template present, `docs/DEVELOPMENT.md` absent, no `docs/`-rooted artifact paths under payload `skills/` | `gatekeeper/tests/cli_contract_render.rs`, `scripts/test-build-payload.sh` | force-run fixtures fail against current code; default suites stay green |
| 2 | Template + renderer (spec §1–2): `templates/CONTRACT.template.md` (six portable sections, three placeholders); `render_contract` + `ContractCtx` in `adapt.rs`, fail-closed both directions; `adapt --contract <framework\|project>` prints the render (stdout, exit 0 / stderr + exit 2); unit tests (AC-2, AC-3); un-ignore the task-1 render fixtures | `templates/CONTRACT.template.md`, `gatekeeper/src/adapt.rs`, `gatekeeper/tests/cli_contract_render.rs` | AC-1/2/3 tests green; clippy/fmt clean |
| 3 | Dogfooding + dev doc (spec §3–4): `docs/DEVELOPMENT.md` gets the two dev sections; `AGENTS.md` regenerated = framework render + dev-doc trailer (trailer constant in `adapt.rs`, appended by `--contract framework`); un-ignore the byte-equality fixture (AC-4, AC-5); root-marker tests stay green | `AGENTS.md`, `docs/DEVELOPMENT.md`, `gatekeeper/src/adapt.rs` | AC-4 byte-equality test green; `cargo test --test cli_root_markers` green; AC-5 grep |
| 4 | Skill wording sweep (spec §5): seven skills + `write-plan/references/plan-template.md` switch to `<artifacts-root>/…` phrasing with the one-line definition parenthetical | `skills/{research-first,brainstorm-design,write-plan,verify-before-done,code-review,finish-branch,resume}/SKILL.md`, `skills/write-plan/references/plan-template.md` | AC-6 grep clean repo-side; skill-rules.json untouched; `check docs` green |
| 5 | Payload manifest (spec §6): `build-payload.sh` ships `templates/`; `test-build-payload.sh` un-guards the task-1 assertions (template present, dev doc absent, no `CONTRACT.md`, AC-6 grep against the staged payload) | `scripts/build-payload.sh`, `scripts/test-build-payload.sh` | AC-7; `just test-payload` green; `just test-e2e` regression net green |
| 6 | ADR + docs (spec §7): `docs/adr/0016-contract-split.md`; README index row; CHANGELOG entry; version bump `0.8.0` in Cargo.toml + Cargo.lock | `docs/adr/0016-contract-split.md`, `docs/adr/README.md`, `CHANGELOG.md`, `gatekeeper/Cargo.toml`, `gatekeeper/Cargo.lock` | AC-8; `gatekeeper check docs` ok; AC-9 |
| 7 | Close-out (main loop, not the subagent): verify artifact (static + `GATEKEEPER_SHADOW=replay`), `just check`, review artifact as branch tip, PR for human merge | `docs/verify/…`, `docs/reviews/…` | all gates green; PR open |

Task-2 surface note: `adapt --contract <framework|project>` is the spec'd CLI surface
(spec §2) — print-only, no file writes, no `GenFile`/`--check` integration this phase
(that arrives with Phase 9 delivery). The fixture template for the fail-closed test is a
tempdir framework root with a deliberately bad template; the real template path resolves
relative to the framework root (`templates/CONTRACT.template.md`), so tests point
`TOPOLOGY_ROOT` at the fixture.

Commit-order constraints: task 1 precedes tasks 2–6 (tdd gate). Task 3 depends on task 2
(renderer exists). Task 4 is independent after task 1 but ships before task 5 (the payload
grep assertion must see the cleaned skills). Task 7's review artifact is the branch tip.

Risks to watch in implementation review: byte-equality AGENTS.md test is brittle to
line-ending/trailing-newline drift — normalize once, assert exactly; the skill sweep must
not change skill names/descriptions (skill-rules.json routing and `cli_doc_sync` depend on
them); `AGENTS.md` restructuring must keep the file non-empty at repo root (root marker,
`require_agents_md`, codex auto-discovery); the payload grep (AC-6) must not false-positive
on the gatekeeper FAIL-message examples inside skills (e.g. `resume/SKILL.md` quotes gate
output) — the assertion targets `docs/<kind>/` artifact-path literals, and quoted gate
output prints absolute resolved paths, not `docs/`-rooted literals, so exclude nothing but
verify this during implementation.

Test baseline at plan time: `just check` green on `main` @ `a61b0a8` (v0.7.0, 461 passed /
6 ignored), `just test-payload` 26/0, `just test-e2e` 53/0.

Out of scope (spec): contract delivery/injection (`@.topology/CONTRACT.md`, managed blocks,
`GATEKEEPER_BIN` wiring) — Phase 9; `build_cursor`/`build_opencode` governed-mode switch —
Phase 9; gate-logic changes — none needed.
