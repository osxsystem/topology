# Verify — contract split (Phase 10)

**Feature:** contract-split
**Date:** 2026-06-12
**Spec:** `docs/specs/2026-06-12-contract-split.md` (AC 1–9)
**Verified by:** main-loop agent (Fable 5), reviewing and re-validating the delegated
implementation (Sonnet subagent) at the branch head.

## AC-1/AC-2/AC-3 — template, both renders, fail-closed (integration via the CLI surface)

`templates/CONTRACT.template.md` carries the six portable `AGENTS.md` sections with exactly
the three known placeholders. `gatekeeper adapt --contract <framework|project>` prints the
render; the integration fixtures (committed red-first, see Quality gates) assert: framework
render has `docs/<kind>/` paths and zero `.claude/topology`; project render has
`.claude/topology/` and zero `docs/<kind>/` artifact paths plus the `GATEKEEPER_BIN` wiring
note (spec §1); an unknown placeholder in the template exits 2 naming the token on stderr:

```evidence
$ cargo test --manifest-path gatekeeper/Cargo.toml --test cli_contract_render
# expect: 5 passed
```

## AC-4/AC-5 — generated AGENTS.md, dev doc split

On-disk `AGENTS.md` byte-equals `adapt --contract framework` output (framework render +
dev-doc trailer) — verified independently with `adapt --contract framework | diff - AGENTS.md`
(no output) and locked by the integration fixture above. Root marker intact:

```evidence
$ cargo test --manifest-path gatekeeper/Cargo.toml --test cli_root_markers
# expect: 1 passed
```

`docs/DEVELOPMENT.md` carries the two dev sections (stack conventions, skill house format)
under a contributors-only preamble; `AGENTS.md` no longer contains them (checked by grep —
"Stack conventions" and "house format" appear only in the trailer pointer line).

## AC-6 — no repo-only path in skills/instincts

Repo-side grep over `skills/` + `instincts/` for
`docs/(specs|plans|research|verify|reviews|memory|learn)/` returns zero hits; the payload-side
equivalent is a permanent assertion in `test-build-payload.sh` (added red-first behind
`PHASE10_RED=1`, un-guarded in task 5):

```evidence
$ just test-payload
# expect: test-build-payload: 30 passed, 0 failed
```

## AC-7 — payload manifest

`build-payload.sh` ships `templates/CONTRACT.template.md` (hard-fails if missing);
`test-build-payload.sh` asserts template present, `DEVELOPMENT.md` absent, no `CONTRACT.md`
in the tarball (slot reserved for Phase 9 inject). The e2e suite covers the unpacked-tree
shape (`templates` added to `EXPECTED_TOPOLOGY_ENTRIES`):

```evidence
$ just test-e2e
# expect: test-payload-e2e: 53 passed, 0 failed
```

## AC-8 — ADR

`docs/adr/0016-contract-split.md` records the template/placeholder set, fail-closed render,
generated `AGENTS.md`, dev-doc location, and the Phase 9 delivery boundary; linked from the
ADR README (docs lint R2):

```evidence
$ cargo run --quiet --manifest-path gatekeeper/Cargo.toml -- check docs
# expect: check docs: ok
```

## AC-9 — full quality gate

475 unit tests passed / 6 ignored (main: 461/6 — net +14 from render unit tests, the
integration fixtures, and the wiring-note asymmetry pair), fmt-check, clippy `-D warnings`,
shellcheck, typos, docs lint:

```evidence
$ just check
# expect: check docs: ok
```

## Deviations found in review (fixed before this artifact)

1. **tdd-gate commit structure** (honestly reported by the subagent): the plan itself
   mis-stated that bundling `scripts/test-build-payload.sh` into the task-1 commit was safe;
   the gate classifies that path as production. Fixed by pre-push history restructure: a pure
   test-only red commit (`gatekeeper/tests/cli_contract_render.rs`, force-run proven red with
   "unknown flag '--contract'", exit 2) now precedes everything; the shell red block follows
   as its own commit. Gate PASSes.
2. **Unflagged spec deviation**: the governed render's `{{BINARY_NOTE}}` was empty
   ("Phase 9 will supply the sentence") instead of carrying the `GATEKEEPER_BIN` wiring note
   the spec §1 decided. Fixed: `PROJECT_BINARY_NOTE` constant, ADR-0016 §1 aligned, two unit
   tests lock the framework/project asymmetry.

## Quality gates

- `gatekeeper check tdd --feature contract-split`: PASS — failing-test-first history
  confirmed after the restructure.
- `gatekeeper check docs`: ok. Verify gate replayed under `GATEKEEPER_SHADOW=replay`.
- Version 0.8.0 in `Cargo.toml`/`Cargo.lock`; CHANGELOG entry present.
- No new dependencies (ADR-0007); payload suites fully offline.
