# Roadmap

The path from today's Topology (gates + Claude Code) to the full operator system. Each phase is a
separately-approved unit of work with its own deliverables and a concrete **verify** check — nothing
is "done" without a check that proves it.

> This is the plan, not a changelog. **Phase 0**, **Phase 1 (security scanning)**, the
> **code-review gate** (Phase 1.5), **Phase 2 (instincts engine)**, **Phase 3 (continuous learning)**, and
> **Phase 4 (cross-harness adapters)**, **Phase 5 (memory + research-first)**, and **Phase 6 (packaging & distribution)** are delivered. See
> [`../METHODOLOGY.md`](../METHODOLOGY.md) and [`ARCHITECTURE.md`](ARCHITECTURE.md).
> The active track is **Phases 7–12 (distribution vs. repository)** — separating what a governed
> project receives from the framework's own development repo.

```mermaid
flowchart LR
    P0["Phase 0<br/>Blueprint<br/>✅ this pass"] --> P1["Phase 1<br/>Security<br/>scanning ✅"]
    P1 --> P2["Phase 2<br/>Instincts<br/>engine ✅"]
    P2 --> P3["Phase 3<br/>Continuous<br/>learning ✅"]
    P3 --> P4["Phase 4<br/>Cross-harness<br/>adapters ✅"]
    P4 --> P5["Phase 5<br/>Memory +<br/>research-first ✅"]
    P5 --> P6["Phase 6<br/>Packaging<br/>+ CI ✅"]
```

```mermaid
flowchart LR
    P7["Phase 7<br/>Distribution<br/>payload"] --> P8["Phase 8<br/>Installer v3"]
    P8 --> P9["Phase 9<br/>adapt v2:<br/>project integration"]
    P9 --> P12["Phase 12<br/>End-to-end<br/>re-verification"]
    P10["Phase 10<br/>Contract split"] --> P12
    P11["Phase 11<br/>Root resolution<br/>hardening"] --> P12
```

**Why this order.** Security is the biggest true gap, so it's front-loaded (Phase 1). Instincts
(Phase 2) must exist before learning (Phase 3), because learning *promotes* gotchas into instincts —
the target has to be there first. Adapters (Phase 4) come once there's a rich operator set worth
fanning out. Memory/research hardening (Phase 5) and packaging (Phase 6) finish the system.

---

## Phase 0 — Blueprint ✅ (this pass)

**Goal.** A reviewable design any contributor can read, and a path to build it.

**Deliverables.** `METHODOLOGY.md`, `docs/ARCHITECTURE.md`, `docs/ROADMAP.md`, `docs/EXTENDING.md`,
five ADRs under `docs/adr/`, and a surgical `README.md` update. No code.

**Verify.** Every pillar (6) and harness (4) has a named section; Mermaid blocks parse and have ASCII
fallbacks; internal links resolve; Phases 1–6 are framed as *planned*, not done.

---

## Phase 1 — Security scanning ✅ *(front-loaded, delivered 2026-06-06)*

**Goal.** A deterministic safety floor: no secret or dangerous command reaches execution or history.

**Deliverables.**
- `gatekeeper/src/scan.rs` — content/command scanner over `security/rules.toml` (ReDoS-safe `RegexSet`; `serde`/`toml`/`serde_json` per ADR-0007; `json.rs` retired).
- `security/rules.toml` — seed rules (cloud keys, private keys, `rm -rf /`, pipe-to-shell, history rewrite).
- `hooks/security-scan.sh` — `PreToolUse` glue (emits deny/ask JSON; fail-closed).
- `hooks/pre-commit.sh` — pre-commit glue scanning the staged blobs + protected-path integrity.
- `skills/security-scanning/SKILL.md` — when/how the agent invokes and responds to a veto.

**`gatekeeper` surface.** `gatekeeper scan --hook` (PreToolUse JSON on stdin), `--cmd`/`--content`
(stdin), `--staged` (git index), `--check-path <p>`; exit `0` clean / `1` veto / `2` fail-closed.

**Verify.** A planted AWS key and a `curl … | sh` are **blocked**; a clean diff/command **passes**;
`cargo test` covers each rule kind; the `PreToolUse` hook blocks a real tool call end to end. Evidence:
`docs/verify/2026-06-06-security-scanning.md`.

**Depends on.** Phase 0.

---

## Phase 2 — Instincts engine ✅ *(delivered 2026-06-08)*

