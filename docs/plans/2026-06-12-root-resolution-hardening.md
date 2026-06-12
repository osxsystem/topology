# Plan — root resolution hardening (Phase 11)

Executes the [spec](../specs/2026-06-12-root-resolution-hardening.md); grounding in the
[research note](../research/2026-06-12-root-resolution-hardening.md) and ROADMAP Phase 11.
Branch: `feat/root-resolution-hardening` (worktree `topology-phase11`). Coding is delegated
to a Sonnet subagent; the main loop plans and reviews. `main.rs` is a protected path — its
commits carry the documented `--no-verify` override per the Track 2 maintainer grant.

| # | Task | Files | Acceptance |
|---|------|-------|------------|
| 1 | Red fixtures commit (test-only, precedes all production edits per the tdd gate): `cli_root_resolution.rs` integration fixtures, `#[ignore]`-tagged so the default suite stays green — (a) hijack-class: marked cwd-ancestor above a plain project resolves it today, must stop; (b) W2: governed project outside `$HOME` falls back to cwd today, must resolve `~/.topology` (via env `HOME` remap); (c) binary-adjacent: binary copied to `<root>/bin/gatekeeper` resolves `<root>` from an unrelated cwd; (d) doctor F1: no root anywhere → exit non-zero; (e) doctor F2: cwd inside a payload clone with `VERSION` → exit non-zero | `gatekeeper/tests/cli_root_resolution.rs` | `cargo test --test cli_root_resolution -- --ignored` fails exactly (a)–(e); default run green |
| 2 | Core rewrite (spec algorithm + testability refactor): `RootSource` / `ResolvedRoot`, four-input pure `resolve_root` with precedence env → self-governed → binary-adjacent → project `.topology` → `~/.topology` → fallback; remove the cwd marker walk and per-ancestor `.topology` probe; `framework_root()` thin wrapper emits the single stderr fallback warning; unit tests for spec AC 1–7 over tempdir fixtures; un-ignore fixtures (a)–(c) | `gatekeeper/src/main.rs` | spec AC 1–7 unit tests green; fixtures (a)–(c) green un-ignored; full suite green (any test relying on the old cwd walk updated to pin via `TOPOLOGY_ROOT` with a comment) |
| 3 | Doctor probes (spec): `resolved by:` line sourced from `ResolvedRoot`; F1 unmarked-root FAIL; F2 inside-payload FAIL (project == framework ∧ `VERSION` present); dev checkout and healthy governed fixture stay exit 0; un-ignore fixtures (d)–(e) | `gatekeeper/src/doctor.rs`, `gatekeeper/src/main.rs` (doctor wrapper) | fixtures (d)–(e) green un-ignored; existing doctor + `version_skew` tests untouched and green |
| 4 | Docs + version: USER-GUIDE root-resolution section rewritten to the new precedence (no new `gatekeeper …` spans beyond existing — keep `cli_doc_sync` green); CHANGELOG `v0.6.0` section; bump **all four** version files (Cargo.toml, Cargo.lock, plugin.json, marketplace.json — the v0.5.1 release guard tripped on the JSON pair) | `docs/USER-GUIDE.md`, `CHANGELOG.md`, `gatekeeper/Cargo.toml`+`Cargo.lock`, `.claude-plugin/*.json` | `gatekeeper check docs` + `cli_doc_sync` green; `grep -r '"version"' .claude-plugin/` shows 0.6.0 |
| 5 | Close-out: verify artifact (replayable evidence for AC 1–9, static PASS + `GATEKEEPER_SHADOW=replay` PASS), `just check`, review artifact as branch tip (strict format, written while uncommitted), PR for human merge; PR description flags the two ROADMAP deviations (Q1 order, self-governed step) for ratification | `docs/verify/…`, `docs/reviews/…` | all gates green; `just check` green; PR open |

Commit-order constraints: task 1 precedes tasks 2–3 (tdd gate heuristic: test-only commit
before production-touching commits). Task 2 precedes task 3 (doctor consumes `ResolvedRoot`).
Each fix commit un-ignores its own fixtures. Task 5's review artifact is the branch tip,
committed alone after `gatekeeper check review` passes (branch-tail convention: verify
second-to-last, review tip).

Risks to watch in implementation review: tests that previously passed *because* of the cwd
walk (hidden dependency — update them to explicit pins, never weaken assertions); the stderr
fallback warning leaking into tests that assert clean stderr; `current_exe()` under
`cargo test` resolving into `target/` (which makes the *repo* binary-adjacent — fine, but
unit tests must inject `exe_path` rather than rely on it).

Out of scope (spec): hook changes, `is_marked_root` / `project_root()` /
`resolve_artifacts_root()` changes, shadow mode for the new doctor failures, new
dependencies, version-skew logic (already shipped).
