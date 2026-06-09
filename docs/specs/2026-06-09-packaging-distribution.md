# Design: Packaging & distribution (Phase 6)

- **Date:** 2026-06-09
- **Feature slug:** packaging-distribution
- **Status:** 🟢 **Approved 2026-06-09** — re-confirmed after two Codex design-gate review passes, with the
  binary-in-plugin fork resolved to **model (1): system-PATH** (Part C / research D#1). Revised to match the corrected
  [research](../research/2026-06-09-packaging-distribution.md) and
  [ADR-0010](../adr/0010-packaging-distribution.md). What changed: the plugin hooks' **fail policy is split**
  (PreToolUse `scan` fail-closed, UserPromptSubmit `activate` fail-open); `$GATEKEEPER_BIN` is a **new**
  addition; the version seam serves **`--version` + `doctor`** (not `check docs`); Codex/OpenCode **have**
  plugin systems (other-harness packaging is a **scope cut**); and the docs-coverage lint now carries a
  **three-file reconciliation precondition** (ADR index, `ARCHITECTURE.md`, README). Search `[rev:codex]`.
- **Roadmap:** [Phase 6](../ROADMAP.md#phase-6--packaging--distribution)

## Goal

Make Topology **installable, versioned, and CI-guarded** without re-engineering anything. The research
found the quality gate already exists (`justfile` + `lychee.toml`) and the repo tree already matches the
Claude Code plugin layout, so this phase **wires** what exists: teach the binary to announce its version,
run the existing gate in CI on a fresh clone, ship a real Claude Code plugin (binary resolved on PATH per
ADR-0010 §1), add a `doctor` health check and a docs-coverage lint, and record packaging metadata. No new
runtime dependency (ADR-0010 consequences).

## Shape

Five buckets. The three Rust additions are built from helpers already in `main.rs`/`scan.rs`/`instinct.rs`;
the CI and plugin are YAML/JSON + Bash glue + Markdown contracts ([[three-language-lanes]]).

### 1. Version surface (Rust)

- **`gatekeeper --version` / `-V`** — a new match arm in the top-level dispatch (`main.rs:51-73`, before
  the `--help|-h|None` arm) printing `gatekeeper <CARGO_PKG_VERSION> (rules schema v<N>)`, where the tool
  version is `env!("CARGO_PKG_VERSION")` (compile-time, no dep) and `<N>` is the rules schema version. Exit
  `0`. The version is also embedded in `print_help()`'s header (`main.rs:75-99`).
- **Version seam (`[rev:codex-2026-06-09]`, ADR-0010 §2).** `scan::SCHEMA_VERSION` is private; introduce a
  small internal `version` module (e.g. `version::tool()` → `env!("CARGO_PKG_VERSION")`,
  `version::rules_schema()` re-exporting `scan`'s value) so the **two** consumers — `--version` and
  `doctor` — depend on the seam, not the private const or a re-declared `1`. The docs-coverage lint is
  **not** a consumer (it reads no schema version). Visibility refactor only, no new dependency.

### 2. `gatekeeper doctor` (Rust)

- A new `doctor` arm (registered alongside `check`/`scan`/… in the dispatch) running a **read-only**
  health check and printing one line per probe with a final summary; exit `0` if all pass, non-zero if any
  fail. Probes (all over machinery that already exists):
  - **resolution transparency (`[rev:codex-2026-06-09]`)** — the binary's own path (`current_exe`) and
    version (via the §1 seam), plus whether a `gatekeeper` is discoverable for hook use and *how*
    (`$GATEKEEPER_BIN` if set, else `PATH`, else "not on PATH — hooks rely on the repo build"). This is the
    one place that surfaces *which* binary wins and its version (ADR-0010 §1; the hooks stay silent on the
    happy path, so `doctor` carries this). Informational unless `$GATEKEEPER_BIN` points at a
    missing/non-executable path, which fails;
  - `security/rules.toml` present and its `schema_version` equals the rules-schema version (via the seam;
    reuse the scan loader);
  - `instincts/` parse via the instinct loader; `skills/*/SKILL.md` parse via the skill loader;
  - hook scripts in `hooks/` exist and are executable;
  - the git `pre-commit` hook is installed (`.git/hooks/pre-commit` present).
- Writes nothing; framework root resolved the same way the other commands do.

### 3. `gatekeeper check docs` — docs-coverage lint (Rust)

- A new arm in `cmd_check` (no `--feature`) enforcing repo-doc invariants lychee can't (ADR-0010 §6):
  - every `skills/*/SKILL.md` has valid frontmatter (reuse the skill loader — fail lists the offenders);
  - every `docs/verify/*.md` note *referenced from* `docs/ROADMAP.md` resolves to a file on disk (a phase
    that cites none — e.g. Phase 0, the no-code Blueprint — is exempt);
  - every `docs/adr/00NN-*.md` is linked from `docs/adr/README.md`.
- **Reconciliation precondition (`[rev:codex-2026-06-09]`, ADR-0010 §6).** The tree does **not** satisfy
  these invariants today, so the three-file reconciliation in §6 must land **first**, or the lint fails on
  its own repo: the ADR index stops at 0007 (0008–0010 unlinked), and the lint can only exit `0` on this
  branch *after* that fix. (README's phantom `.claude-plugin/` is resolved by §5 creating the manifest;
  `ARCHITECTURE.md`'s `[planned]` tags + "static binary" are corrected in §6.)
- Exit `0` when the tree is clean (true **after** reconciliation), `1` with the specific gaps otherwise.
- Wired into the `justfile` (a `docs` recipe folded into `check`) and CI (bucket 4).

### 4. CI + release (GitHub Actions, YAML)

- **`.github/workflows/ci.yml`** — on `push` + `pull_request`: checkout, install Rust (pinned toolchain
  with `rustfmt`+`clippy`), cache (`Swatinem/rust-cache`), install `just` + `shellcheck` + `typos` +
  `cargo-deny` + `lychee`, then run **`just check`** (the blocking offline gate) and **`gatekeeper check
  docs`**. A **separate, non-blocking** job runs the network gates (`just deny`, `just links`) so flake
  can't wedge a merge (ADR-0010 §3). The workflow invokes the *existing recipes*, not re-spelled commands.
- **`.github/workflows/release.yml`** — on `push: tags: v*`: a **version-match guard** asserting the tag
  `vX.Y.Z` equals `Cargo.toml`'s `version` (and `plugin.json`'s) — fail the release if they diverge
  (ADR-0010 §2); then `cargo build --release` for `aarch64-apple-darwin` (macOS runner) and attach the
  stripped binary to a GitHub Release (`softprops/action-gh-release` or `gh release`).

### 5. Claude Code plugin + marketplace (JSON/Markdown + Bash glue)

```
.claude-plugin/
  plugin.json          # manifest: name, version (== Cargo), description, author, repository, license
  marketplace.json     # lists the topology plugin; source = this repo
hooks/
  hooks.json           # wires the existing hook scripts via ${CLAUDE_PLUGIN_ROOT}
  *.sh                 # existing scripts — resolve `gatekeeper` on PATH (GATEKEEPER_BIN override)
  skill-rules.json     # DATA: the `activate` hook's routing table
security/rules.toml    # DATA: the `scan` hook's rules — REQUIRED (fail-closed → absent denies every tool call)
instincts/             # DATA: always-on instinct bodies, read by `activate` + `doctor`
skills/                # routed content AND the anchor framework_root() keys on (main.rs:102)
```

- **The plugin bundles everything the binary reads, not just `skills/`.** `gatekeeper` resolves its data
  relative to a *framework root* (`framework_root()`, `main.rs:102`: walk up from CWD to the nearest dir
  with `skills/`); the hooks `cd "$ROOT"` so that root is `${CLAUDE_PLUGIN_ROOT}`. So `security/rules.toml`,
  `hooks/skill-rules.json`, and `instincts/` must ship at the plugin root alongside `skills/` — omit
  `rules.toml` and the fail-closed `scan` hook denies every tool call. All are committed data (no generation).
- **`.claude-plugin/plugin.json`** — required `name: topology` (kebab-case) + semver `version` matching
  `Cargo.toml`; `description`, `author.name`, `repository`, `license`, `homepage`. The `hooks` field points
  at `hooks/hooks.json`; `skills` resolve from the existing root `skills/`.
- **`hooks/hooks.json`** — declares the `UserPromptSubmit` + `PreToolUse` handlers, each a
  `"${CLAUDE_PLUGIN_ROOT}/hooks/<script>.sh"` command. The scripts call `gatekeeper` via `$GATEKEEPER_BIN`
  (a **new** override — neither hook reads it today) → `PATH`. **Fail policy is split by hook type and
  must be preserved (`[rev:codex-2026-06-09]`, ADR-0010 §1):** a missing binary makes the **PreToolUse
  `scan` hook fail-*closed*** (emit `deny` — `security-scan.sh:4,25,32`; a security veto that fails open is
  worse than none) and the **UserPromptSubmit `activate` hook fail-*open*** (advisory line + `exit 0` —
  `skill-activation.sh:23,27`; routing is advisory). Both messages name the fix (`run scripts/install.sh`).
  The hooks stay **silent on the happy path** (`security-scan.sh:28-30`); resolution transparency is
  `doctor`'s job (§2), not the hooks'.
- **`.claude-plugin/marketplace.json`** — `name`, `owner{name,email}`, and a `plugins[]` entry for
  `topology` with `source` referencing this repo (relative `.` for in-repo, or the github source). The
  documented user flow: `/plugin marketplace add osxsystem/topology` → `/plugin install topology@<market>`.
- **`scripts/install.sh`** gains a documented note: the binary install (build-from-source today) is a
  prerequisite for the plugin; `adapt --harness {codex|cursor|opencode}` remains the path for the other
  harnesses (ADR-0010 §5). The plugin **coexists** with `adapt`; it does not replace per-machine
  `.claude/settings.json` wiring for non-plugin installs.

### 6. Packaging metadata + docs reconciliation (Markdown/TOML)

- `Cargo.toml` gains `authors`, `repository`, `homepage`, `keywords` (keep `description`/`license`).
- **Docs reconciliation — a precondition for §3's lint (`[rev:codex-2026-06-09]`, ADR-0010 §6).** Three
  files drift from delivered reality and must be reconciled *before* `check docs` is enabled:
  - `docs/adr/README.md` — extend the index to add **0008, 0009, 0010** (R2 of the lint fails otherwise);
  - `docs/ARCHITECTURE.md` — flip shipped modules `[planned]`→`[built]` (`scan`, `instinct`, `learn`,
    `adapt`, `check research`, the adapters, `security-scan.sh`, `pre-commit.sh`) and replace "static
    binary" with "single std-only macOS-arm64 executable" (it dynamically links `libSystem` — not musl);
  - `README.md` — the phantom `.claude-plugin/plugin.json` (`README.md:28-29,78`) is resolved by §5
    *creating* the manifest (code catches up to doc); fix its "static binary" wording (`:82`) too.
- ROADMAP Phase 6 → ✅ delivered with a verify-note link (and "static binary" corrected there if present);
  a plugin `README.md` (or section) documents the install/version/doctor surface; ADR-0010 is the decision
  record.

## Acceptance criteria (checked in the verify note)

1. **Version surface.** `gatekeeper --version` and `gatekeeper -V` both print
   `gatekeeper <X.Y.Z> (rules schema v<N>)` and exit `0`; `<X.Y.Z>` equals `Cargo.toml`'s `version` and
   `<N>` equals `scan::SCHEMA_VERSION`. `print_help()` shows the version. (CLI test.)
2. **`doctor` is read-only and accurate, and surfaces binary resolution.** On a healthy tree `gatekeeper
   doctor` exits `0` and reports each probe, **including the resolved binary path + version + resolution
   mechanism** (`$GATEKEEPER_BIN`/`PATH`/repo); with a seeded fault (e.g. a `rules.toml` whose
   `schema_version` mismatches, a non-executable hook, or `$GATEKEEPER_BIN` pointing at a missing path) it
   exits non-zero naming the failing probe; it writes nothing (tree unchanged after the run). (CLI test in
   a scratch root, including a `GATEKEEPER_BIN`-set invocation.)
3. **Docs-coverage lint passes clean and catches gaps.** `gatekeeper check docs` exits `0` on this branch;
   in a scratch root it exits `1` and names the gap when a `SKILL.md` has broken frontmatter, a
   `docs/verify/…md` pointer in `docs/ROADMAP.md` has no file on disk, or an ADR is unlinked from the ADR README. Missing/extra args behave like
   the other `check` arms. (CLI test.)
4. **CI workflow runs the existing gate.** `.github/workflows/ci.yml` exists, is valid YAML, triggers on
   `push`+`pull_request`, installs `just` + the four tools, and invokes **`just check`** + `gatekeeper
   check docs` (the same recipes the human runs — asserted by grepping the workflow for the recipe names,
   not re-spelled commands). The network gates run in a separate non-blocking job. The gate it mirrors is
   shown green locally via `just ci`. (Static assertion + local `just ci`; a green CI run is linked once
   the workflow has executed on the remote.)
5. **Release workflow guards the version and builds the artifact.** `.github/workflows/release.yml` exists,
   is valid YAML, triggers on `push: tags: v*`, contains a step asserting the tag equals `Cargo.toml`'s
   `version` (failing on mismatch), and builds + attaches an `aarch64-apple-darwin` release binary.
   (Static assertion of the steps; the version-match logic is unit-demonstrated against a matching and a
   mismatching value.)
6. **Plugin manifest validates.** `.claude-plugin/plugin.json` is valid JSON with required `name` and a
   semver `version` equal to `Cargo.toml`'s; `hooks` points at `hooks/hooks.json`, which is valid JSON
   wiring the existing scripts via `${CLAUDE_PLUGIN_ROOT}`. `claude plugin validate .` passes when the
   `claude` CLI is available; otherwise the required-fields + JSON-validity + version-match are asserted
   directly. (ADR-0010 verifiability caveat.)
7. **Marketplace manifest is installable.** `.claude-plugin/marketplace.json` is valid JSON with `name`,
   `owner`, and a `plugins[]` entry for `topology` whose `source` references this repo; the install flow
   is documented (`/plugin marketplace add osxsystem/topology` → `/plugin install`). (JSON-validity +
   field assertion; doc present.)
8. **Plugin hooks resolve the binary via PATH (system-PATH model) with the correct split fail policy.**
   The bundled hook scripts invoke `gatekeeper` from `$GATEKEEPER_BIN` → `PATH` (not a bundled `bin/`).
   When the binary is absent: the **PreToolUse `scan` hook fails closed** — emits a `deny` JSON decision
   (not a silent allow); the **UserPromptSubmit `activate` hook fails open** — prints an advisory line and
   exits `0`. Both name the fix. On the happy path the scanner is silent. This proves ADR-0010 §1's
   external-binary contract, its split, and its residual. (Script inspection + two no-binary invocation
   tests, one per hook, asserting deny-vs-advisory.)
9. **No new runtime dependency; suite stays green.** The `[dependencies]` graph in `Cargo.toml`/`Cargo.lock`
   is unchanged (CI tool installs are not crate deps; ADR-0010 consequences); `--version` and `doctor`
   read the **version seam** (`CARGO_PKG_VERSION` + `scan`'s schema), `doctor` and `check docs` reuse the
   existing scan/instinct/skill parsers, all on `std` only. Full `cargo test`, `cargo clippy -- -D
   warnings`, and `cargo fmt --check` are clean.
10. **Packaging metadata present.** `Cargo.toml` carries `authors`, `repository`, `homepage`, `keywords`,
    `description`, `license`; `cargo build --release` and `cargo package --locked` (or `--list`) succeed.
11. **Install + coexistence documented.** `scripts/install.sh` documents the binary prerequisite and the
    plugin path; `adapt --harness {codex|cursor|opencode|claude}` still works (a `--check` run is clean),
    proving the plugin coexists with the adapters rather than replacing them.
12. **Version bump discipline is recorded.** The verify note records that setting an explicit `version`
    means every release must bump `plugin.json` + `Cargo.toml` in lockstep (or installed plugins won't
    update), enforced by the release workflow's version-match guard — an explicit, reviewed consequence,
    not an unstated gap (ADR-0010 §2).

## Non-goals (this phase)

- **Bundled prebuilt binary inside the plugin** — the binary is resolved on PATH (ADR-0010 §1); shipping
  per-platform binaries in the plugin's `bin/` is a reversible future upgrade.
- **A cross-compile release matrix** — macOS arm64 only (ADR-0010 §4); other targets are a one-workflow
  change later.
- **Plugins/marketplaces for Codex/Cursor/OpenCode** — a deliberate **scope cut, not a capability gap
  (`[rev:codex-2026-06-09]`):** Codex CLI (`codex plugin marketplace`, verified 0.137.0) and OpenCode
  (`opencode plugin`, verified 1.14.22) *do* have plugin systems now; only Cursor has none. We package one
  plugin (Claude Code) this terminal phase; their distribution stays the existing `adapt` outputs +
  install docs, and native Codex/OpenCode plugins are an explicitly deferred future extension (research B5).
- **Generating `plugin.json`/`marketplace.json` from `adapt`** — they are hand-authored committed source
  (ADR-0010 §5), unlike the path-bearing `.claude/settings.json`.
- **Publishing to crates.io** — metadata is added for provenance/`cargo install --git`, but no publish
  step; Topology is a tool, not a library dependency.
- **A `gatekeeper version` subcommand in addition to `--version`** — one spelling (`--version`/`-V`) is
  enough ([[surgical-changes-only]]).
- **Replacing the justfile gate with bespoke CI steps** — CI invokes the existing recipes; the justfile
  stays the single source of truth (ADR-0010 §3).