**Goal.** Always-on, reasoning-based guardrails, cheaper than skills, injected every session.

**Deliverables.**
- `instincts/` directory + the `<id>.md` format (frontmatter `id` / `priority` / optional `source`; body = the *why*). Instincts carry **no scope** — always-on.
- `gatekeeper/src/instinct.rs` — hand-rolled frontmatter parser, directory loader (sorted, deduped, fail-mode matrix), preamble renderer with word-budget truncation, `cmd_instinct` (list/render), `activate_section`.
- `activate` extended to inject the always-on instinct set alongside routed skills.
- Six seed instincts: `constraints-as-reasoning`, `evidence-over-assertion`, `gates-not-rules` (high); `surgical-changes-only`, `three-language-lanes` (high), `weakest-enforcement-that-works` (medium).

**New `gatekeeper` surface.** `gatekeeper instinct list`, `gatekeeper instinct render --harness <h> [--budget <n>]`;
`activate` now emits instincts.

**Verify.** `gatekeeper activate` injects the always-on instincts under the `Always-on instincts —`
header for any prompt; a missing `instincts/` dir yields no instincts and exit 0; `gatekeeper instinct
render --harness claude` reproduces the same bodies. Evidence: `docs/verify/2026-06-07-instincts-engine.md`.

**Depends on.** Phase 0. (Independent of Phase 1; orderable either way.)

---

## Phase 3 — Continuous learning ✅ *(delivered 2026-06-08)*

**Goal.** Failures and corrections become permanent operators — the system tightens where it's burned.

**Deliverables.**
- `gatekeeper/src/learn.rs` — capture (append a structured gotcha to the append-only ledger) + promote
  (scaffold an operator; validate it against that operator's own loader; print a diff; write only on
  explicit human confirmation).
- `docs/learn/` — the gotcha ledger (`ledger.md`) + a `README.md` describing the entry format and the loop.
- `skills/capture-gotcha/SKILL.md` — recognize a recurring failure and route it (wired into
  `hooks/skill-rules.json`); `hooks/learn-capture.sh` — an opt-in `Stop` hook for automated capture.
- Promotion path: a ledger entry → a new `instinct`, `skill`, or `security/rules.toml` rule, **human-approved**.

**New `gatekeeper` surface.** `gatekeeper learn capture` (on Stop/gate-failure), `gatekeeper learn list`,
`gatekeeper learn promote`.

**Verify.** `learn capture` appends a parseable `## <id>` entry to `docs/learn/ledger.md` (and a recurrence
is the same id captured again — `learn list` shows the occurrence count); `learn promote --kind instinct`
writes an `instincts/<id>.md` (with `source: ledger:<id>`) that passes `gatekeeper instinct list`,
`--kind skill` writes a `skills/<id>/SKILL.md` that appears in `gatekeeper list`, and `--kind rule
--pattern <re>` appends a `[[rule]]` that `gatekeeper scan` loads; a declined promotion (no `y`) writes
nothing. Evidence: `docs/verify/2026-06-08-continuous-learning.md`.

**Depends on.** Phase 2 (promotes into instincts) and Phase 1 (promotes into scan rules).

---

## Phase 4 — Cross-harness adapters ✅ *(delivered 2026-06-08, ahead of Phase 3)*

**Goal.** Native Codex, Cursor, and OpenCode support generated from the one Markdown source.

**Deliverables.**
- `gatekeeper/src/adapt.rs` — pure `root -> Vec<GenFile>` builders + `apply_or_check` (a `--check`
  idempotency mode); `adapters/README.md` documents the per-harness mapping. No new crates.
- Codex: generate `.codex/config.toml` (project-safe `project_doc_max_bytes`, validated against
  `codex --strict-config`); the contract rides on the auto-discovered `AGENTS.md`. (Project-local config
  may not carry `profiles`/provider keys, so there are no "Codex agents/profiles" — see ADR-0008.)
- Cursor: generate `.cursor/rules/*.mdc` — always-on **instincts** and the `AGENTS.md` contract map to
  Cursor's **Always** mode; keyword-routed **skills** map to **Agent Requested** (description-based — the
  closest primitive, since Cursor has no keyword router; see ADR-0008).
- OpenCode: generate `opencode.json` (`instructions`) + `.opencode/instincts.md` + `.opencode/skills/`
  copied from `skills/`.
- Claude: generate `.claude/settings.json` (the hook wiring) — the source-native harness as a uniform
  generated target. `scripts/install.sh` documents the opt-in `adapt` commands.

