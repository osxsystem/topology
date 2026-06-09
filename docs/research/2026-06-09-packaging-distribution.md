# Research: Packaging & distribution (Phase 6)

- **Status:** 🟢 **Approved 2026-06-09** — design-gate sign-off; the one open product fork (binary-in-plugin
  model, Part C / D#1) is resolved to **model (1): system-PATH**. Revised and re-confirmed across two Codex (GPT-5.5)
  design-gate review passes before approval. **Pass 1** raised five improvements (system-PATH framed as an explicit
  product compromise, a binary-resolution failure matrix, a Rust version/schema interface guardrail,
  precise "single binary ≠ static musl" language, and architecture-doc drift to fix *before* a
  docs-coverage lint can enforce ADR-index linkage). **Pass 2** caught six factual deltas in those edits,
  now corrected: the binary-resolution matrix must **split by hook type** (PreToolUse `scan` is
  fail-*closed*, UserPromptSubmit `activate` is fail-*open* — verified against the real hooks), Codex CLI
  and OpenCode **do** have plugin systems now (so the other-harness story is a scope cut, not a capability
  gap), the version seam has **two** consumers (`--version`+`doctor`, not `check docs`), README drifts
  (advertises a `.claude-plugin/` that doesn't exist), ADR-0010 **already exists** (was provisional, now Accepted — not
  "to-be-written"), and resolution transparency is **`doctor`'s** job, not every hook's. Search this file
  for `[rev:codex-2026-06-09]` to see what changed. Downstream: with this sign-off, ADR-0010, the spec, and the plan are **re-confirmed** against the corrected research and model (1) — ADR-0010 → **Accepted**, spec → **Approved**.
- **Date:** 2026-06-09
- **Feature slug:** packaging-distribution
- **Question:** Topology is a single Rust binary (`gatekeeper` 0.1.0) plus Markdown operators, wired into
  four harnesses by `adapt`. To "package & distribute" it (per `docs/ROADMAP.md` Phase 6) we need:
  per-harness installer flows, plugin manifests (`.claude-plugin/`, equivalents), version pinning
  (`gatekeeper --version` + a rules-schema version), a `doctor` health check, and CI that runs the
  quality gate on a fresh clone. **What already exists, what's the target convention for each piece, and
  what are the genuine design forks** — especially: how does a compiled-per-machine binary ship inside a
  Claude Code plugin that expects a prebuilt `bin/`?

> Grounding tracks, kept separate (instinct: [[evidence-over-assertion]]). **Part A** is a first-hand read
> of this repo's current state (every claim cited `file:line`, from a fan-out exploration). **Part B** is
> the target convention per sub-area, with the Claude-Code plugin facts verified live against the docs via
> the `claude-code-guide` (2026-06-09). **Part C** is the binary-distribution tension that ties several
> decisions together. **Part D/E** are the open forks for the design gate and a recommended scope.

---

## Part A — Current state (first-hand, `file:line`)

The ROADMAP frames Phase 6 as greenfield, but a surprising amount of the *quality-gate* half already
exists locally — it just never runs in CI. The gaps are real but narrower than the roadmap implies.

### A1. The quality gate is already defined — but only runs by hand

- **`justfile` already encodes the full gate.** `check: fmt-check lint test shell typos` (offline-safe)
  and `ci: check deny links` (adds network/slow). The recipes shell out to:
  `cargo fmt --manifest-path gatekeeper/Cargo.toml --check`, `cargo clippy … -- -D warnings`,
  `cargo test …`, `shellcheck hooks/*.sh scripts/*.sh`, `typos`,
  `cargo deny --manifest-path gatekeeper/Cargo.toml check`, and
  `lychee --config lychee.toml 'docs/**/*.md' '*.md'`. (`justfile:1-52`.)
- **`lychee.toml` exists** — a full link-check config (20s timeout, 2 retries, accept 200–299/403/429,
  excludes `target/`/`.git/`/`.claude/`, skips mailto/loopback/private IPs).
- **`shfmt` is deliberately advisory** — explicitly kept *out* of `check`/pre-commit because the scripts
  use a hand-aligned style shfmt would undo (`justfile` `shfmt` recipe comment). Honor that in CI.
- **`cargo deny` implies a `deny.toml`** under `gatekeeper/` (license/advisory audit).
- **No `.github/` directory exists** — zero CI config. The gate is defined but **nothing runs it on push
  or PR**. This is the single biggest real gap.

→ *Implication:* Phase 6's "CI" deliverable is mostly **wiring `just ci` into GitHub Actions on a fresh
clone**, not designing the checks. Don't rebuild the justfile; mirror it.

### A2. Versioning exists in two disconnected places

- **Binary version:** `gatekeeper/Cargo.toml:4` → `version = "0.1.0"`. The crate **never reads its own
  version** — no `CARGO_PKG_VERSION` usage anywhere, and **no `--version`/`-V`/`version` handling** in the
  dispatch (`main.rs:51-73`; `--help|-h|None` → `print_help()`, unknown → error + exit 2). `print_help()`
  (`main.rs:75-99`) hardcodes usage with no version embedded.
- **Rules-schema version:** `scan.rs:17` → `const SCHEMA_VERSION: u32 = 1`; `security/rules.toml:1` carries
  `schema_version = 1`; the loader **rejects** any mismatch (`scan.rs:146-149`). `instinct.rs:12` has its own
  `const SCHEMA_VERSION: u32 = 1`, likewise enforced on load (`instinct.rs:152-154`). **Memory artifacts are
  unversioned** — `memory::render` (`memory.rs:54-80`) stamps only `feature`/`created`/`branch`/`head_sha`/
  `status`/`verified_by`, with no `schema_version`. (The `schema_version = 1` strings at `memory.rs:405,417,437`
  are scan-rule TOML fixtures inside unit tests, not memory frontmatter.)

→ *Implication:* where schema versioning exists (scan rules, instincts) it is real and enforced, but
**decoupled from the binary version** — a `0.2.0` binary still accepts `schema_version = 1`. That
decoupling is arguably *correct* (data-format compat ≠ tool version), but it's currently implicit — and
**memory artifacts carry no version field at all**, a packaging-relevant gap if their format ever changes.
Design must decide whether `--version` surfaces the schema versions, the bump/compatibility policy, and
whether memory artifacts should be versioned too.

### A3. Install today builds from source; no PATH install, no manifests

- **`scripts/install.sh`** (`:1-74`): checks for cargo (fails gracefully), runs `cargo build --release`
  (binary lands at `gatekeeper/target/release/gatekeeper`), symlinks `CLAUDE.md → AGENTS.md`,
  `chmod +x hooks/*.sh scripts/*.sh`, copies `hooks/pre-commit.sh` → `.git/hooks/pre-commit`, and prints
  the optional `adapt` commands. It **does not** install onto `PATH` (suggests a manual `sudo ln -sf`),
  emit any plugin manifest, or pin a version.
- **`adapt.rs`** emits four harness targets (`.codex/config.toml`, `.cursor/rules/*.mdc`, `opencode.json`
  + `.opencode/…`, `.claude/settings.json`) with a `--check` drift mode. **No plugin manifest builder
  exists** — `.claude-plugin/plugin.json` is *mentioned aspirationally* in `README.md:28-29,78` but never
  generated; grep for "plugin"/"marketplace"/"manifest" finds only docs, no code.
- **`.claude/settings.json`** embeds **absolute, machine-local hook paths** (per ADR-0003) — it is
  produced per-machine by `adapt`/`install.sh`, never committed. This is the key fact for plugin
  packaging (see Part C).

### A4. ADR landscape

ADRs 0001–0009 predate Phase 6 and **none** touch packaging, CI, versioning, or release. ADR-0003 (one
Markdown source; generated per-harness configs; "outputs are build artifacts, never hand-edit") is the
closest and governs how a plugin's generated files relate to source.

**Update (`[rev:codex-2026-06-09]`):** ADR-0010 (`docs/adr/0010-packaging-distribution.md`) has since been
written and — with this research's sign-off (model (1) system-PATH) — is **re-confirmed *Accepted*** (it
was re-opened to *Proposed* across these two review passes). The deltas those passes folded in:
the hook fail-behaviour (C2 — its decision 1 wording predates the fail-closed/fail-open split), the
version seam's real consumers (B1 — `--version`+`doctor`, not `check docs`), the Codex/OpenCode packaging
scope (B5 — they *do* have plugin systems), and the docs-reconciliation precondition (A6). It remains
**absent from the ADR index** (the index stops at 0007; see A6) until the Task-3 reconciliation — which the
docs-coverage lint would flag.

