# Research: Packaging & distribution (Phase 6)

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
  `schema_version = 1`; the loader **rejects** any mismatch (`scan.rs:146-149`). `instinct.rs` has its own
  `SCHEMA_VERSION = 1`; memory frontmatter stamps `schema_version = 1` (`memory.rs:405,417,437`).

→ *Implication:* schema versioning is real and enforced, but **decoupled from the binary version** — a
`0.2.0` binary still accepts `schema_version = 1`. That decoupling is arguably *correct* (data-format
compat ≠ tool version), but it's currently implicit. Design must decide whether `--version` surfaces
both, and the bump/compatibility policy.

### A3. Install today builds from source; no PATH install, no manifests

- **`scripts/install.sh`** (`:1-74`): checks for cargo (fails gracefully), runs `cargo build --release`
  (binary lands at `gatekeeper/target/release/gatekeeper`), symlinks `CLAUDE.md → AGENTS.md`,
  `chmod +x hooks/*.sh scripts/*.sh`, copies `hooks/pre-commit.sh` → `.git/hooks/pre-commit`, and prints
  the optional `adapt` commands. It **does not** install onto `PATH` (suggests a manual `sudo ln -sf`),
  emit any plugin manifest, or pin a version.
- **`adapt.rs`** emits four harness targets (`.codex/config.toml`, `.cursor/rules/*.mdc`, `opencode.json`
  + `.opencode/…`, `.claude/settings.json`) with a `--check` drift mode. **No plugin manifest builder
  exists** — `.claude-plugin/plugin.json` is *mentioned aspirationally* in `adapters/README.md` but never
  generated; grep for "plugin"/"marketplace"/"manifest" finds only docs, no code.
- **`.claude/settings.json`** embeds **absolute, machine-local hook paths** (per ADR-0003) — it is
  produced per-machine by `adapt`/`install.sh`, never committed. This is the key fact for plugin
  packaging (see Part C).

### A4. ADR landscape

ADRs 0001–0009 exist; **none** touch packaging, CI, versioning, or release. ADR-0003 (one Markdown
source; generated per-harness configs; "outputs are build artifacts, never hand-edit") is the closest and
governs how a plugin's generated files relate to source. → Phase 6 needs a **new ADR-0010**.

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
| Plugin manifests (`.claude-plugin/` etc.) | ❌ none (aspirational mention) | `adapters/README.md` |
| Release: tagged static binary | ❌ none | profile is size-optimized only |

---

## Part B — Target conventions per sub-area

### B1. Binary version (`--version`)

- Standard Rust idiom: print `env!("CARGO_PKG_VERSION")` (compile-time, from Cargo.toml) for `--version`
  / `-V`. Cheapest possible — no new dep, one match arm before `print_help`. This is the single most
  load-bearing prerequisite for everything else (releases, plugin manifest version, debugging, `doctor`).
- **Decision space:** should `--version` also print the supported rules `SCHEMA_VERSION` (and instinct
  schema)? A one-line `gatekeeper 0.1.0 (rules schema v1)` is honest and nearly free.
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

### B5. Other harnesses' "packaging"

Codex/Cursor/OpenCode have **no Claude-style plugin marketplace**. Their distribution *is* the existing
`adapt`-generated native config (`.codex/config.toml`, `.cursor/rules/*.mdc`, `opencode.json`) plus an
install doc. So "per-harness installer flows" realistically means: **one real plugin for Claude Code** +
**`adapt` + documented install** for the other three. Don't invent fake plugin systems for harnesses that
lack them ([[weakest-enforcement-that-works]]).

### B6. `doctor` and docs-coverage lint (the soft/optional deliverables)

- **`gatekeeper doctor`** (roadmap marks optional): a health check — binary version, rules.toml present +
  schema_version supported, `instincts/`/`skills/` parse, hooks executable, git hooks installed. It's
  pure read-only introspection over machinery that already exists; a thin aggregator. Genuinely optional.
- **Docs-coverage lint** is the *undefined* half of "link/coverage lint." Candidate meaning: every skill
  has a `SKILL.md` with valid frontmatter, every ADR is linked from the index, every roadmap phase marked
  ✅ has a `docs/verify/` note, no orphan operators. This could be a new `gatekeeper` subcommand or a
  shell/CI check. Risk of scope creep — needs an explicit decision on whether it's in Phase 6 at all.

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

---

## Part D — Open questions / forks for the design gate

1. **Binary-in-plugin model** (Part C) — system-PATH (1), bundled prebuilt matrix (2), or build-on-install
   (3)? Drives the release matrix and CI weight. *(Strongest candidate for an explicit user decision.)*
2. **Scope of "plugin manifests, equivalents"** — one real Claude Code plugin + marketplace, and treat
   Codex/Cursor/OpenCode packaging as the existing `adapt` + docs? Or attempt more per harness?
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
   Entangled with #1.

---

## Part E — Recommended scope (weakest-enforcement-that-works, surgical)

Grounded in Part A (much of the gate already exists) and the project's instincts, a tight Phase 6 that
delivers the roadmap's intent without over-building:

1. **`--version`/`-V`** printing `CARGO_PKG_VERSION` (+ rules schema version) — the keystone. Trivial,
   unblocks everything. (`main.rs`, one arm + test.)
2. **GitHub Actions CI** that installs `just` + the four CLI tools and runs `just ci` (or `just check` as
   the blocking gate + a separate `links`/`deny` job) on push/PR — mirroring the existing local gate, no
   re-design. Plus a **tag → release** workflow building the size-optimized binary and attaching it to a
   GitHub Release.
3. **A real Claude Code plugin**: `.claude-plugin/plugin.json` + `marketplace.json`, reusing the existing
   root-level `skills/` and `hooks/` layout, with hooks invoking the binary via `${CLAUDE_PLUGIN_ROOT}`
   (resolving Part C — recommend model **(1) system-PATH** or **(1)+(2) hybrid for tagged releases**,
   pending the user's call). Other harnesses: keep `adapt` + a documented install flow (B5).
4. **A small `doctor`** *(if approved)* — pure read-only aggregation over existing machinery.
5. **ADR-0010** capturing the binary-in-plugin decision + the binary/schema-version policy.
6. Defer or minimize **docs-coverage lint** unless the user wants a concrete check — the link check
   already satisfies "docs lint."

> Net: Phase 6 is more *wiring and packaging* than new engine code. The justfile/lychee gate already
> exists (don't rebuild it — run it in CI); the binary just needs to *announce its version* and *ship*;
> the plugin layout already matches our tree. The one genuinely hard decision is **how the compiled
> binary travels inside a plugin** (Part C) — resolve that first in design.