**New `gatekeeper` surface.** `gatekeeper adapt --harness {codex|cursor|opencode|claude} [--check]`.

**Verify.** `gatekeeper adapt --harness <h>` writes each harness's native files; the generated
`.codex/config.toml` loads under `codex --strict-config`; `opencode.json` / `.claude/settings.json` are
valid JSON in the documented schema; copied skills are byte-equal; `--check` is idempotent (exit 0) and
flags drift (exit 1). Evidence: `docs/verify/2026-06-08-cross-harness-adapters.md`.

**Depends on.** Phase 2 (instincts to fan out) + the skill set. Phase 3's learning loop is **not** a
prerequisite — it only adds more operators to fan out later, so Phase 4 shipped first.

---

## Phase 5 — Memory + research-first hardening ✅ *(delivered 2026-06-08)*

**Goal.** Make context a managed budget and exploration a gated stage.

**Deliverables.**
- `memory/` protocol — handoff artifact format (one kind; the `compaction` kind was cut — a handoff written before context fills serves the same purpose); `gatekeeper memory write/read/list` helpers, with secret-refusal on the rendered artifact and `memory/artifacts/` write-protection.
- RTK integration documented and wired as the default shell proxy.
- `research` gate: `gatekeeper check research` + `skills/research-first/SKILL.md`, prepended to the sequence.
- The `resume` session-lifecycle skill (read handoff → verify state → act). *(Domain skills for the house stack are deferred — not part of the executed plan. The `code-review` critic skill + `review` gate were pulled forward and delivered 2026-06-05 — see `docs/adr/0006-code-review-gate.md`.)*

**New `gatekeeper` surface.** `gatekeeper check research --feature <slug>`; memory read/write helpers.

**Verify.** The `research` gate blocks `design` when no research note exists; a handoff artifact
round-trips (write → fresh session → resume); the `code-review` subagent returns findings against the plan.
**Evidence:** [`docs/verify/2026-06-08-memory-research-first.md`](verify/2026-06-08-memory-research-first.md) — all 11 acceptance criteria checked, quality gates green (198 passed / 2 ignored, no dependency change).

**Depends on.** Phase 4 (research-first skill should ship to all harnesses).

---

## Phase 6 — Packaging & distribution ✅ *(delivered 2026-06-09)*

**Goal.** One-command install per harness, pinned and CI-guarded.

**Deliverables.**
- Per-harness installer flows + plugin manifests (`.claude-plugin/`, equivalents).
- Version pinning (gatekeeper + rules schema versions).
- CI: `cargo test` / `cargo fmt --check` / `cargo clippy -- -D warnings` + a docs link/coverage lint.

**New `gatekeeper` surface.** `gatekeeper --version`; a `gatekeeper doctor` health check (optional).

**Verify.** A clean-machine install works for each harness; CI is green on a fresh clone; a tagged
release ships a single std-only macOS-arm64 binary (it dynamically links `libSystem`).
**Evidence:** [`docs/verify/2026-06-09-packaging-distribution.md`](verify/2026-06-09-packaging-distribution.md) — all 12 acceptance criteria checked, quality gates green (213 passed / 2 ignored, no dependency change), `claude plugin validate .` passed.

**Addendum (2026-06-10):** Prebuilt-binary distribution (four-target release matrix, installer download, plugin self-provisioning) delivered as a follow-on. Decision: [ADR-0011](adr/0011-prebuilt-binary-distribution.md). Verify: [`docs/verify/2026-06-10-one-command-install.md`](verify/2026-06-10-one-command-install.md).

**Addendum (2026-06-10):** Guided installer (harness/scope prompts), project-local artifact root (`.claude/topology/`), stale-PATH repair delivered as a follow-on. Decision: [ADR-0012](adr/0012-project-root-vs-framework-root.md). Verify: [`docs/verify/2026-06-10-installer-v2.md`](verify/2026-06-10-installer-v2.md).

**Depends on.** Phases 1–5.

---

# Track 2 — Distribution vs. repository (Phases 7–12)

