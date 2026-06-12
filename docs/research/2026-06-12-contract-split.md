# Research — contract split (Phase 10)

## Problem

ROADMAP Phase 10 (`docs/ROADMAP.md:368–388`) wants the portable operating contract separated
from the framework-dev conventions currently conflated in `AGENTS.md`: a contract **template**
ships in the payload with parameterized artifact paths, `adapt` renders it for the target
project, a framework-dev doc stays in the repo, and the framework's own `AGENTS.md` becomes
the template instantiated for the framework plus the dev doc (dogfooding). Verify criteria:
the payload contains the template and no dev doc; rendering for a governed project yields only
`.claude/topology/…` paths; rendering for the framework yields `docs/…` paths; **no skill or
instinct references a repo-only path**.

Audit at main `a61b0a8` (v0.7.0, post-Phase-8).

## What AGENTS.md conflates today (88 lines)

| Section (line) | Classification |
|---|---|
| intro (1–3) | portable — methodology framing |
| Operating contract (5–14) | portable — skill-listing preamble (references `skills/`, harness-relative, OK) |
| The gate sequence (16–38) | portable **but path-broken**: hardcodes `docs/specs/` (27), `docs/plans/` (29), `docs/verify/` (32), `docs/reviews/` (34) — wrong for governed projects, whose artifacts root is `.claude/topology/` |
| Rules vs. gates (40–44) | portable |
| Conduct between gates (46–56) | portable |
| Stack conventions for this repo (58–62) | **framework-dev only** (Rust/Bash/Markdown stack, `cargo fmt`/`clippy`) |
| Skill description house format (64–70) | **framework-dev only** (contributor concern: how to author skills) |
| Compact Instructions (72–89) | portable — the memory/compaction protocol (`gatekeeper memory write/read`, `resume` skill) |

So the split is clean: two sections leave for the dev doc; the rest is the contract, needing
only path parameterization.

## What already exists (verified in source)

- **Path parameterization is already solved at the gate layer.** `resolve_artifacts_root()`
  (`gatekeeper/src/main.rs:467–477`): project == framework → `docs/`, else
  `.claude/topology/`. Every gate, `memory`, `learn`, and `config` already route through it
  (`artifacts_root()` at `main.rs:481`). Phase 10 needs **no gate changes** — only the prose
  (contract + skills) must stop contradicting the binary.
- **`adapt` is the renderer host.** `cmd_adapt(args, read_root, write_root)`
  (`gatekeeper/src/adapt.rs:364`) already has the two-root design the ROADMAP says rendering
  needs ("it knows both roots"): `read_root` = framework/payload, `write_root` = project.
  Generation is pure (`Vec<GenFile>`), all I/O through `apply_or_check` with a `--check`
  drift/idempotency mode — a rendered `CONTRACT.md` is just one more `GenFile`. There is no
  template machinery yet; builders use string constants and read `AGENTS.md` verbatim
  (`require_agents_md`, `adapt.rs:146`).
- **Contract leakage path exists today:** `build_cursor` embeds the raw `AGENTS.md` into
  `.cursor/rules/agents-contract.mdc` (`adapt.rs:205–212`) and `build_opencode` points
  `instructions` at `AGENTS.md` (`adapt.rs:241`) — so governed cursor/opencode projects
  currently receive the framework's `docs/…` paths and its dev conventions. The rendered
  contract must become what these builders deliver when roots differ (full delivery wiring is
  Phase 9; Phase 10 provides the template + render function they will consume).
- **Payload slot is reserved, not filled.** The payload spec
  (`docs/specs/2026-06-10-distribution-payload.md:45`) lists `CONTRACT.md` as a "reserved
  slot — the rendered operating contract (Phase 10)". `build-payload.sh` ships `AGENTS.md`
  **as a root marker only** (`build-payload.sh:103–105`; `is_marked_root` needs one of
  `["AGENTS.md", "gatekeeper"]`) — its content is not load-bearing for the binary, so
  restructuring `AGENTS.md` breaks no root resolution. `require_agents_md` in adapt only
  needs the file to exist with the contract for the *current* root.

## Skills that contradict governed projects (the verify-criterion blocker)

Seven skills hardcode repo-only `docs/…` paths in body text (instincts are clean — zero hits):

| Skill | Hardcoded paths |
|---|---|
| `research-first/SKILL.md:8,16,24,31` | `docs/research/` |
| `brainstorm-design/SKILL.md:8,15` | `docs/specs/` |
| `write-plan/SKILL.md:8,20` + `references/plan-template.md:5` | `docs/plans/`, `docs/specs/` |
| `verify-before-done/SKILL.md:15` | `docs/verify/` |
| `code-review/SKILL.md:27,65` | `docs/reviews/` |
| `finish-branch/SKILL.md:24` | `docs/reviews/` |
| `resume/SKILL.md:69,76,88` | `docs/verify/` |