### A5. Scorecard vs the ROADMAP Phase 6 deliverables

| ROADMAP deliverable | Status today | Evidence |
|---|---|---|
| CI job *definition* (Rust + shell + spell + deny + links) | ✅ exists | `justfile`, `lychee.toml` |
| CI *execution* on push/PR (`.github/workflows`) | ❌ none | no `.github/` |
| Markdown link check | ✅ tooling ready | `lychee.toml`, `justfile links` |
| Docs-**coverage** lint | ❌ undefined | nothing |
| `gatekeeper --version` | ❌ none | `main.rs:51-73` |
| Rules-schema version pin | ✅ enforced (decoupled) | `scan.rs:17,146-149` |
| `gatekeeper doctor` (optional) | ❌ none | — |
| Per-harness installer flows | ⚠️ partial (`install.sh` + `adapt`) | `scripts/install.sh`, `adapt.rs` |
| Plugin manifests (`.claude-plugin/` etc.) | ❌ none (aspirational mention) | `README.md:28-29,78` |
| Release: tagged binary (ROADMAP says "static" — see B3, imprecise) | ❌ none | profile is size-optimized only |

### A6. The architecture docs already drift from reality — a prerequisite, not a footnote `[rev:codex-2026-06-09]`

Phase 6 proposes a *docs-coverage lint* (Part B6) that would, among other things, enforce "every
`docs/adr/00NN-*.md` is linked from `docs/adr/README.md`." But that invariant is **already violated**, so
the lint can't be switched on until the docs are reconciled:

