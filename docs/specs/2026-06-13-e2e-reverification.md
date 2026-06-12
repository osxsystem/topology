# Spec — end-to-end re-verification (Phase 12)

**Status:** approved
**Research:** `docs/research/2026-06-13-e2e-reverification.md`
**ROADMAP:** Phase 12 (closes Track 2)

> **Design approval note.** Driven autonomously under an explicit maintainer delegation
> ("kick off Phase 12 via /loop, decide autonomously, self-review, open a PR for Codex"). The design
> gate's human approval is folded into the **PR review by Codex**, not a pre-commit `Status:` flip.
> The scope decisions in §0 are what Codex should sanity-check.

## Goal

Prove, with real captured evidence on a genuine reference project, that a consumer who runs the
installer gets the five consumer-visible outcomes — and lock that proof into a reproducible,
CI-gated harness so the outcomes cannot silently regress.

## 0. Decisions (resolved from research Q1–Q5; ratify at PR review)

- **Q1 reference project — build a genuine fixture, don't depend on the missing `react-weather-app`.**
  The harness creates a minimal but real project (a `react-weather-app`-shaped stand-in: `package.json`
  with a name/scripts, a `src/` source file, a `README.md`, real git history) so the run mirrors a
  consumer project. Self-contained, offline, CI-safe. Rationale: the named external repo isn't
  committed (research F1); a real fixture proves the same outcomes without an unreproducible dependency.
- **Q2 new script.** `scripts/test-e2e-reference.sh` (+ `just test-e2e-reference`), wired into the CI
  `installer` job — distinct from the 35K mechanics harness, lower merge risk.
- **Q4 no gatekeeper version bump.** The binary is unchanged (research F5); Phase 12 ships a
  verification harness + CI wiring + verify artifact only. No `Cargo.toml` / `Cargo.lock` change, no
  release tag. (Track-2 close is recorded in ROADMAP + CHANGELOG `Unreleased`.)
- The harness runs the **real `install.sh`** offline, building the payload from the checkout and the
  binary from source (`--build-from-source`), so evidence is genuine, not mocked.

## 1. The harness (`scripts/test-e2e-reference.sh`)

`set -euo pipefail`; reuse the `pass`/`fail` counter idiom and exit-nonzero-on-any-fail convention of
`test-payload-e2e.sh`. A `cleanup` trap removes all tempdirs. The framework root is the checkout
(`$(git rev-parse --show-toplevel)` from the script location).

### 1a. Reference fixture builder
`_make_reference_project <dir>`: `git init`, set test identity, write `package.json`
(`"name": "react-weather-app"`, a `build`/`test` script), `src/index.js`, `README.md`; commit. This
is the consumer project both scopes install into.

### 1b. Red self-test (research F5 — the harness must not be tautological)
**Before any install**, assert the five outcomes are ABSENT on a fresh fixture (no `CLAUDE.md` import,
no `.claude/settings.json`, no `.claude/topology/`, a `git commit` of a planted secret SUCCEEDS
because no hook is installed). Record this as the red baseline; then install and re-assert green.

### 1c. `--project` scope — the five outcomes (the heart of Phase 12)
Run `install.sh --project <fixture> --harness claude --yes --build-from-source` (offline), then assert:

- **O1 contract in context** — `<fixture>/CLAUDE.md` contains the line `@.topology/CONTRACT.md`
  (created/appended by Phase 9 `adapt`), and `<fixture>/.topology/CONTRACT.md` exists and renders the
  governed paths (`.claude/topology`).
- **O2 bare `gatekeeper` via `GATEKEEPER_BIN`** — `.claude/settings.json` `env.GATEKEEPER_BIN` points
  at the installed binary; invoking `"$GATEKEEPER_BIN" --version` and `… check design --feature x`
  from the project with **`PATH` scrubbed of any gatekeeper** still works (proves no PATH/sudo step).
- **O3 hooks fire** — `.claude/settings.json` wires `UserPromptSubmit`→`skill-activation.sh` and
  `PreToolUse`→`security-scan.sh`; invoking each hook script directly with a representative payload
  produces its contract behavior (skill-activation emits advisory context / exit 0; security-scan on a
  planted secret emits a `deny` decision).
- **O4 project pre-commit blocks a planted secret** — stage a file containing a high-signal secret
  (e.g. an AWS-key-shaped string), attempt `git commit`; assert non-zero exit and a scanner BLOCK
  line, and that the commit did NOT land (`git rev-parse HEAD` unchanged). Confirm `--no-verify` is
  the documented bypass (commit succeeds with it) — the documented residual, asserted, not hidden.
- **O5 design artifact lands under the project** — `gatekeeper` run from the project resolves
  artifacts root to `<fixture>/.claude/topology` (assert via `doctor`); writing a spec to
  `<fixture>/.claude/topology/specs/<date>-x.md` (+ its research note) makes
  `gatekeeper check design --feature x` resolve and read there (PASS), proving gate artifacts anchor
  to the project, not the framework.

### 1d. `--global` scope
Run `install.sh --global --yes --harness none --build-from-source` into a temp `TOPOLOGY_HOME`; assert
the payload lands at `$TOPOLOGY_HOME/.topology` with a `bin/gatekeeper`, `doctor` run from a separate
project resolves the global framework root (`GlobalHome`), and the binary `--version` matches the
payload `VERSION` (no skew). Proves the shared-install substrate behind O2.

## 2. CI wiring

Add `test-e2e-reference` to the `justfile` and to the CI `installer` job (`ci.yml`) alongside the
existing `test-payload`/`test-fetch`/`test-e2e` so the five outcomes gate every future merge.

## 3. Docs

- Verify artifact `docs/verify/2026-06-13-e2e-reverification.md`: the real captured transcript of the
  red baseline + both scopes, mapping each of O1–O5 to its evidence.
- ROADMAP status: Phase 12 → delivered; Track 2 closed.
- CHANGELOG `## Unreleased` note (no version/tag — binary unchanged).

## Acceptance

- **AC-1 (red baseline).** On a fresh fixture pre-install, all five outcomes assert ABSENT (the
  planted-secret commit succeeds with no hook). Proves the harness is not tautological.
- **AC-2..AC-6** = O1..O5 each green after `--project --harness claude` install, with captured evidence.
- **AC-7 (global).** `--global` install: payload + binary at `TOPOLOGY_HOME`, `doctor` resolves
  `GlobalHome`, no version skew.
- **AC-8 (reproducible + CI).** `just test-e2e-reference` exits 0 offline and is wired into CI.
- **AC-9 (no binary change).** `git diff` touches no `gatekeeper/src/**`, `Cargo.toml`, or `Cargo.lock`.

## Non-goals

- No gatekeeper code change, no version bump, no release tag (binary unchanged).
- Not a live Claude Code *session* (real agent in the loop) — that needs the harness; O3 proves the
  hooks fire by invoking the wired scripts, which is the deterministic, CI-able equivalent. A
  live-session smoke test is a possible follow-up, noted not silently dropped.
- The external `react-weather-app` repo itself — superseded by the genuine in-harness fixture (Q1).