Skills ship verbatim into payloads and are copied raw into `.opencode/skills/`
(`adapt.rs:252–257`), so they **cannot be rendered per-project** without forking the payload
model. The fix is wording: `<artifacts-root>/specs/…` with a one-line definition of artifacts
root (the gatekeeper FAIL messages already print the resolved absolute path, so the agent is
never guessing). The contract template defines the term once; skills reference it.

## Constraints and prior decisions that bind the design

- **ADR-0007 dependency freeze** (serde, serde_json, toml, regex): no template engine crate.
  Placeholder substitution must be hand-rolled (string replace) — consistent with adapt's
  existing constant-plus-format style.
- **Payload read-only at runtime** (Phase 7 decision, `ROADMAP.md:288`): `adapt` writing the
  rendered contract is install/integration-time, not runtime — allowed; but the *render
  target* for governed projects belongs to Phase 9's delivery story
  (`@.topology/CONTRACT.md` import line, `ROADMAP.md:230–234`). Phase 10's contract is the
  template + a pure render function + the framework dogfooding.
- **AGENTS.md must keep existing at the framework root** — root marker
  (`ROOT_MARKERS`, `main.rs`) and `require_agents_md` hard-error. Restructuring content is
  safe; deleting/renaming is not.
- **Docs lint** (`gatekeeper check docs`) guards ADR README links; a retirement/structure
  ADR is warranted (ADR-0015 precedent: record what moves where and why).
- **Phase 14 hollow-pass dispatch** (ADR-0014) reads gate names from the dispatch table —
  the contract's gate-sequence diagram is prose only, no coupling.

## What Phase 10 must build (scope candidate for the spec)

1. **`templates/CONTRACT.template.md`** (new, repo + payload): the portable sections of
   today's `AGENTS.md` (intro, operating contract, gate sequence, rules-vs-gates, conduct,
   compact/memory protocol) with placeholders for the artifact-path family
   (e.g. `{{ARTIFACTS_ROOT}}` → `docs` or `.claude/topology`) and the gatekeeper invocation
   (`{{GATEKEEPER_CMD}}` → `gatekeeper` in-repo vs the resolved `bin/` path or
   `$GATEKEEPER_BIN` in governed projects).
2. **A pure render function in `adapt.rs`** (`template + roots -> String`), unit-tested both
   ways (framework render contains `docs/…` and no `.claude/topology`; project render the
   inverse), surfaced as a `GenFile` so `--check` idempotency covers it.
3. **`docs/CONTRIBUTING-dev.md`** (name TBD in spec; stays in repo): stack conventions +
   skill house format sections move here verbatim; excluded from the payload (docs/ already
   is — assert it in `test-build-payload.sh`).
4. **Framework `AGENTS.md` regenerated** as the template rendered for the framework + a
   pointer to the dev doc — the repo dogfoods its own template (drift-checkable via
   `adapt --check` once `AGENTS.md` itself becomes a `GenFile`, which also guards against
   hand-edits diverging from the template).
5. **Skill wording sweep**: the seven skills (+ plan template) switch to
   `<artifacts-root>/…` phrasing; payload/e2e assertions that no `docs/`-rooted artifact
   path remains in `skills/` or `instincts/`.
6. **Payload manifest**: `build-payload.sh` ships the template; `test-build-payload.sh`
   asserts template present + dev doc absent (the Phase 10 verify line).

## Open questions for the design gate

- Template placeholder syntax (`{{NAME}}` vs `__NAME__`) and whether unknown placeholders
  hard-fail the render (recommend: yes, fail-closed — a typo'd placeholder shipping to users
  is a silent contract corruption).
- Whether the framework `AGENTS.md` is generated-and-committed (drift-gated like the other
  adapt outputs) or hand-maintained-with-a-test asserting it matches the render (recommend:
  generated, `adapt --check` covers it).
- Does the payload `CONTRACT.md` slot carry the **template** (rendered later, per-project)
  or a **pre-rendered governed-project default**? ROADMAP says adapt renders at inject time
  knowing both roots → ship the template under `templates/`, leave `CONTRACT.md` to Phase 9's
  inject step (recommend) — or pre-render the governed flavor into the payload slot at build
  time since `.claude/topology` is the same for every governed project, making Phase 9's
  inject a copy. The spec must pick one.
- Naming of the dev doc (`docs/DEVELOPMENT.md` vs `CONTRIBUTING.md` at root — root files risk
  payload inclusion by accident; recommend under `docs/`).
