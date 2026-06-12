# Spec — contract split (Phase 10)

**Status:** approved
**Research:** `docs/research/2026-06-12-contract-split.md`

## Goal

The portable operating contract and the framework-dev conventions stop sharing one file.
A contract **template** with parameterized paths ships in the payload; a pure render
function in `adapt` instantiates it for either world; the framework's own `AGENTS.md` is
the template rendered for the framework (dogfooding) plus a pointer to the dev doc; no
skill or instinct references a repo-only path.

## 1. The contract template (`templates/CONTRACT.template.md`)

A new top-level `templates/` directory holds the template. Its content is the portable
sections of today's `AGENTS.md` — intro, operating contract, gate sequence, rules-vs-gates,
conduct between gates, compact/memory protocol — with two placeholder families
(research §"What Phase 10 must build" item 1):

- `{{ARTIFACTS_ROOT}}` — the artifact-path prefix: `docs` (framework) or
  `.claude/topology` (governed project). Every gate-sequence path
  (`{{ARTIFACTS_ROOT}}/specs/<date>-<feature>.md` etc.) uses it.
- `{{GATEKEEPER_CMD}}` — how the agent invokes the binary: `gatekeeper` in the framework
  repo (PATH'd dev build), `gatekeeper` in governed projects too **via `$GATEKEEPER_BIN`
  wiring** — but the governed render adds one line stating the binary is wired through
  `GATEKEEPER_BIN` in `.claude/settings.json` (Phase 9's deliverable), so the contract
  needs no absolute path baked in. Decision: same literal `gatekeeper` both ways, plus the
  conditional wiring note via a third placeholder `{{BINARY_NOTE}}` (framework: empty;
  governed: the wiring sentence). Rationale: absolute paths in a committed contract would
  rot on reinstall.

The two framework-dev sections of `AGENTS.md` (stack conventions §58–62, skill house format
§64–70) do **not** appear in the template.

## 2. Render machinery in `adapt.rs`

A pure function (no I/O, unit-testable):

```rust
fn render_contract(template: &str, ctx: &ContractCtx) -> Result<String, String>
```

- `ContractCtx { artifacts_root: String, binary_note: String }` — built from the existing
  `read_root`/`write_root` pair (`resolve_artifacts_root` already encodes the rule).
- Substitution is plain string replacement (ADR-0007: no template crate).
- **Fail-closed** (research open-question 1, decided): after substitution, any remaining
  `{{` in the output is a hard error naming the offending placeholder — a typo'd
  placeholder must never ship silently. Unknown placeholders in the template likewise
  error; the known set is exactly the three above.

Rendering surfaces as a `GenFile`, so `apply_or_check --check` covers drift/idempotency
exactly like every other adapt output. Delivery wiring for governed projects (where the
rendered file lands, the `@.topology/CONTRACT.md` import) is **Phase 9 scope**; this phase
ships the template + renderer and proves both renders in unit tests.

## 3. Framework dogfooding: `AGENTS.md` is generated

(Research open-question 2, decided: generated-and-committed, drift-gated.)

- `AGENTS.md` := `render_contract(template, framework ctx)` + a short trailer section
  pointing at the dev doc (`## Framework development` → "Stack conventions and the skill
  house format live in `docs/DEVELOPMENT.md` — read it before changing this repo.").
- The trailer is part of the generation (a constant in `adapt.rs`), not hand-edited.
- A unit test asserts the on-disk `AGENTS.md` equals the generated content byte-for-byte —
  hand-edits to `AGENTS.md` or template drift fail `just check`. The file stays committed
  (root marker + codex auto-discovery + adapt's `require_agents_md` all keep working;
  research §constraints).

## 4. The dev doc (`docs/DEVELOPMENT.md`)

(Research open-question 4, decided: under `docs/`, automatically payload-excluded.)

Receives the two framework-dev sections verbatim (stack conventions, skill description
house format) under a one-line preamble stating its audience (framework contributors, never
shipped). `docs/` is already excluded from the payload; `test-build-payload.sh` gains an
explicit negative assertion anyway (§6) because the Phase 10 verify line names it.

## 5. Skill wording sweep (verify-criterion blocker)

The seven skills + plan template (research table) switch from `docs/<kind>/…` to
`<artifacts-root>/<kind>/…` phrasing. The term is defined once per skill at first use, in
one parenthetical: "(artifacts root: `docs/` in the framework repo, `.claude/topology/` in
governed projects — gate FAIL messages print the resolved path)". No skill behavior
changes; gates already resolve paths via `resolve_artifacts_root` (research §what already
exists). Instincts are already clean.

## 6. Payload manifest changes

- `build-payload.sh` ships `templates/CONTRACT.template.md`.
- The payload `CONTRACT.md` slot stays **empty/reserved** (research open-question 3,
  decided: render-at-inject-time per ROADMAP `docs/ROADMAP.md:377`; pre-rendering would bake
  in delivery decisions that belong to Phase 9).
- `test-build-payload.sh` asserts: template present in the tarball; `docs/DEVELOPMENT.md`
  absent; no `CONTRACT.md` yet; **no `docs/`-rooted artifact path remains anywhere under
  the payload's `skills/` or `instincts/`** (the grep-based assertion that locks §5).

## 7. ADR

`docs/adr/0016-contract-split.md` records: template + placeholder set + fail-closed render;
AGENTS.md generated; dev doc location; the Phase 9 boundary (delivery/injection). Linked
from `docs/adr/README.md` (docs lint R2).

## Acceptance criteria

- **AC-1** `templates/CONTRACT.template.md` exists; contains the six portable sections; no
  framework-dev section; placeholders only from the known set.
- **AC-2** `render_contract` framework render contains `docs/…` paths and zero
  `.claude/topology`; governed render contains `.claude/topology/…` paths and zero
  `docs/`-rooted artifact paths; unit-tested both ways.
- **AC-3** Render is fail-closed: an unknown placeholder in the template, or any
  unsubstituted `{{` in output, errors (unit-tested).
- **AC-4** `AGENTS.md` equals the generated render-plus-trailer byte-for-byte
  (unit-tested); still present at repo root (root marker intact:
  `cargo test cli_root_markers` stays green).
- **AC-5** `docs/DEVELOPMENT.md` carries the two dev sections; `AGENTS.md` no longer
  contains "Stack conventions" or "house format" content.
- **AC-6** No file under `skills/` or `instincts/` matches
  `docs/(specs|plans|research|verify|reviews|memory|learn)/` (asserted in
  `test-build-payload.sh` against the staged payload).
- **AC-7** Payload contains the template, no dev doc, no `CONTRACT.md`
  (`test-build-payload.sh`).
- **AC-8** ADR-0016 exists and is linked from the ADR README (`gatekeeper check docs` ok).
- **AC-9** Full quality gate: `just check` green (fmt, clippy `-D warnings`, shellcheck,
  typos, docs lint, unit tests), `just test-payload` green.

## Out of scope

- Contract **delivery** into governed projects (`@.topology/CONTRACT.md` import, managed
  blocks, `GATEKEEPER_BIN` settings wiring) — Phase 9.
- Switching `build_cursor`/`build_opencode` to deliver the rendered contract when roots
  differ — Phase 9 (today's AGENTS.md-verbatim behavior is unchanged this phase; the leak
  documented in research §what-already-exists closes when Phase 9 wires delivery).
- Any gate-logic change — `resolve_artifacts_root` already parameterizes.
