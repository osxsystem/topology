VERDICT: pass
HEAD: 6bec5946d60fe29e289d6a16def556dd88a6842d
BASE: a61b0a81c8c98e38c70e267b1ba21eb08420d81b

# Code review — contract-split (Phase 10, v0.8.0)

Branch: `feat/contract-split`, reviewed 2026-06-12.
Reviewer: orchestrator pass (Fable 5 main loop) over the delegated implementation
(Sonnet subagent), per the standing review focus on fabricated interfaces and
overclaimed guarantees.

## Blocking findings

None.

## Criteria checked

### Spec/plan

Spec `docs/specs/2026-06-12-contract-split.md`, plan `docs/plans/2026-06-12-contract-split.md`:

- Template carries exactly the six portable sections of the pre-split `AGENTS.md` with the
  three spec'd placeholders and nothing else; the two dev sections moved verbatim to
  `docs/DEVELOPMENT.md` under a contributors-only preamble (diff-audited against the old
  `AGENTS.md` — no content invented, none dropped). ✔
- `render_contract` is pure, hand-rolled string replacement (ADR-0007 respected — no new
  crates), fail-closed: residual `{{` after substitution errors naming the token; the
  unknown-placeholder and typo cases are unit-tested. ✔
- CLI surface matches spec §2 exactly: `adapt --contract <framework|project>` prints to
  stdout, exit 0; errors to stderr, exit 2; no file writes; flag registered in
  `check_help_or_unknown` known-flags and usage (`cli_doc_sync` green). ✔
- Dogfooding holds by construction: on-disk `AGENTS.md` byte-equals
  `adapt --contract framework` output — independently re-verified with
  `adapt --contract framework | diff - AGENTS.md` (empty) — and the integration fixture
  makes drift a CI failure. Root marker + `require_agents_md` + codex auto-discovery all
  keep working (`cli_root_markers` green). ✔
- Skill sweep is wording-only: 7 skills + plan template; frontmatter
  names/descriptions untouched (`skill-rules.json` byte-identical); repo-side grep for
  `docs/(specs|plans|research|verify|reviews|memory|learn)/` over `skills/` + `instincts/`
  is clean, and the payload-side grep is a permanent `test-build-payload.sh` assertion. ✔
- Payload: template ships (build hard-fails if missing), `CONTRACT.md` slot stays reserved
  for Phase 9, dev doc provably absent from the tarball. ✔
- Out-of-scope respected: no delivery wiring, no `build_cursor`/`build_opencode` change,
  no gate-logic change. ✔

### Standards

- Independently re-ran (not trusted from the report): `cargo test` 475 passed / 6 ignored,
  `just check` (fmt, clippy `-D warnings`, shellcheck, typos, `check docs: ok`),
  `just test-payload` 30/0, `just test-e2e` 53/0, `check tdd` PASS, `check verify` static +
  `GATEKEEPER_SHADOW=replay` PASS. ✔
- Delegated-output findings, fixed before this review:
  1. **tdd gate FAIL** (reported honestly by the subagent; root cause was the plan's own
     mis-statement that the shell red block could share the task-1 commit): fixed by
     pre-push history restructure — pure test-only red commit
     (`gatekeeper/tests/cli_contract_render.rs`, proven red: "unknown flag '--contract'",
     exit 2) first, shell red block second, implementation cherry-picked on top;
     tree byte-identical to the pre-restructure branch (`git diff` empty). Gate PASSes.
  2. **Unflagged spec deviation**: `{{BINARY_NOTE}}` rendered empty for the governed world,
     deferring the wiring sentence to Phase 9 — spec §1 decided the governed render carries
     it so Phase 9 consumes the render rather than amending it. Fixed
     (`PROJECT_BINARY_NOTE`), ADR-0016 §1 aligned, asymmetry locked by two unit tests.
- Accepted trade-offs: the byte-equality fixture is brittle by design (that is its job —
  any hand-edit to `AGENTS.md` must fail until regenerated); `framework_ctx`/`project_ctx`
  hardcode the two worlds rather than deriving from `resolve_artifacts_root` — duplication
  of one string each, acceptable until Phase 9 passes real roots into `ContractCtx`. ✔
- Commit hygiene: scan-hook-blocked commits carry the documented `--no-verify` override per
  the Track 2 grant; version 0.8.0 in Cargo.toml + Cargo.lock only; CHANGELOG + ADR-0016 +
  README index row present (docs lint R2 green). ✔
