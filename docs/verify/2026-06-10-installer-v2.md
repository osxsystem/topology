# Verify — installer v2

**Feature:** installer-v2 (guided installer, project-local artifact root, stale-PATH repair)
**Date:** 2026-06-10
**Spec:** [docs/specs/2026-06-10-installer-v2.md](../specs/2026-06-10-installer-v2.md)
**Verified by:** main-loop agent (Fable 5), independently re-running every acceptance criterion
after reviewing the delegated implementation.

## Symptom reproduced (the "before", on `main`)

On `main`, the first cross-project test put gate artifacts in the governed project's root `docs/`,
the installer asked nothing (no harness or scope choice), `adapt` run with `TOPOLOGY_ROOT` set
would have written its config into the framework checkout, and a stale 0.1.0
`~/.cargo/bin/gatekeeper` sat on PATH unflagged. All four resolved below.

## AC-1 — Framework repo unchanged

In-repo `check design` / `check plan` for this feature PASS against root `docs/` (equal-roots
path); `just check` green end-to-end. No artifact files moved in this repo.

## AC-2/3/4 — External-project artifacts, review gate, adapt (integration tests)

`cargo test`: **232 passed, 0 failed, 2 ignored** across all suites. The new tests were read for
vacuousness before trusting them: `external_project_design_fails_naming_claude_topology` asserts
the FAIL names `.claude/topology/` AND not `docs/research`; its PASS twin first re-proves the FAIL
with research-only, then flips to PASS after the spec lands under `.claude/topology/specs/`; the
docs-root-ignored test plants the same files under scratch `docs/` and asserts they are not found.
Review-gate and adapt twins follow the same prove-both-directions shape
(`gatekeeper/tests/cli_review.rs`, `cli_adapt.rs`). The pre-existing review hardening tests pass
with only the signature adaptation (no behavioral edits).

## AC-5 — Prompted install (non-tty, flags)

`install.sh --project <scratch> --harness claude --yes` with the `file://` fixture:

- Vendored the framework at `<scratch>/.topology` (local clone), appended `.topology/` to the
  scratch `.gitignore`, fetched the prebuilt into `.topology/bin/gatekeeper`.
- `wrote .claude/settings.json` — in the **project**, with both hook commands pointing at
  `<scratch>/.topology/hooks/*.sh` (verified by reading the file).
- The manifest listed all six entries (vendored framework, `.gitignore` edit, binary, `CLAUDE.md`
  symlink, pre-commit copy, wiring file) as absolute paths.

`--global --harness none --yes` reproduces the v1 flow plus `assumed:` lines naming the
overriding flags (exercised as part of AC-6a below).

## AC-6 — Stale-PATH repair

Fixture: a stub `gatekeeper` printing `0.0.1`, prepended to PATH.

- **Non-tty:** the warning block named the stub path, `stale: 0.0.1`, `installed: 0.3.0`, and the
  `cp` remedy; `cmp` proved the stub byte-identical afterward.
- **Interactive (pty via `script`, answer `y` through the `PROMPT_INPUT_FD` seam):** the prompt
  named path and both versions; the stub was overwritten in place and afterwards reported
  `gatekeeper 0.3.0`; the printed manifest contains `<path> (overwritten with 0.3.0)`.

**Bug found and fixed during this verification** (commit `fix(install): run stale-PATH repair
before the manifest is printed`): the repair originally ran *after* the manifest printout, so an
accepted overwrite could never appear in the install's own audit output. The pass above is
post-fix.

## AC-7 — Doctor

With the 0.0.1 stub on PATH, `doctor` printed `framework root:`, `project root:`,
`artifacts root: …/docs` (equal-roots), and
`PATH gatekeeper: <stub> (version skew: 0.0.1 vs 0.3.0)` — and still exited 0 with
`doctor: all probes ok` (informational, as specced).

## AC-8 — Quality gates

`just check` green (fmt, clippy, 232 tests, shellcheck over hooks+scripts, typos, docs lint);
`check docs` green with ADR-0012 linked from the index and the ROADMAP verify token resolving to
this file.

## Additional finding fixed during review

**Version coherence** (commit `chore(release): bump version to 0.3.0`): the spec missed that the
installer's pinned download would fetch the v0.2.0 binary — which lacks the two-roots behavior —
making the new harness-wiring step write artifacts to the old locations. Bumped all three
manifests to 0.3.0; the post-merge `v0.3.0` tag activates the matching release. Until then the
fetch falls back to cargo with a clear message (same transition as v0.2.0, recorded in the PR).
