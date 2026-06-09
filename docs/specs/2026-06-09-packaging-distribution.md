# Design: Packaging & distribution (Phase 6)

- **Date:** 2026-06-09
- **Feature slug:** packaging-distribution
- **Status:** approved (authorized as the Phase 6 build; grounded by
  [research](../research/2026-06-09-packaging-distribution.md) and
  [ADR-0010](../adr/0010-packaging-distribution.md))
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
  version is `env!("CARGO_PKG_VERSION")` (compile-time, no dep) and `<N>` is `scan::SCHEMA_VERSION`. Exit
  `0`. The version is also embedded in `print_help()`'s header (`main.rs:75-99`).

### 2. `gatekeeper doctor` (Rust)

- A new `doctor` arm (registered alongside `check`/`scan`/… in the dispatch) running a **read-only**
  health check and printing one line per probe with a final summary; exit `0` if all pass, non-zero if any
  fail. Probes (all over machinery that already exists):
  - binary version (always passes — informational);
  - `security/rules.toml` present and its `schema_version` equals `scan::SCHEMA_VERSION` (reuse the scan
    loader);
  - `instincts/` parse via the instinct loader; `skills/*/SKILL.md` parse via the skill loader;
  - hook scripts in `hooks/` exist and are executable;
  - the git `pre-commit` hook is installed (`.git/hooks/pre-commit` present).
- Writes nothing; framework root resolved the same way the other commands do.

### 3. `gatekeeper check docs` — docs-coverage lint (Rust)

- A new arm in `cmd_check` (no `--feature`) enforcing repo-doc invariants lychee can't (ADR-0010 §6):
  - every `skills/*/SKILL.md` has valid frontmatter (reuse the skill loader — fail lists the offenders);
  - every ROADMAP phase row/section marked ✅ has a matching `docs/verify/*.md` note;
  - every `docs/adr/00NN-*.md` is linked from `docs/adr/README.md`.
- Exit `0` when the tree is clean (it must be, on this branch), `1` with the specific gaps otherwise.
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
skills/                # already at root, discovered as-is
```

- **`.claude-plugin/plugin.json`** — required `name: topology` (kebab-case) + semver `version` matching
  `Cargo.toml`; `description`, `author.name`, `repository`, `license`, `homepage`. The `hooks` field points
  at `hooks/hooks.json`; `skills` resolve from the existing root `skills/`.
- **`hooks/hooks.json`** — declares the `UserPromptSubmit` + `PreToolUse` handlers, each a
  `"${CLAUDE_PLUGIN_ROOT}/hooks/<script>.sh"` command. The scripts call `gatekeeper` via `PATH` (or
  `$GATEKEEPER_BIN`); if the binary is missing they emit a clear "gatekeeper not installed — run
  install.sh" message and fail open/closed as the existing hook does (ADR-0010 §1).
- **`.claude-plugin/marketplace.json`** — `name`, `owner{name,email}`, and a `plugins[]` entry for
  `topology` with `source` referencing this repo (relative `.` for in-repo, or the github source). The
  documented user flow: `/plugin marketplace add osxsystem/topology` → `/plugin install topology@<market>`.
- **`scripts/install.sh`** gains a documented note: the binary install (build-from-source today) is a
  prerequisite for the plugin; `adapt --harness {codex|cursor|opencode}` remains the path for the other
  harnesses (ADR-0010 §5). The plugin **coexists** with `adapt`; it does not replace per-machine
  `.claude/settings.json` wiring for non-plugin installs.

### 6. Packaging metadata + docs (Markdown/TOML)

- `Cargo.toml` gains `authors`, `repository`, `homepage`, `keywords` (keep `description`/`license`).
- ROADMAP Phase 6 → ✅ delivered with a verify-note link; a plugin `README.md` (or section) documents the
  install/version/doctor surface; ADR-0010 is the decision record.

## Acceptance criteria (checked in the verify note)

1. **Version surface.** `gatekeeper --version` and `gatekeeper -V` both print
   `gatekeeper <X.Y.Z> (rules schema v<N>)` and exit `0`; `<X.Y.Z>` equals `Cargo.toml`'s `version` and
   `<N>` equals `scan::SCHEMA_VERSION`. `print_help()` shows the version. (CLI test.)
2. **`doctor` is read-only and accurate.** On a healthy tree `gatekeeper doctor` exits `0` and reports
   each probe; with a seeded fault (e.g. a `rules.toml` whose `schema_version` mismatches, or a
   non-executable hook) it exits non-zero naming the failing probe; it writes nothing (tree unchanged
   after the run). (CLI test in a scratch root.)
3. **Docs-coverage lint passes clean and catches gaps.** `gatekeeper check docs` exits `0` on this branch;
   in a scratch root it exits `1` and names the gap when a `SKILL.md` has broken frontmatter, a ✅ roadmap
   phase lacks a verify note, or an ADR is unlinked from the ADR README. Missing/extra args behave like
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
8. **Plugin hooks resolve the binary via PATH (system-PATH model).** The bundled hook scripts invoke
   `gatekeeper` from `PATH` / `$GATEKEEPER_BIN` (not a bundled `bin/`), and when the binary is absent emit a
   clear "not installed" message rather than a silent failure — proving ADR-0010 §1's external-binary
   contract and its residual. (Script inspection + a no-binary invocation test.)
9. **No new runtime dependency; suite stays green.** The `[dependencies]` graph in `Cargo.toml`/`Cargo.lock`
   is unchanged (CI tool installs are not crate deps; ADR-0010 consequences); `--version`/`doctor`/`check
   docs` use `CARGO_PKG_VERSION` + existing parsers + `std`. Full `cargo test`, `cargo clippy -- -D
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
- **Plugins/marketplaces for Codex/Cursor/OpenCode** — those harnesses have no plugin system; their
  distribution stays the existing `adapt` outputs + install docs (research B5).
- **Generating `plugin.json`/`marketplace.json` from `adapt`** — they are hand-authored committed source
  (ADR-0010 §5), unlike the path-bearing `.claude/settings.json`.
- **Publishing to crates.io** — metadata is added for provenance/`cargo install --git`, but no publish
  step; Topology is a tool, not a library dependency.
- **A `gatekeeper version` subcommand in addition to `--version`** — one spelling (`--version`/`-V`) is
  enough ([[surgical-changes-only]]).
- **Replacing the justfile gate with bespoke CI steps** — CI invokes the existing recipes; the justfile
  stays the single source of truth (ADR-0010 §3).
