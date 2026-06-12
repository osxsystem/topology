# Plan — installer v3: global payload + plugin retirement (Phase 8)

Executes the [spec](../specs/2026-06-12-installer-v3.md); grounding in the
[research note](../research/2026-06-12-installer-v3.md) and ROADMAP Phase 8.
Branch: `feat/installer-v3` (worktree `topology-phase8`). Coding delegated to a Sonnet
subagent; the main loop plans and reviews. Protected/sensitive paths in this phase:
`gatekeeper/src/main.rs`, `gatekeeper/Cargo.toml`, `.github/workflows/release.yml` — commits
touching them carry the documented `--no-verify` override per the Track 2 grant.

| # | Task | Files | Acceptance |
|---|------|-------|------------|
| 1 | Red fixtures commit (test-only, precedes all production edits per the tdd gate): (i) extend `test-payload-e2e.sh` with the global-scope scenarios as initially-skipped cases behind a `PHASE8_RED=1` guard — piped global offline install (AC-1), corrupted-checksum refusal (AC-1), checkout global assembly (AC-2), legacy-global-clone rescue/refusal/`--yes` (AC-3); (ii) `#[ignore]`-tagged unit test in `gatekeeper`: `skills/` + `.claude-plugin/` alone is not a marked root (AC-6) | `scripts/test-payload-e2e.sh`, `gatekeeper/src/main.rs` (test mod only) | guarded cases fail when force-enabled against current code; default `just test-e2e` + suite stay green |
| 2 | Global payload switch (spec §1): shared download/verify/unpack + `_handle_existing_root` helpers hoisted above the scope branch; piped global downloads payload, checkout global assembles via `build-payload.sh`; `--build-from-source` global rules; legacy-global rescue to `${ROOT}-backup-<ts>/`; remove `git clone`/`git pull` path; un-skip e2e scenarios (drop the `PHASE8_RED` guard for AC-1/2/3 cases) | `scripts/install.sh`, `scripts/test-payload-e2e.sh` | `just test-e2e` green incl. new scenarios; shellcheck clean |
| 3 | Plugin retirement (spec §2): delete the four plugin files; release-guard probes reduced to Cargo.toml; `ROOT_MARKERS` → `["AGENTS.md", "gatekeeper"]` + doctor F1 message + un-ignore the marker unit test; README/USER-GUIDE plugin sections removed | `.claude-plugin/*` (delete), `hooks/ensure-gatekeeper.sh` + `hooks/hooks.json` (delete), `.github/workflows/release.yml`, `gatekeeper/src/main.rs`, `gatekeeper/src/doctor.rs`, `README.md`, `docs/USER-GUIDE.md` | AC-4 grep clean; AC-5; AC-6 test green un-ignored; `cli_doc_sync` + `check docs` green |
| 4 | PATH cleanup + CI job (spec §3–4): drop the `sudo ln` suggestion (stale-PATH repair untouched, AC-7); add offline `installer` job to `ci.yml` running `just test-payload`, `just test-fetch`, `just test-e2e` | `scripts/install.sh`, `.github/workflows/ci.yml` | AC-7 diff-audited; AC-8 job present and green in PR CI |
| 5 | Docs + version: CHANGELOG `v0.7.0`; bump `Cargo.toml` + `Cargo.lock` (the only manifests left); USER-GUIDE install section reflects one-channel reality | `CHANGELOG.md`, `gatekeeper/Cargo.toml`, `gatekeeper/Cargo.lock`, `docs/USER-GUIDE.md` | `gatekeeper check docs` green; AC-9 |
| 6 | Close-out (main loop, not the subagent): verify artifact (static + `GATEKEEPER_SHADOW=replay`), `just check`, review artifact as branch tip, PR for human merge | `docs/verify/…`, `docs/reviews/…` | all gates green; PR open |

Commit-order constraints: task 1 precedes tasks 2–5 (tdd gate heuristic). Task 2 precedes
task 4's e2e-in-CI wiring (the job must run the finished suite). Task 3's release-guard edit
ships in the same commit as the manifest deletions — never split. Task 6's review artifact
is the branch tip (branch-tail convention).

Risks to watch in implementation review: the global/local code share must not change local
behavior (e2e local scenarios are the regression net); `_handle_existing_root` for global
runs with `PROJECT_PATH` empty — rescue must not write into `/.claude/topology`; deleting
`hooks.json` must not break the dev clone's hook wiring (verified: wired via user-level
settings, no in-repo `.claude/`); the e2e suite must stay offline (no network in CI).

Out of scope (spec): Phase 9 `GATEKEEPER_BIN` settings wiring, Phase 10 contract template,
`adapt` changes, payload contents changes, new dependencies.