- **The ADR index stops at 0007.** `docs/adr/README.md:7-15` lists 0001–0007; **0008, 0009, and 0010 are absent** from the index even though all three files now exist (0010 has
  since been authored and is now *Accepted* — no longer "to-be-written", per the revision note above). A docs-coverage lint asserting index-linkage would fail on day one.
- **`ARCHITECTURE.md` labels delivered modules `[planned]`.** It tags `scan`, `instinct`, `learn`, `adapt`,
  `check research`, `security-scan.sh`, `pre-commit.sh`, and the Codex/Cursor/OpenCode adapters as
  `[planned]` (`docs/ARCHITECTURE.md:30,57-58,93,118-121,135-137,150,159,188-192`), while the ROADMAP marks
  Phases 1–5 **delivered** (`docs/ROADMAP.md:7`). The doc describes a system two phases behind the code.
- **`README.md` advertises a `.claude-plugin/` that doesn't exist** (`[rev:codex-2026-06-09]`, second
  review). The "What's in here" tree shows `.claude-plugin/plugin.json # Claude Code packaging`
  (`README.md:28-29`) and the "Make it yours" step references packaging "next to `.claude-plugin/`"
  (`README.md:78`) — but `ls .claude-plugin` returns *no such file or directory* (verified). README also
  repeats the imprecise "single static binary" claim (`README.md:82`) — the same wording B3 corrects. So
  README is **doc-ahead-of-code** in one place and imprecise in another.

→ *Implication:* this is a **sequencing constraint**, not just cleanup. Codex's review is right that the
docs must be reconciled with delivered reality **before** any docs-coverage lint enforces these
invariants — otherwise Phase 6 ships a gate that red-flags the repo it ships in. The reconciliation set is
**three files**: `docs/adr/README.md` (extend the index to 0008–0010), `docs/ARCHITECTURE.md` (flip
shipped `[planned]`→`[built]`; fix "static binary"), and `README.md` (the phantom `.claude-plugin/` —
either *create* the manifest this phase, since we're building the plugin anyway, or correct the tree; and
fix "static binary"). Note the README tension cuts the other way from the rest: there the *code* must
catch up to the doc (build the plugin) rather than the doc to the code. This makes docs-reconciliation a
*gating sub-task* of the docs-lint deliverable, surfaced as a fork in Part D.

---

## Part B — Target conventions per sub-area

### B1. Binary version (`--version`)

- Standard Rust idiom: print `env!("CARGO_PKG_VERSION")` (compile-time, from Cargo.toml) for `--version`
  / `-V`. Cheapest possible — no new dep, one match arm before `print_help`. This is the single most
  load-bearing prerequisite for everything else (releases, plugin manifest version, debugging, `doctor`).
- **Decision space:** should `--version` also print the supported rules `SCHEMA_VERSION` (and instinct
  schema)? A one-line `gatekeeper 0.1.0 (rules schema v1)` is honest and nearly free.
- **Interface guardrail (`[rev:codex-2026-06-09]`).** `scan::SCHEMA_VERSION` is a **private** `const` in
  `scan.rs:17` (verified: `const SCHEMA_VERSION: u32 = 1;`, used only internally at `scan.rs:146-149`).
  **Two** new consumers want it: `--version` and `doctor`. *(Not the docs-coverage lint — an earlier draft
  wrongly listed `check docs` here. That lint checks skill frontmatter, ADR-index links and ROADMAP verify
  references (B6); none of those read `SCHEMA_VERSION`, so it is not a version-seam consumer. `[rev:codex-2026-06-09]`)*
  Reaching into a private constant — or worse, re-declaring `1` in each — couples them to an
  implementation detail and invites drift (the exact failure mode A2 already flags between binary and
  schema versions). The clean shape is a **small internal version interface** the two consumers depend on
  instead of the raw const: e.g. a `version` module exposing `pub fn tool() -> &'static str` (wrapping
  `env!("CARGO_PKG_VERSION")`) and `pub fn rules_schema() -> u32` (re-exporting `scan`'s value through one
  public seam), with the `instinct`/memory schema added the same way if surfaced. `--version` and `doctor`
  then read the interface, not the constant. This keeps the module boundary clean as packaging logic grows
  and is the one architectural guardrail Codex recommends adding *before* implementation. Still no new
  dependency — it's a visibility/ownership refactor, not new machinery.
