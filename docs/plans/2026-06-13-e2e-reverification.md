# Plan — end-to-end re-verification (Phase 12)

Executes the [spec](../specs/2026-06-13-e2e-reverification.md); grounded in the
[research note](../research/2026-06-13-e2e-reverification.md) and ROADMAP Phase 12.
Branch: `feat/phase12-e2e` (worktree `topology-phase12`). Implementation (the e2e shell harness)
delegated to a Sonnet subagent; the main loop runs it for real to capture verify evidence, then
self-reviews via a fresh-context critic.

**No binary change** (spec §0 Q4): no `gatekeeper/src/**`, `Cargo.toml`, or `Cargo.lock` edits — so
no protected-path overrides expected. All scripts `set -euo pipefail`; reuse the `pass`/`fail` idiom
and offline-install pattern (`--build-from-source`, no network) of `scripts/test-payload-e2e.sh`.

**Baseline (plan gate):** `cargo test` on `feat/phase12-e2e` (fresh off `main`) green; `just test-e2e`
(existing) green. Verified in the verify artifact.

| # | Task | Files | Acceptance |
|---|------|-------|------------|
| 1 | **Harness skeleton + reference fixture + red self-test.** `scripts/test-e2e-reference.sh` with `set -euo pipefail`, `pass`/`fail` counters, `cleanup` trap; `_make_reference_project` (git repo + `package.json` name `react-weather-app` + `src/index.js` + README + commit); the **red baseline** block asserting the five outcomes ABSENT on a fresh fixture (no CLAUDE.md import, no `.claude/`, planted-secret commit SUCCEEDS). | `scripts/test-e2e-reference.sh` | running the red block on a bare fixture passes its "absent" assertions; the planted-secret commit succeeds pre-install |
| 2 | **`--project` install + O1–O5.** Run `install.sh --project <fixture> --harness claude --yes --build-from-source` offline; assert O1 (CLAUDE.md `@.topology/CONTRACT.md` + `.topology/CONTRACT.md`), O2 (`GATEKEEPER_BIN` set; `"$GATEKEEPER_BIN" --version` + `check design` work with PATH scrubbed), O3 (settings wires both hooks; invoking `skill-activation.sh`/`security-scan.sh` behaves), O4 (planted-secret `git commit` blocked, HEAD unchanged, `--no-verify` bypasses), O5 (doctor artifacts root = `<fixture>/.claude/topology`; a spec under `.claude/topology/specs/` makes `check design` PASS). | `scripts/test-e2e-reference.sh` | AC-2..AC-6 green on a real install |
| 3 | **`--global` scope (AC-7).** `install.sh --global --yes --harness none --build-from-source` into temp `TOPOLOGY_HOME`; assert payload + `bin/gatekeeper` present, `doctor` from a separate project resolves `GlobalHome`, binary `--version` == payload `VERSION`. | `scripts/test-e2e-reference.sh` | AC-7 green |
| 4 | **justfile recipe + CI wiring (AC-8).** `just test-e2e-reference`; add it to the CI `installer` job in `ci.yml` next to `test-payload`/`test-fetch`/`test-e2e`. | `justfile`, `.github/workflows/ci.yml` | recipe exits 0 offline; CI job includes it |
| 5 | **Run for real + verify artifact.** Main loop runs `just test-e2e-reference`, captures the transcript (red baseline + both scopes), writes `docs/verify/2026-06-13-e2e-reverification.md` mapping O1–O5 + AC-7 to evidence; verify gate PASS. | `docs/verify/2026-06-13-e2e-reverification.md` | `gatekeeper check verify` PASS; evidence blocks allowlisted |
| 6 | **Docs: ROADMAP + CHANGELOG (AC-9).** ROADMAP status table Phase 12 → delivered, Track 2 closed; CHANGELOG `## Unreleased` note (no version/tag). Confirm `git diff` touches no `gatekeeper/src/**`/`Cargo.*`. | `docs/ROADMAP.md`, `CHANGELOG.md` | `check docs` ok; AC-9 holds |

**Sequencing:** 1 (skeleton + red proof) → 2 (project outcomes, the core) → 3 (global) → 4 (CI) →
5 (real run + verify) → 6 (docs). Tasks 1–4 are the Sonnet deliverable; 5–6 are the main loop's
(running for real + evidence + docs).

**Out of scope (spec non-goals):** live Claude-Code-session smoke test; gatekeeper code changes; the
external `react-weather-app` repo (superseded by the in-harness fixture).