**The problem (found 2026-06-10, verifying installer-v2 against `react-weather-app`).** "Install"
means "clone the dev repo": a governed project receives the gatekeeper source, the framework's own
`docs/`, `RESEARCH.md`, and the full git history — the workshop instead of the tool — while the one
artifact it genuinely needs, the operating contract in **its own** `CLAUDE.md`/`AGENTS.md`, is never
delivered (Claude Code does not read `.topology/CLAUDE.md`). Two outright bugs compound it: the
pre-commit hook installs into `.topology/.git/hooks/` (guarding the vendored copy, not the project),
and the post-install `doctor` runs from inside `.topology/`, so it reports project == framework and
`artifacts root: .topology/docs` — verifying the wrong world.

**The fix in one line.** Introduce a **distribution payload** distinct from the repository, and make
`adapt` deliver the contract into the governed project.

**Decisions (2026-06-10).**
- **Plugin channel retired.** The Claude Code plugin path (`/plugin install topology@topology`,
  SessionStart self-provisioning) is untested and duplicates the installer; remove it rather than
  port it to the payload model (Phase 8).
- **Upgrade = replace `.topology/` wholesale.** A local-customization overlay is deferred; forking
  the framework is the interim customization story. `react-weather-app` is the reference fixture
  for upgrade behaviour (Phase 12).
- **Contract delivery is append-only, import-first.** Never clobber or regenerate the user's
  `CLAUDE.md`/`AGENTS.md`. Claude Code: append one `@.topology/CONTRACT.md` import line (create the
  file with just that line when missing) — upgrades then never touch the user's file, because the
  imported contract lives in the payload. Harnesses without imports (codex): append an idempotent
  managed block instead.
- **Project-doc init is agent-driven, first-session, not install-time.** `adapt` stays
  deterministic (bare stub only); the contract / getting-started skill instructs the *first agent
  session* to detect the bare stub and write the project's own documentation above the import.
  `gatekeeper` never calls an LLM. An opt-in `adapt --init-agent` shell-out (claude-only, tty
  prompt, init-then-append order) is a stretch deliverable, not the default.

**Target layout** (local scope; global is the same payload at `~/.topology` with c–f only in the project):

```
<project>/
├── .topology/                  # gitignored, installer-managed, replaceable payload
│   ├── bin/gatekeeper           # (a) the enforcer
│   ├── hooks/*.sh               # (b) routing + scan glue
│   ├── security/rules.toml      # (b)
│   ├── skill-rules.json         # (b)
│   ├── skills/  instincts/      # what activate routes into
│   └── VERSION                  # payload version; drives upgrade/skew checks
├── .claude/
│   ├── settings.json            # (c) hook wiring + GATEKEEPER_BIN env
│   └── topology/                # (d) committed gate artifacts
│       └── research/ specs/ plans/ verify/ reviews/
├── CLAUDE.md                    # (e) managed Topology block (the operating contract)
└── .git/hooks/pre-commit        # (f) into the PROJECT's git, not .topology/.git
```

The payload is harness-neutral, so it does **not** live under `.claude/` — `.claude/topology/` holds
only the project's committed gate artifacts; `.topology/` holds the disposable, versioned payload.

---

## Phase 7 — Distribution payload

**Goal.** A release artifact that is the unit of install — the tool without the workshop.

**Deliverables.** *(Design grilled 2026-06-10; decisions below.)*
- **Platform-neutral payload** (decision: one tarball, not four per-platform ones): CI builds
  `topology-payload.tar.gz` (stable asset name, so `releases/latest/download/` resolves without
  knowing the version; version lives inside) + an entry in the existing `SHA256SUMS`. Contents:
  `hooks/{skill-activation,security-scan,pre-commit,learn-capture}.sh`, `hooks/skill-rules.json`,
  `skills/`, `instincts/`, `security/rules.toml`, `scripts/fetch-gatekeeper.sh` (the installer runs
  it post-unpack; it switches from reading `plugin.json` to the payload `VERSION`), `VERSION`, and
  a reserved `CONTRACT.md` slot (Phase 10). The repo layout already matches the payload layout —
  assembly is a copy list. The platform-specific binary rides the existing four-target release
  pipeline and is fetched into the payload's `bin/` at install time.
- Explicitly excluded: `gatekeeper/` source, `docs/`, `RESEARCH.md`, `METHODOLOGY.md`, `.github/`,
  `.claude-plugin/`, git history, `memory/TEMPLATE.handoff.md` (compiled into the binary via
  `include_str!`), `adapters/` (documentation only), `hooks/ensure-gatekeeper.sh` + `hooks/hooks.json`
  (plugin-only; retired in Phase 8).
