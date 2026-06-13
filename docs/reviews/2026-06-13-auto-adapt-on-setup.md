VERDICT: pass
HEAD: 3a73c1b01accbdf55f2fe9d854091e5066c78b16
BASE: 2d20e274e1235b02fe228222b3d255bd514e3575

# Review: auto-adapt-on-setup (2026-06-13)

## Blocking findings
None.

## Non-blocking notes
- `justfile:35` invokes `./gatekeeper/target/release/gatekeeper` by repo-relative path, so `just setup` only works when run from the repo root. `just` runs recipes with the working directory at the justfile's location by default, so this is correct in practice — but it is a latent coupling worth a comment if recipes ever gain a `cd`. Non-blocking.
- The new characterization test `dogfood_settings_claude_apply_rerun_is_noop` (`cli_adapt.rs:676`) relies on stdout marker strings (`wrote .claude/settings.json`, `DRIFT .claude/settings.json`) rather than re-reading the file mtime/content. This mirrors the existing `--check`/apply contract already asserted elsewhere and is consistent with sibling tests, so it is acceptable; a content/mtime assertion would be marginally more robust but is not required.
- The recipe always recompiles (`cargo build --release`) on every `just setup`, paying an incremental-build tax on warm trees. This is the design's deliberate M1 choice (always-build avoids a forbidden Bash `[ -x … ] || build` conditional) and is documented in the spec; flagged only as an accepted trade-off, not a defect.

## Criteria checked
### Spec/plan
- **AC1 (fresh tree auto-wires portable settings, no manual adapt)** — `justfile:33-35` appends `cargo build --release` then `gatekeeper/target/release/gatekeeper adapt --harness claude` after the pre-commit install. With no prior `.claude/settings.json`, the self-governed (`roots_differ == false`) path takes `use_portable = true` (`adapt.rs:856`), emits `${CLAUDE_PROJECT_DIR}` hook paths and drops `GATEKEEPER_BIN` (`adapt.rs:876-880`, `build_claude_hooks adapt.rs:536-562`). Verify doc shows the produced portable settings.json end-to-end.
- **AC2 (re-run no-op on settings.json)** — relies on the write-on-drift guard `} else if !disk_ok {` at `adapt.rs:930`; `disk_ok` compares only the two managed keys (`adapt.rs:907-923`). Pinned by the new test's second-apply assertion (no `wrote` line) and `--check` exit-0 assertion.
- **AC3 (self-governed claude apply-rerun-noop characterization, single-root harness, not run_proj)** — `dogfood_settings_claude_apply_rerun_is_noop` (`cli_adapt.rs:676`) uses `scratch_root` + `run` (same harness as `dogfood_settings_are_portable:648`), NOT the governed `run_proj` of `ac4_settings_no_clobber:510`. It asserts the no-op explicitly via both the absent `wrote` line and a clean `--check`, so it does not duplicate ac4. Honestly labelled test-after/characterization in the doc comment and plan (Task 1 "TDD honesty") — correct call, since `adapt`'s no-op already ships; a fake red→green would have been dishonest.
- **AC4 (trigger points wired + post-checkout declined)** — `just setup` trigger wired (`justfile:33-35`); `install.sh` already covered (no change, per non-goals); `post-checkout` explicitly declined with rationale in spec Approach 3 (`specs:85-88`) and Decision. No fourth `.git/hooks/pre-commit` writer is introduced — the recipe calls `adapt` only, which never writes the pre-commit hook (the pre-commit copy block at `justfile:18-32` is unchanged).
- **AC5 (DEVELOPMENT.md links ADR-0019 + states build coupling)** — `docs/DEVELOPMENT.md` new "Bootstrapping a fresh clone or worktree" section links `[ADR-0019](adr/0019-generated-only-settings-json.md)` and states the release build is load-bearing (portable settings omit `GATEKEEPER_BIN`, hooks resolve `gatekeeper/target/release/gatekeeper` via `security-scan.sh` fallback).
- **AC6 (Unreleased CHANGELOG entry)** — `CHANGELOG.md` adds a `### Changed` entry describing the build+adapt enhancement, the #52 complement, ADR-0019, the no-op-on-rerun property, and the load-bearing build.

### Standards
- **Three-language lanes (AGENTS.md / spec Constraints)** — the justfile addition is pure glue: two unconditional command lines (`justfile:34-35`) plus an `@echo`. No path computation, no conditionals, no branching logic; the wiring decision (portable vs absolute, drift detection, scaffold gating) all lives in Rust `adapt`. Conforms.
- **ADR-0019 (settings.json generated-only, never committed)** — the recipe *generates* settings.json via `adapt`; nothing in the diff commits or tracks it. The change is exactly the "corrective complement (#58)" named in ADR-0019:47-50. Conforms.
- **ADR-0003 (one markdown source per harness / generated configs)** — no adapter logic or config-content change; settings.json remains derived from the contract + adapter code. Conforms.
- **Surgical-changes-only (AGENTS.md diff-traceability)** — `git diff` is exactly: recipe enhancement, one characterization test, two doc edits (DEVELOPMENT.md, CHANGELOG.md), and four process artifacts (research/spec/plan/verify). No adjacent refactors or unrelated edits. Conforms.
- **Simplicity (AGENTS.md)** — smallest viable shape: folds build+adapt into the existing `setup` recipe rather than adding a new `just wire` recipe (spec Approach 2, deferred). No new abstraction or config knob. Conforms.
- **M1 failure semantics** — each recipe body line is its own shell and `just` aborts on first non-zero exit, so a `cargo build` failure (`justfile:34`) is fatal-and-loud and stops before `adapt` (`justfile:35`), leaving the hook installed but settings unwired — the documented "red, explained setup beats a half-wired clone that looks fine" behavior. Conforms.
- **Load-bearing invariants verified in code** — inv.2: `security-scan.sh:39-40` confirms the `$ROOT/gatekeeper/target/release/gatekeeper` fallback that portable mode depends on; portable drops `GATEKEEPER_BIN` at `adapt.rs:876-880`. inv.3: scaffold block is guarded by `if roots_differ` at `adapt.rs:962`, so self-governed adapt writes only `.claude/settings.json` (the `harness == "claude"` block `adapt.rs:863-956` runs unconditionally; the `.topology/` scaffold does not). inv.4: `adapt` never writes `.git/hooks/pre-commit`. All hold.
- **No CI / infinite-loop risk** — `just setup` is not invoked anywhere under `.github/` (grep: 0 hits); the recipe contains no self-invocation. The setup-time build/adapt does not run in CI and cannot recurse.