- **Release-tag ↔ Cargo version policy:** today there is no automation. Convention is a git tag `vX.Y.Z`
  whose number matches `Cargo.toml`'s `version`; CI can *assert* they match on a tag build (cheap guard
  against the "forgot to bump" footgun the plugin docs warn about, Part B4).

### B2. CI (GitHub Actions)

- Repo is `github.com/osxsystem/topology` → **GitHub Actions** is the obvious platform (no fork worth
  debating). The job is to **mirror `just ci`** on a fresh clone.
- Standard Rust CI shape: pinned toolchain (`dtolnay/rust-toolchain@stable` or a pinned version) with
  `rustfmt` + `clippy` components, a build cache (`Swatinem/rust-cache`), then run the gate. `just` itself
  is installable in CI (`taiki-e/install-action` or `cargo install just`), letting CI call the *same*
  recipes the human runs — single source of truth, no drift between local and CI gates.
- **Offline vs network split matters in CI:** `fmt-check`/`lint`/`test`/`shell`/`typos` are hermetic;
  `links` (lychee) and `deny` (advisory DB fetch) hit the network. Options: run the full `ci` target and
  accept occasional link-flake, or split `links` into a separate non-blocking/scheduled job. lychee's
  retry/accept config already softens flake.
- **Tools needed in CI:** `shellcheck`, `typos`, `lychee`, `cargo-deny`, `just`. Each has a maintained
  install action or `cargo install`. Triggers: `push`/`pull_request` for the gate; `push: tags: v*` for
  the release job (B3).
- **`gatekeeper scan --staged`** is the pre-commit floor (`hooks/pre-commit.sh:22`); CI could optionally
  run `gatekeeper scan` over the diff too, but the pre-commit hook + human review already cover it —
  weakest-enforcement-that-works ([[weakest-enforcement-that-works]]) says don't duplicate it unless CI
  is the only enforcement point.

### B3. Release / binary distribution

- `[profile.release]` is already size-optimized (`opt-level="z"`, `lto=true`, `strip=true`) — good for a
  shipped CLI. Missing: the *act* of producing and publishing artifacts.
- **Terminology precision (`[rev:codex-2026-06-09]`).** The ROADMAP and ARCHITECTURE.md say "static
  binary," and the scorecard below echoes it — but that phrase overclaims for the chosen target. The
  release target is **`aarch64-apple-darwin`** (Part C / the macOS dev platform): that produces a *single
  self-contained executable* (std-only, no runtime crate deps to install), but macOS has **no static libc**
  — it dynamically links `libSystem`. A *fully static* artifact is a Linux/musl concept
  (`x86_64-unknown-linux-musl`), which we are **not** building this phase. So the honest claim is "a single
  std-only executable for macOS arm64," not "a static binary." The verify note and any release docs must
  use the precise phrasing; the loose "static" wording in ROADMAP/ARCHITECTURE should be corrected as part
  of the docs reconciliation (A6) so the docs-coverage lint isn't enforcing an inaccurate claim.
- Convention: a **tag-triggered** workflow (`on: push: tags: v*`) that builds the release binary and
  attaches it to a GitHub Release (e.g. `softprops/action-gh-release`). The real fork is the **target
  matrix** — see Part C; it's entangled with the plugin-binary question.
- Cargo metadata gaps for any external publish: `Cargo.toml` lacks `authors`, `repository`, `homepage`,
  `keywords`. Cheap to add; required only if we ever publish to crates.io (probably out of scope — this
  is a tool, not a library).

### B4. Claude Code plugin + marketplace (verified live 2026-06-09)

Authoritative facts (docs: `code.claude.com/docs/en/plugins`, `/plugins-reference`,
`/plugin-marketplaces`):

