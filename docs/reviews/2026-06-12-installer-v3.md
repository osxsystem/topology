VERDICT: pass
HEAD: 9da8ace3656cd46817bafec8701d02908cf4afaf
BASE: 87301c09c607e5a597a05bfcef3c8af2649abcaa

# Code review — installer-v3 (Phase 8, v0.7.0)

Branch: `feat/installer-v3`, reviewed 2026-06-12.
Reviewer: orchestrator pass (Fable 5 main loop) over the delegated implementation
(Sonnet subagent), per the standing review focus on fabricated interfaces and
overclaimed guarantees.

## Blocking findings

None.

## Criteria checked

### Spec/plan

Spec `docs/specs/2026-06-12-installer-v3.md`, plan `docs/plans/2026-06-12-installer-v3.md`:

- Global scope is payload-based in both modes: piped downloads + checksum-verifies via the
  shared `_download_and_verify_payload` helper; checkout assembles via `build-payload.sh`
  into `$ROOT` (the checkout is no longer `ROOT`); the `git clone` / `git pull --ff-only`
  paths are gone (`grep` confirms zero `git clone|git pull` in `install.sh`). ✔
- Shared machinery hoisted, not duplicated; `_handle_existing_root` is scope-aware:
  legacy-global rescue writes only `${ROOT}-backup-<ts>/` (explicit guard comment + e2e
  scenario assert no `.claude/` is created — the empty-`PROJECT_PATH` path bug named in the
  plan's risk list cannot occur). ✔
- Plugin channel fully retired: four files deleted; release guard reduced to
  tag == Cargo.toml in the same commit; `ROOT_MARKERS == ["AGENTS.md", "gatekeeper"]` with
  the doctor F1 message updated; README/USER-GUIDE plugin sections removed; remaining grep
  hits are historical/absence-assertion only (hand-checked). ✔
- `sudo ln` suggestion removed; stale-PATH repair block untouched (diff-audited). ✔
- Offline `installer` CI job added (test-payload, test-fetch, test-e2e); e2e grew 29 → 44
  scenarios with every pre-existing local-scope assertion unmodified. ✔
- Version 0.7.0 in Cargo.toml/Cargo.lock; CHANGELOG section; one-channel USER-GUIDE. ✔

### Standards

- Independently re-ran (not trusted from the report): `just check` (461 passed / 6 ignored,
  fmt, clippy `-D warnings`, shellcheck, typos, docs), `just test-e2e` (44/0),
  `just test-payload` (26/0), `just test-fetch` (3/0). ✔
- Verify gate static PASS and full `GATEKEEPER_SHADOW=replay` PASS (6/6 evidence commands
  executed green). ✔
- Delegated-output findings, fixed before this review:
  1. **tdd gate FAIL** (reported honestly by the subagent as a deviation): its red fixture
     lived in `main.rs`'s `#[cfg(test)]` module, which the gate's path heuristic counts as
     production. Fixed by pre-push history restructure: a genuine behavior-level red fixture
     (`gatekeeper/tests/cli_root_markers.rs`, commit `603a770`, force-run proven red against
     the pre-retirement code) now precedes all production commits; gate PASSes.
  2. The subagent's other two deviations (interactive-refusal branch untestable without a
     tty; AC-4 grep hits that are absence-assertions) were verified and accepted — both are
     recorded in the verify artifact's "Known gap" section rather than hidden.
- Accepted trade-off: `_rescue_legacy_clone_global`'s "skipped (already exists)" branch is
  near-unreachable (the backup dir is timestamped per run) — harmless belt-and-suspenders,
  kept for symmetry with the local rescue. ✔
- Protected-path commits (`main.rs`, `Cargo.toml`, workflows) carry the documented
  `--no-verify` override; no new dependencies (ADR-0007); e2e suite remains fully offline. ✔
