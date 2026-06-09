# 0010 — Packaging & distribution: system-PATH binary, CI mirrors the justfile, hand-authored plugin

- **Status:** Accepted
- **Date:** 2026-06-09

Phase 6 packages Topology for distribution (ROADMAP
[Phase 6](../ROADMAP.md#phase-6--packaging--distribution)): per-harness install, plugin manifests,
version pinning, a health check, and CI that runs the quality gate on a fresh clone. This ADR records the
cross-cutting decisions, grounded by the
[research](../research/2026-06-09-packaging-distribution.md) — whose headline finding is that the *gate
itself already exists* (`justfile` + `lychee.toml`) but never runs, and that our `skills/`+`hooks/` tree
already matches the Claude Code plugin layout. So Phase 6 is mostly **wiring and packaging**, not new
engine code.

## Decisions

1. **The compiled binary travels via the system PATH, not bundled in the plugin.** A Claude Code plugin
   expects a *prebuilt* executable in `bin/`, but `gatekeeper` is compiled per-machine by `install.sh`
   (`cargo build --release`). Rather than cross-compile a per-platform matrix on every release and host
   binaries, **the plugin ships hooks + skills + commands only**, and its hooks invoke `gatekeeper`
   resolved on `PATH` (overridable via a `GATEKEEPER_BIN` env var), the same binary `install.sh` builds.
   The bundled hook **scripts** are still referenced portably with `${CLAUDE_PLUGIN_ROOT}` (the path
   variable Claude Code sets at load); only the *binary* is external. This is the weakest mechanism that
   delivers the behaviour (instinct: [[weakest-enforcement-that-works]]): no cross-compile, the plugin is
   pure data + glue, and a missing binary degrades with a clear message instead of a broken hook.
   *Residual, stated plainly:* the binary is **not** self-contained in the plugin — the user installs it
   separately (`install.sh`/`cargo install`), so "one-command install" is "add marketplace + install
   plugin **and** build the binary," not a single step. Prebuilt binaries (decision 4) are a convenience
   layer that softens this, and bundling them in `bin/` later is a reversible upgrade, not a rewrite.

2. **Binary version and rules-schema version are independent; `--version` surfaces both.**
   `gatekeeper --version`/`-V` prints `gatekeeper <CARGO_PKG_VERSION> (rules schema v<SCHEMA_VERSION>)` —
   the tool version from `env!("CARGO_PKG_VERSION")` (compile-time, no new dependency) and the rules data
   format version from the existing `scan::SCHEMA_VERSION` const. They are **decoupled on purpose**
   (research A2): the schema version tracks `rules.toml`/frontmatter *data* compatibility (a `0.2.0` binary
   may still accept `schema_version = 1`), while the binary version tracks the *tool*. The plugin
   manifest's `version` mirrors the Cargo version, and CI **asserts** a release tag `vX.Y.Z` matches
   `Cargo.toml`'s `version` (the "forgot to bump" footgun the plugin docs warn about — pushing commits
   without bumping the manifest version silently fails to update installed plugins).

3. **CI mirrors the justfile — it does not re-define the gate.** The quality gate already lives in
   `justfile` (`check` = `fmt-check lint test shell typos`; `ci` = `check deny links`) and `lychee.toml`.
   GitHub Actions installs `just` + the four external tools (`shellcheck`, `typos`, `lychee`,
   `cargo-deny`) and runs the **same recipes** the human runs, so there is one source of truth and no
   local-vs-CI drift (instinct: [[constraints-as-reasoning]] — the recipe is the contract). The blocking
   PR gate is the offline `just check`; the network gates (`deny`, `links`) run in a **separate,
   non-blocking** job so a transient link 5xx or advisory-DB fetch can't wedge a merge — lychee's
   retry/accept config already softens this, and `shfmt` stays advisory (the scripts are hand-aligned),
   exactly as the justfile documents.

4. **Release artifacts: macOS arm64 only, on a version tag.** A `push: tags: v*` workflow builds the
   already-size-optimized release binary for `aarch64-apple-darwin` (the dev/primary platform) and
   attaches it to a GitHub Release. The prebuilt binary is a **convenience**, not the primary install path
   (decision 1 makes build-from-source the floor), so a single target is the right scope — adding
   `x86_64-unknown-linux-gnu` or a broader matrix later is a reversible change to one workflow, recorded
   here as a deliberate scope choice, not an omission (instinct: [[surgical-changes-only]]).

5. **Plugin and marketplace manifests are hand-authored committed source — not `adapt`-generated.**
   [ADR-0003](0003-one-markdown-source-per-harness-adapters.md) makes `.claude/settings.json` a *generated*
   artifact because it embeds machine-local absolute hook paths. That rationale **does not apply** to
   `.claude-plugin/plugin.json` / `marketplace.json`: they carry no machine-specific paths (hooks resolve
   via `${CLAUDE_PLUGIN_ROOT}` and the binary via PATH), they are stable metadata, and Claude Code has a
   first-class validator (`claude plugin validate`). So they are normal committed source, validated in CI,
   not another generator in `adapt.rs`. **The plugin coexists with `adapt`; it does not replace it:** the
   plugin is the packaged distribution *for Claude Code*, while `adapt --harness {codex|cursor|opencode}`
   remains the distribution path for the three harnesses that have no plugin/marketplace system (research
   B5), and `adapt --harness claude` + `install.sh` remain the local-dev wiring.

6. **`doctor`, docs-coverage lint, and crates.io metadata are all in scope this phase.**
   - **`gatekeeper doctor`** — a read-only health check (binary version; `security/rules.toml` present and
     its `schema_version` supported; `instincts/` and `skills/` parse via their existing loaders; hooks
     present/executable; git pre-commit hook installed). Pure introspection over machinery that already
     exists; reuses existing parsers, no new dependency.
   - **Docs-coverage lint** — a new `gatekeeper check docs` arm (no `--feature`) enforcing repo-doc
     invariants the link-checker can't: every `skills/*/SKILL.md` has valid frontmatter, every ROADMAP
     phase marked ✅ has a matching `docs/verify/*.md`, and every `docs/adr/00NN-*.md` is linked from
     `docs/adr/README.md`. This is the "coverage" half of "link/coverage lint" (lychee is the "link"
     half); it lives as a `check` arm (deterministic, testable via the existing CLI harness, instinct:
     [[gates-not-rules]]) and is wired into the justfile/CI.
   - **crates.io metadata** — add `authors`, `repository`, `homepage`, `keywords` (and keep
     `description`/`license`) to `Cargo.toml`. Not because we publish a library (Topology is a tool, not a
     crate dependency), but the metadata is cheap, aids any future `cargo install --git` / release
     provenance, and was explicitly requested.

## Consequences

- **No new runtime dependency.** `--version` uses the built-in `CARGO_PKG_VERSION`; `doctor` and
  `check docs` reuse the existing scan/instinct/skill parsers and `std`. ADR-0007's no-new-deps posture
  holds; `Cargo.lock` runtime graph is unchanged (CI tool installs are not crate deps).
- **Three-language lanes preserved** ([[three-language-lanes]]): the enforcers (`--version`, `doctor`,
  `check docs`) are Rust in `gatekeeper/src/`; the CI workflows + plugin hook wiring are YAML/JSON +
  Bash glue; the contracts (plugin README, ADR, install docs) are Markdown. No lane crossings.
- **Verifiability caveat, recorded honestly** ([[evidence-over-assertion]]): "CI is green on a fresh
  clone" and "`claude plugin validate` passes" depend on GitHub Actions and the `claude` CLI, which may
  not run in every local verify environment. The verify note will (a) prove the workflow YAML parses and
  invokes the real recipes, (b) run the same `just ci` locally to show the gate it mirrors is green, and
  (c) run `claude plugin validate` if the CLI is present, else assert the manifest's required fields and
  JSON validity directly. What CI proves end-to-end (a green run on push) is linked from the verify note
  once the workflow has run, not asserted from a dry tree.
- **Plugin updates require a version bump.** Because we set an explicit `version` (decision 2), every
  user-facing change must bump `plugin.json`'s `version` (and `Cargo.toml`'s, kept in lockstep) or
  installed plugins won't update — enforced by the tag/version-match CI guard.
- ROADMAP Phase 6 moves to ✅ delivered with a link to the verify note; the canonical gate sequence
  (research → design → plan → tdd → verify → review) is unchanged — Phase 6 adds the *unfeatured*
  `check docs` arm and `doctor`, not a new sequence stage.
- This is the **terminal roadmap phase**; after Phase 6 the system is "fully built" per the roadmap, and
  further work (semantic recall, more release targets, bundled binaries, more harness plugins) is
  explicitly future-ADR territory, reversible from here.