- **Manifest `.claude-plugin/plugin.json`** — only required field is `name` (kebab-case). Recommended:
  `version` (semver — **must be bumped every release or updates don't propagate**), `description`,
  `author.name`, `displayName`. Component path fields: `skills`, `commands`, `agents`, `hooks`,
  `mcpServers`, `lspServers`. Unknown top-level fields are ignored (cross-tool manifests OK).
- **Layout:** only `plugin.json` lives in `.claude-plugin/`; components sit at **plugin root** —
  `skills/<name>/SKILL.md`, `commands/*.md`, `agents/*.md`, `hooks/hooks.json`, `.mcp.json`, and a
  `bin/` dir whose contents are added to PATH. **Our existing `skills/` and `hooks/` already match this
  layout** — that's a big tailwind.
- **Hooks bundled in a plugin** fire like user hooks but run with `${CLAUDE_PLUGIN_ROOT}` (install dir,
  changes on update) and `${CLAUDE_PLUGIN_DATA}` (persistent) set. A plugin hook **can invoke a bundled
  binary**: `"command": "\"${CLAUDE_PLUGIN_ROOT}\"/bin/gatekeeper"`. This is exactly how we'd replace the
  absolute-path `.claude/settings.json` wiring (A3) with a portable plugin.
- **Marketplace `.claude-plugin/marketplace.json`** at a repo root: `name`, `owner{name,email}`,
  `plugins[]` (each with `name`, `source`, optional `version`). `source` can be a relative path,
  `{source:"github", repo:"osxsystem/topology", ref:"v1.0.0"}`, a git URL, a git-subdir (monorepo), or
  npm. User flow: `/plugin marketplace add osxsystem/topology` → `/plugin install topology@<marketplace>`.
- **No Claude-Code-version pin field** exists in the manifest; feature-version requirements (e.g.
  `displayName` needs v2.1.143+) are documented in README, not enforced.
- **Validation/test:** `claude plugin validate ./<plugin>` (`--strict` to fail on warnings);
  `claude --plugin-dir ./<plugin>` to test locally.

### B5. Other harnesses' "packaging" `[rev:codex-2026-06-09]`

*Correction — the original "they have no plugin system" claim was stale.* Codex and OpenCode now ship
real plugin systems; verified first-hand on this machine (2026-06-09):

- **Codex CLI 0.137.0** exposes `codex plugin {add,list,marketplace,remove}` — a marketplace-backed
  plugin system (OpenAI launched the Codex plugin marketplace 2026-03-27; plugins bundle skills + MCP
  configs across CLI/IDE/desktop). `codex plugin marketplace add <owner/repo|git-url|local>` mirrors
  Claude's flow closely.
- **OpenCode 1.14.22** has `opencode plugin <npm-module>` plus auto-loaded local plugins
  (`.opencode/plugins/*.ts` — JS/TS modules exporting hook functions) and npm packages listed in
  `opencode.json`.
- **Cursor** remains the exception — no plugin marketplace; distribution is still `.cursor/rules/*.mdc`
  (static rule injection).

So the honest framing is a **deliberate Phase-6 scope cut, not a capability gap**: we package **one real
plugin for Claude Code** and keep Codex/Cursor/OpenCode on the existing `adapt`-generated native config
(`.codex/config.toml`, `.cursor/rules/*.mdc`, `opencode.json`) + a documented install. Native Codex and
OpenCode plugins are now genuinely buildable and are the obvious *future* extension — recorded as a
reversible scope choice ([[surgical-changes-only]]), not "those harnesses lack plugin systems."
Weakest-enforcement-that-works ([[weakest-enforcement-that-works]]) still argues against fanning out to
four plugin formats in the terminal phase, but design should own that as a *choice* and the ROADMAP/ADR
language must stop asserting the harnesses can't be packaged.

### B6. `doctor` and docs-coverage lint (the soft/optional deliverables)

- **`gatekeeper doctor`** (roadmap marks optional): a health check — binary version, rules.toml present +
  schema_version supported, `instincts/`/`skills/` parse, hooks executable, git hooks installed. It's
  pure read-only introspection over machinery that already exists; a thin aggregator. *Roadmap-optional,
  but no longer architecturally optional (`[rev:codex-2026-06-09]`):* C2 shows `doctor` is the mitigation
  that makes the system-PATH model (1) tolerable — it's the one place that prints the resolved binary path
  + version + resolution mechanism. Recommend keeping it **in** Phase 6 for that reason, not as a nicety.
- **Docs-coverage lint** is the *undefined* half of "link/coverage lint." Candidate meaning: every skill
  has a `SKILL.md` with valid frontmatter, every ADR is linked from the index, every roadmap phase marked
  ✅ has a `docs/verify/` note, no orphan operators. This could be a new `gatekeeper` subcommand or a
  shell/CI check. Risk of scope creep — needs an explicit decision on whether it's in Phase 6 at all.
  **Hard precondition (`[rev:codex-2026-06-09]`):** per A6 the repo *already violates* the candidate
  invariants (ADR index stops at 0007; shipped modules tagged `[planned]`). The docs must be reconciled
  **first**, or the lint red-flags its own repo on introduction. Sequence: reconcile docs → define the
  lint narrowly (those three invariants, nothing more — keep it from becoming a generic lint bucket) →
  enable it.

---

## Part C — The crux: shipping a compiled binary inside a plugin

A Claude Code plugin expects a **prebuilt** executable in `bin/` (copied to a per-machine cache at install
time). But today `gatekeeper` is **compiled on the user's machine** by `install.sh` (`cargo build
--release`). These two models collide. Three resolutions (a real architectural fork for the design gate):

1. **Plugin references a system-installed `gatekeeper` on PATH.** The plugin ships hooks/skills only; the
   user installs the binary separately (`install.sh`/`cargo install`). Simplest plugin, but the binary
   isn't self-contained in the plugin and PATH resolution can fail — partially defeats "one-command
   install."
2. **Plugin bundles prebuilt platform binaries in `bin/`** (e.g. `bin/gatekeeper-aarch64-apple-darwin`,
   `…-x86_64-unknown-linux-gnu`) and a tiny shim selecting by platform. Requires the **release matrix**
   (B3) to cross-compile and the marketplace to host them — couples Phase 6's release job to the plugin.
   Most "it just works," heaviest CI.
3. **Plugin ships a build-on-install step** (a hook/script runs `cargo build` on first use). Self-contained
   source, but needs a Rust toolchain on the user's machine and adds first-run latency — basically today's
   `install.sh` wrapped as a plugin.

This fork drives: the release target matrix (B3), whether the marketplace hosts binaries, and how
ambitious "one-command install" really is. It is the **primary thing to resolve in design** and the
clearest candidate for a user decision. Note the project's own bias ([[weakest-enforcement-that-works]],
[[surgical-changes-only]]) leans toward (1) or a (1)+(2)-for-tagged-releases hybrid over a heavy
cross-compile matrix on every push.

### C1. System-PATH is a deliberate product compromise, not "one-command install" `[rev:codex-2026-06-09]`

Codex's review is right to flag the wording gap: the ROADMAP promises "one-command install per harness,"
but model (1) cannot deliver that literally. With a system-PATH binary, the Claude Code flow is
**three acts, not one**: (a) build/install the binary (`install.sh` or `cargo install`), (b)
`/plugin marketplace add osxsystem/topology`, (c) `/plugin install topology@…`. The plugin is pure
data + glue; the executable it drives is acquired separately. This is an acceptable trade — it avoids a
cross-compile matrix and keeps the plugin self-describing — **but design must state it as a compromise**,
not paper over it. "One-command install" becomes an *aspiration* that model (2) (bundled prebuilt
binaries) would later satisfy; (1) is the honest Phase-6 floor. The mitigation that makes (1) tolerable
is **legible diagnostics in `doctor`** — which *always* prints the resolved binary path + version — plus
hooks that fail *legibly on the unhappy path*: a missing binary yields an actionable message (a `deny`
reason for the scanner, an advisory line for activation), not a silently broken hook. The hooks stay
quiet when all is well (they already do — `security-scan.sh:28-30` is silent on allow); they do **not**
narrate resolution on every call. Resolution transparency is `doctor`'s job, not every hook's (C2).

### C2. Binary-resolution failure matrix — the highest-risk seam `[rev:codex-2026-06-09]`

Because the binary lives outside the plugin, *resolution* is where model (1) breaks. **Grounding first —
the two existing hooks have opposite fail policies *and* opposite resolution orders (verified, not
assumed):**

- **`hooks/security-scan.sh` (PreToolUse) is fail-*closed*.** Resolution order is **repo-built first**,
  then PATH: `…/target/release` → `…/target/debug` → `command -v gatekeeper` → else **`deny`**
  (`security-scan.sh:16-26`); a scanner *error* also denies ("failing closed", `:32`). The header states
  the contract outright — "a missing/erroring binary emits a deny" (`:4`). A security veto that fails
  *open* is worse than none, so this must not change.
- **`hooks/skill-activation.sh` (UserPromptSubmit) is fail-*open*.** Opposite order — **PATH first**, then
  repo-built — and a missing binary prints an advisory line and `exit 0` (`skill-activation.sh:16-25`);
  the comment is explicit, "Never fail the user's turn on hook error" (`:27`). Skill routing is advisory,
  not a boundary; blocking the prompt would be hostile.
- **Neither hook currently reads `$GATEKEEPER_BIN`** (verified — no such reference in either script). The
  override is a *proposed addition* for the plugin context, **not** current behaviour; design must add it,
  not assume it.

So "missing binary" has **no single answer** — it splits by hook type. The matrix (the concrete artifact
Codex asked for):

| # | Scenario | Detection | Required behaviour |
|---|---|---|---|
| 1a | **Missing binary — PreToolUse (`scan`)** | resolve/exec fails | **Fail-closed: emit `deny`** ("scanner unavailable — run ./scripts/install.sh", status quo `security-scan.sh:25`). Never allow silently — that disables the veto. |
| 1b | **Missing binary — UserPromptSubmit (`activate`)** | resolve/exec fails | **Fail-open:** advisory line, `exit 0` (status quo `skill-activation.sh:23`). `doctor` reports ❌ + the install command. |
| 2 | **Stale binary** — older than the plugin/rules expect | `--version` vs plugin `version` / rules `SCHEMA_VERSION` | `doctor` warns with both versions + the upgrade command; hooks still run (forward-compatible, A2), and a schema mismatch surfaces via `scan`'s existing reject (`scan.rs:146-149`). |
| 3 | **Unrelated/shadowing PATH binary** — some *other* `gatekeeper` | name collision (cf. the `rtk` precedent) | Already *partly mitigated*: `security-scan.sh` prefers the repo-built binary precisely so "a stale or unrelated PATH binary must not stand in for the real veto" (`:16-17`). For the plugin (no repo tree), `doctor` prints the **resolved absolute path** to eyeball, and `$GATEKEEPER_BIN` disambiguates. No signature/identity check this phase (known limit). |
| 4 | **`$GATEKEEPER_BIN` override** *(new — not in either hook today)* | env var present | Highest precedence, *then* each hook's existing order. Validate it exists + is executable; if not, fall to that hook's missing-binary policy (1a/1b) with a message naming the bad override. `doctor` shows the override is active and where it points. |
| 5 | **Local repo binary (dev)** — `…/target/release/…`, not on PATH | repo path exists | The dev path: `security-scan.sh` already prefers it, and `install.sh`/`adapt --harness claude` wire absolute paths (ADR-0003). `doctor` labels "repo dev binary" vs "PATH binary" so a contributor isn't misled. |
| 6 | **Plugin install** — hook runs under `${CLAUDE_PLUGIN_ROOT}` | plugin context | The repo `target/` won't exist, so resolution falls to `$GATEKEEPER_BIN` → PATH; the *script* stays portable via `${CLAUDE_PLUGIN_ROOT}`. PreToolUse still fail-closed, UserPromptSubmit still fail-open. Document that installing the plugin does **not** install the binary. |

→ *Implication (`[rev:codex-2026-06-09]`):* resolution transparency belongs to **`doctor`, not the
hooks**. `doctor` *always* prints the resolved path + version + which mechanism won (override →
repo-built/PATH → not-found). The **hooks stay quiet on the happy path** — `security-scan.sh` emits a
decision only when denying/erroring and is silent on allow (`:28-30`); they surface resolution only on a
failure/debug path, never on every call. This is the strongest argument for keeping `doctor` *in* Phase 6
(revises B6's "genuinely optional").

---

## Part D — Open questions / forks for the design gate

1. **Binary-in-plugin model** (Part C) — system-PATH (1), bundled prebuilt matrix (2), or build-on-install
   (3)? Drives the release matrix and CI weight. *(Strongest candidate for an explicit user decision.)*
2. **Scope of "plugin manifests, equivalents"** — one real Claude Code plugin + marketplace, and keep
   Codex/Cursor/OpenCode on the existing `adapt` + docs? *(`[rev:codex-2026-06-09]`: Codex CLI and
   OpenCode now have real plugin systems (B5), so this is a deliberate **scope cut**, not a forced choice
   — own it as such. A native `codex plugin` / OpenCode-plugin package is a viable but explicitly deferred
   extension.)*
3. **`adapt` vs. a new `package`/manifest command** — does the plugin/marketplace manifest get *generated*
   by a new `gatekeeper adapt --harness claude --plugin` (consistent with ADR-0003 "generated, never
   hand-edited"), or is `plugin.json`/`marketplace.json` hand-authored static source? (Generation keeps
   one-source discipline but adds code; static is simpler but another hand-maintained file.)
4. **`--version` content** — version only, or `gatekeeper X.Y.Z (rules schema vN)`? And do we add a CI
   guard asserting the git tag matches `Cargo.toml`?
5. **`doctor`** — in Phase 6 or deferred? (Roadmap says optional.)
6. **Docs-coverage lint** — define a concrete check and include it, or defer and ship only the (existing)
   link check as the "docs lint"? (Scope-creep risk.)
7. **CI network gates** — run `deny`+`links` in the blocking PR gate, or split them into a separate/
   scheduled job to avoid network flake blocking merges?
8. **Release matrix** — macOS-arm64 only (the dev platform), or +linux-x86_64 (CI runners / most users)?
   Entangled with #1. *(And state the artifact precisely: "single std-only macOS-arm64 executable," not
   "static binary" — B3 `[rev:codex-2026-06-09]`.)*
9. **Version/schema interface** *(`[rev:codex-2026-06-09]`, recommended-yes)* — introduce the small Rust
   `version` seam (B1) that `--version` and `doctor` depend on (not `check docs` — it's not a schema
   consumer), instead of reaching into the private `scan::SCHEMA_VERSION`? Codex recommends this as a
   pre-implementation guardrail; low cost, no new dep. *(Leaning: yes.)*
10. **Binary-resolution behaviour** *(`[rev:codex-2026-06-09]`)* — adopt the C2 failure matrix as the
    spec for hook + `doctor` behaviour (resolution transparency, `GATEKEEPER_BIN` override, non-blocking
    "not found")? This is the highest-risk seam of model (1) and should be pinned in design, not left to
    implementation.
11. **Docs reconciliation as a gating sub-task** *(`[rev:codex-2026-06-09]`)* — accept that the
    three-file reconciliation (A6: `docs/adr/README.md` index → 0008–0010; `ARCHITECTURE.md`
    `[planned]`→`[built]` + "static binary" fix; `README.md` phantom `.claude-plugin/` + "static binary")
    must land *before* the docs-coverage lint is enabled? And specifically for README's phantom manifest:
    **create `.claude-plugin/plugin.json` this phase** (we're building the plugin anyway — code catches up
    to doc) vs. edit the README tree? Or drop the docs-coverage lint from Phase 6 entirely and ship only
    the existing link check (#6), avoiding the reconciliation dependency?

---

## Part E — Recommended scope (weakest-enforcement-that-works, surgical)

Grounded in Part A (much of the gate already exists) and the project's instincts, a tight Phase 6 that
delivers the roadmap's intent without over-building:

0. **Reconcile the docs first** *(`[rev:codex-2026-06-09]`)* — the three-file set (A6): extend the ADR
   index to 0008–0010 (`docs/adr/README.md`), flip shipped `[planned]`→`[built]` and fix "static binary"
   in `ARCHITECTURE.md`, and resolve `README.md`'s phantom `.claude-plugin/` (create the manifest, since
   we're building the plugin anyway) + its "static binary" wording. Precondition for the docs-coverage
   lint and cheap.
1. **`--version`/`-V`** printing `CARGO_PKG_VERSION` (+ rules schema version) — the keystone. Trivial,
   unblocks everything. (`main.rs`, one arm + test.) Implement it on a **small Rust `version` interface**
   (B1 `[rev:codex-2026-06-09]`) that `doctor` shares — *not* `check docs` (it's not a schema consumer) —
   rather than the private `scan::SCHEMA_VERSION` const.
2. **GitHub Actions CI** that installs `just` + the four CLI tools and runs `just ci` (or `just check` as
   the blocking gate + a separate `links`/`deny` job) on push/PR — mirroring the existing local gate, no
   re-design. Plus a **tag → release** workflow building the size-optimized binary and attaching it to a
   GitHub Release.
3. **A real Claude Code plugin**: `.claude-plugin/plugin.json` + `marketplace.json`, reusing the existing
   root-level `skills/` and `hooks/` layout, with the hook *scripts* referenced portably via
   `${CLAUDE_PLUGIN_ROOT}` and the *binary* resolved per the C2 matrix (`$GATEKEEPER_BIN` → PATH, keeping
   PreToolUse fail-closed / UserPromptSubmit fail-open). Resolving Part C — recommend model
   **(1) system-PATH** or **(1)+(2) hybrid for tagged releases**, pending the user's call; frame (1) as an
   explicit compromise, **not** "one-command install" (C1). Other harnesses: keep `adapt` + a documented
   install flow — an explicit **scope cut**, since Codex/OpenCode now *do* have plugin systems we're
   choosing to defer (B5), not a capability gap.
4. **A small `doctor`** — recommend **in scope** (not merely "if approved"): C2 makes it the mitigation
   that keeps the system-PATH model legible. Pure read-only aggregation over existing machinery; its
   load-bearing output is binary-resolution transparency (resolved path + version + mechanism).
5. **ADR-0010** capturing the binary-in-plugin decision + the binary/schema-version policy.
6. Defer or minimize **docs-coverage lint** unless the user wants a concrete check — the link check
   already satisfies "docs lint."

> Net: Phase 6 is more *wiring and packaging* than new engine code. The justfile/lychee gate already
> exists (don't rebuild it — run it in CI); the binary just needs to *announce its version* (via a clean
> Rust `version` seam, not the private const) and *ship*; the plugin layout already matches our tree. The
> one genuinely hard decision is **how the compiled binary travels inside a plugin** (Part C) — resolve
> that first in design, and spec its resolution failures (C2) since that seam is where model (1) breaks.
> Two cheap-but-load-bearing preconditions the Codex review surfaced: reconcile the architecture docs
> (A6) before enabling a docs-coverage lint, and call the system-PATH model an explicit compromise rather
> than "one-command install" (C1).