- `VERSION` file: line-anchored TOML (`version`, `rules_schema`) — parseable by the bash `grep`
  idiom and the `toml` crate alike; consumed by `doctor`, the stale-binary check, and
  `fetch-gatekeeper.sh`. Version resolution (decision): the installer always takes the **latest
  release** (`TOPOLOGY_VERSION` env overrides for pinning/rollback); the single source of truth is
  `gatekeeper/Cargo.toml`, tag-guarded by CI.
- **The payload is read-only at runtime** (decision): `gatekeeper` never writes inside it after
  install. Mutable state moves from `framework_root()` to `artifacts_root()` — memory handoffs to
  `.claude/topology/memory/`, the learn ledger to `.claude/topology/learn/ledger.md` in governed
  projects (the framework repo's ledger path is unchanged; its `memory/artifacts/` migrates to
  `docs/memory/`). `learn promote` becomes framework-only: in a governed project it refuses and
  points at the fork story, instead of writing into the replaceable payload.

**Verify.** A tagged release carries the payload; unpacking it yields a tree where
`bin/gatekeeper --version` (post-fetch), `activate`, and `scan` all work with `TOPOLOGY_ROOT`
pointed at the unpacked dir; the tarball contains no `*.rs`, no `docs/`, no `.git`; `memory write`
and `learn capture` in a governed fixture write under `.claude/topology/`, and `learn promote`
there refuses.

**Depends on.** Phase 6 (release matrix exists).

---

## Phase 8 — Installer v3

**Goal.** Both scopes consume the payload; the install verifies the world the user will run in.

**Deliverables.**
- `install.sh` downloads + unpacks the payload to `~/.topology` (global) or `<project>/.topology`
  (local) instead of `git clone`; `--build-from-source` remains, building *into* the payload layout.
- Pre-commit hook installed into the **project's** `.git/hooks/pre-commit` (fix the `$ROOT/.git`
  target bug).
- The `sudo ln -sf … /usr/local/bin` suggestion removed (superseded by `GATEKEEPER_BIN` wiring in
  Phase 9); stale-PATH repair retained.
- Post-install `doctor` runs from the **project root**, not from inside the payload.
- **Retire the plugin channel**: remove `.claude-plugin/` (manifest + marketplace listing), the
  SessionStart self-provisioning hook (`hooks/ensure-gatekeeper.sh`, `hooks/hooks.json`), and the
  plugin sections from `README.md` and the installer's post-install notes. One install channel:
  this script.
- **Legacy-clone migration** (decision 2026-06-10): on detecting a clone-based `.topology/` (has
  `.git`), rescue mutable state into the artifacts root (`memory/artifacts/*` →
  `.claude/topology/memory/`, `docs/learn/ledger.md` → `.claude/topology/learn/`), prompt before
  deleting the clone (`--yes` honors the non-interactive path), then unpack the payload in its
  place. No silent deletion; no clone-beside-payload state.

**Verify.** Local install of a fixture project: `.topology/` contains payload files only; the
fixture's own `.git/hooks/pre-commit` blocks a planted secret; `doctor` (run from the fixture root)
reports project root = fixture, framework root = payload, artifacts root =
`<fixture>/.claude/topology`.

**Depends on.** Phase 7.

---

## Phase 9 — `adapt` v2: project integration

**Goal.** `adapt` delivers everything a governed project needs — including the contract.

**Deliverables.**
- Scaffold `.claude/topology/{research,specs,plans,verify,reviews}/` in the project.
- `.claude/settings.json` gains an `env` block setting `GATEKEEPER_BIN` to the resolved binary path,
  so `gatekeeper check …` works for the agent with no PATH or sudo step.
- Deliver the operating contract **append-only, import-first**: for Claude Code, append one
  `@.topology/CONTRACT.md` import line to the project's `CLAUDE.md` (create the file with just that
  line when missing) — the contract body lives in the payload, so upgrades never touch the user's
  file. For harnesses without imports (codex `AGENTS.md`), append an idempotent **managed block**
  (begin/end markers; re-running updates the block in place; `--check` flags drift). Never clobber
  or regenerate user content in either mode.
- Equivalent contract delivery for cursor/opencode targets (each harness's native
  always-on surface).
- First-session init: the contract (or `_getting-started` skill) gains a gate-shaped instruction —
  *project doc is a bare Topology stub → analyze the codebase and write the project's documentation
  above the import → then proceed*. Stretch: opt-in `adapt --init-agent` (claude-only, tty-prompted,
  runs `claude -p "/init"` **before** appending the import).

