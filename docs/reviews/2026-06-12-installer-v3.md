VERDICT: pass
HEAD: 8b661795099d5d59e086aee6d8d76791df13d655
BASE: 87301c09c607e5a597a05bfcef3c8af2649abcaa

# Code review — installer-v3 (Phase 8, v0.7.0)

Branch: `feat/installer-v3`, reviewed 2026-06-12; **revision 3** after two human review
rounds on PR #43 (revision 1 reviewed `9da8ace…`; §Revision 2 covers the first-round
response `43f6230` + `2803163`; §Revision 3 covers the standards-round response
`1d2f0d5` + `8b66179`).
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

## Revision 2 — response to the human review on PR #43

### Spec/plan

- **Blocking finding fixed and regression-locked:** trailing-slash `TOPOLOGY_HOME` made
  `${ROOT}-backup-<ts>` a child of `ROOT`, so the legacy-clone rescue's backup was deleted
  with the clone after printing "rescued". Fix: `ROOT="${ROOT%/}"` (global scope;
  `PROJECT_PATH` was already normalized via `cd && pwd`). E2e test J proven effective by
  temporarily reverting the fix (suite 47/1, failing exactly on the new scenario), then
  re-applying (48/0). ✔
- **Reviewer's open question 1 adopted:** `PROMPT_INPUT_FD` seam extended to
  `can_prompt()`/`ask()` (set-and-readable → promptable; `/dev/tty` default untouched);
  e2e test K now executes the AC-3 interactive-refusal branch (exit 1, intact-clone
  message, `.git` + ledger untouched). The verify artifact's "known gap" is closed. ✔
- Should-fixes: header comments corrected (no-clone reality, `TOPOLOGY_HOME` wording);
  `cleanup_dl` trap registered inside `_download_and_verify_payload` right after `mktemp`
  (curl failure under `set -e` no longer leaks the temp dir); test H asserts the re-run's
  exit code + "Upgrading existing payload" marker instead of the vacuous VERSION check. ✔
- Nits: dead `trap_extra_corrupt` deleted; duplicate cargo-build scope branches collapsed;
  sections renumbered (the two "2."s); test I guard comment states what it can/cannot see;
  `RESEARCH.md` repo-layout diagram updated (open question 2). ✔

### Standards

- Independently re-run at this head: `just check` (461/6), `just test-e2e` (48/0),
  `just test-payload` (26/0), `just test-fetch` (3/0); verify gate static +
  `GATEKEEPER_SHADOW=replay` PASS. ✔
- `scripts/install.sh` is itself a protected path — the pre-commit hook blocked the
  response commit as designed; recommitted `--no-verify` with the documented override per
  the Track 2 grant. ✔
- Suite stays offline: test K's prompt input is a file via the seam, not a tty. ✔

## Revision 3 — response to the standards review round

### Spec/plan

- **Hard violation 1 (ADR layer) fixed:** new [ADR-0015](../adr/0015-plugin-channel-retirement.md)
  records the retirement (deletions, guard narrowing, `ROOT_MARKERS`, latest-with-pin version
  resolution, global payload + backup rescue, ADR-0012 §4 alignment); ADR-0010/0011 statuses
  amended in part with the surviving decisions named; README index updated (docs lint R2
  guards the links). ✔
- **Hard violation 2 (rescue gap) fixed in code, not a decision note:** both rescue
  functions now cover clone-era `memory/artifacts/*` exactly as ADR-0013's consequence
  names — local into `.claude/topology/memory/`, global into the backup dir — with
  sentinel assertions in both legacy e2e scenarios. ✔
- **Judgement call 3:** global installs name `$ROOT (global payload)` in the closing
  manifest, symmetric with local scope. ✔
- **Judgement call 4 adopted as behavior, recorded in ADR-0015 §6:**
  `_handle_existing_root` decides-then-acts; headless without `--yes` refuses with a remedy
  (the printed default is "N" — ADR-0012 §4), rescue + delete run only on confirmed
  replacement, and a refusal leaves no backup litter. New e2e test L (headless refusal,
  clone untouched) plus a no-backup assertion in test K. ✔
- Reviewer's trivial notes: AC-4 letter-vs-spirit accepted as-is; the spec §3 "post-install
  notes" gap stands covered by the doctor health check (no new note added — Phase 9 owns
  the `GATEKEEPER_BIN` wiring story). ✔

### Standards

- Independently re-run at this head: `just check` (461/6 + docs lint incl. the new ADR
  links), `just test-e2e` (53/0), `just test-payload` (26/0); verify gate static +
  `GATEKEEPER_SHADOW=replay` PASS. ✔
- The non-interactive behavior change (delete → refuse) is a deliberate safety inversion
  recorded in ADR-0015 §6; all e2e install invocations use `--yes` or the seam, so no
  scenario relied on the old destructive default. ✔