**Verify.** On a fixture with a pre-existing `CLAUDE.md`: one `adapt` run appends only the import
line and touches nothing else; a second run is a no-op (`--check` exits 0); on a fixture with no
`CLAUDE.md`, the file is created with just the import; the codex managed block round-trips
(edit → re-run → restored); a Claude Code session in the fixture sees the gate sequence in context
and `gatekeeper check design --feature x` runs bare; on a bare-stub fixture, the first session is
instructed to write the project's documentation above the import.

**Depends on.** Phase 8 (payload paths to point at); Phase 10 (the template it injects).

---

## Phase 10 — Contract split

**Goal.** Separate the portable operating contract from the framework-dev conventions now conflated
in `AGENTS.md`.

**Deliverables.**
- A portable **contract template** (ships in the payload): gate sequence, rules-vs-gates framing,
  `gatekeeper` commands with the resolved binary path, **parameterized artifact paths**
  (`.claude/topology/…` for governed projects — fixing the hardcoded `docs/specs/…` in today's
  `AGENTS.md`), and the memory/compaction protocol. `adapt` renders it at inject time (it knows
  both roots) and writes the result to `.topology/CONTRACT.md` — the file the `@import` points at.
- A framework-dev doc (stays in the repo, never ships): the Rust/Bash/Markdown stack conventions,
  skill house format, contribution workflow.
- The framework repo's own `AGENTS.md` becomes contract-template-instantiated-for-the-framework +
  the dev doc — the repo dogfoods its own template.

**Verify.** The payload contains the template and no dev doc; rendering the template for a governed
project yields only `.claude/topology/…` paths; rendering it for the framework repo yields `docs/…`
paths; no skill or instinct references a repo-only path.

**Depends on.** Phase 7 (a payload to ship it in). Parallel with Phases 8–9.

---

## Phase 11 — Root resolution hardening

**Goal.** `framework_root()` can never land somewhere surprising; `doctor` reports the project's
perspective.

**Deliverables.**
- Resolution order: `$GATEKEEPER_BIN`-adjacent payload → `$TOPOLOGY_ROOT` → `<project>/.topology` →
  `~/.topology` — never a bare upward marker walk (kills the `~/skills` hijack, where a run outside
  the repo resolved framework root to `$HOME`).
- `doctor` resolves from the project root, names both roots, and **fails** (non-zero) when a
  governed project resolves project == framework.
- Version-skew check reads the payload `VERSION` against the binary.

**Verify.** Unit tests for each resolution step and the `$HOME`-hijack regression; `doctor` in a
governed fixture exits non-zero when run from inside `.topology/`; skew between `VERSION` and the
binary is reported.

**Depends on.** Phase 7 (`VERSION` exists). Parallel with Phases 8–10.

---

## Phase 12 — End-to-end re-verification

**Goal.** Prove the consumer-visible outcomes on the real reference project.

**Deliverables.** Wipe `react-weather-app`'s current install; run both scopes (`--global`, then
separately `--project`); record the verify artifact.

**Verify.** Five outcomes, each evidenced:
1. the agent sees the operating contract in its context (project `CLAUDE.md` managed block);
2. `gatekeeper check …` runs bare in a session (via `GATEKEEPER_BIN`);
3. the `UserPromptSubmit` and `PreToolUse` hooks fire;
4. the project's own pre-commit blocks a planted secret;
5. a design artifact lands in `<project>/.claude/topology/specs/`.

**Depends on.** Phases 8–11.

---

## Status at a glance

| Phase | Capability | Status |
|---|---|---|
| 0 | Blueprint (docs + diagrams + roadmap) | ✅ delivered |
| 1 | Security scanning | ✅ delivered |
| 1.5 | Code-review gate (pulled forward) | ✅ delivered |
| 2 | Instincts engine | ✅ delivered |
| 3 | Continuous learning | ✅ delivered |
| 4 | Cross-harness adapters | ✅ delivered |
| 5 | Memory + research-first | ✅ delivered |
| 6 | Packaging & CI | ✅ delivered |
| 7 | Distribution payload | ⬜ planned |
| 8 | Installer v3 | ⬜ planned |
| 9 | `adapt` v2: project integration | ⬜ planned |
| 10 | Contract split | ⬜ planned |
| 11 | Root resolution hardening | ⬜ planned |
| 12 | End-to-end re-verification | ⬜ planned |
