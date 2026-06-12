# 0010 — Packaging & distribution: system-PATH binary, CI mirrors the justfile, hand-authored plugin

- **Status:** 🟡 Accepted (re-confirmed 2026-06-09), amended in part by
  [ADR-0015](0015-plugin-channel-retirement.md) (2026-06-12): the hand-authored
  plugin/marketplace manifests (§5) are retired with the plugin channel; the system-PATH
  binary model and CI-mirrors-justfile decisions stand. — *re-opened to Proposed across two
  Codex design-gate review passes, revised to match the corrected
  [research](../research/2026-06-09-packaging-distribution.md), and re-confirmed on sign-off
  with the binary-in-plugin fork resolved to model (1) system-PATH.*
- **Date:** 2026-06-09

> **Revision note (`[rev:codex-2026-06-09]`).** This ADR was first authored from the pre-review research and
> Accepted; a second review pass found six factual deltas, now folded in below: (1) the external-binary
> "degrades with a clear message" gloss hid a **fail-policy split** — the PreToolUse `scan` hook is
> fail-*closed* (emits `deny`), the UserPromptSubmit `activate` hook is fail-*open*; (2) `$GATEKEEPER_BIN`
> is a **new** addition (neither hook reads it today); (3) resolution transparency is **`doctor`'s** job,
> not every hook's; (4) the version seam serves **`--version` + `doctor`**, not `check docs`; (5) Codex
> CLI and OpenCode **have** plugin systems now, so the other-harness story is a **scope cut**, not a
> capability gap; (6) the docs-coverage lint has a **reconciliation precondition** (the ADR index, the
> `ARCHITECTURE.md` `[planned]` tags, and README's phantom `.claude-plugin/` must be fixed first).

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
   binaries, **the plugin ships everything *except* the binary** — hooks, skills, commands, and the data those read at runtime — and its hooks invoke `gatekeeper`
   resolved on `PATH` (and, **newly this phase**, an optional `$GATEKEEPER_BIN` override — *neither hook
   reads it today* `[rev:codex-2026-06-09]`, so design adds it), the same binary `install.sh` builds.
   The bundled hook **scripts** are still referenced portably with `${CLAUDE_PLUGIN_ROOT}` (the path
   variable Claude Code sets at load); only the *binary* is external. **The binary resolves its data
   relative to a *framework root* — `framework_root()` (`main.rs:102`) walks up from CWD to the nearest
   directory containing `skills/`, and the hooks `cd "$ROOT"` so that root is `${CLAUDE_PLUGIN_ROOT}`. The
   plugin must therefore bundle the data those reads consume: `security/rules.toml` (the fail-*closed*
   `scan` hook — absent, it denies *every* tool call), `hooks/skill-rules.json` and `instincts/` (the
   fail-open `activate` hook and `doctor`), and `skills/` (both the routed content *and* the anchor
   `framework_root()` keys on). All are committed data, so "bundle" means "include", not "generate".** This is the weakest mechanism that
   delivers the behaviour (instinct: [[weakest-enforcement-that-works]]): no cross-compile, the plugin is
   pure data + glue, and a missing binary degrades **per the existing hook contract**, not silently.
   *The fail policy is split by hook type and must be preserved (`[rev:codex-2026-06-09]`):* the
   **PreToolUse `scan` hook is fail-*closed*** — a missing/erroring binary emits a `deny`
   (`hooks/security-scan.sh:4,25,32`), because a security veto that fails open is worse than none; the
   **UserPromptSubmit `activate` hook is fail-*open*** — a missing binary prints an advisory line and
   `exit 0` (`hooks/skill-activation.sh:23,27`), because skill routing is advisory, not a boundary. The
   two hooks resolve in *opposite* order today (scan prefers the repo-built binary so a stale PATH binary
   can't shadow the veto, `security-scan.sh:16-26`; activate prefers PATH); `$GATEKEEPER_BIN`, once added,
   takes precedence and then defers to each hook's existing order. **Resolution transparency — which
   binary won and its version — is `doctor`'s job, not the hooks'; the hooks stay silent on the happy
   path** (`security-scan.sh:28-30`).
   *Residual, stated plainly:* the binary is **not** self-contained in the plugin — the user installs it
   separately (`install.sh`/`cargo install`), so "one-command install" is "add marketplace + install
   plugin **and** build the binary," not a single step. Prebuilt binaries (decision 4) are a convenience
   layer that softens this, and bundling them in `bin/` later is a reversible upgrade, not a rewrite.

2. **Binary version and rules-schema version are independent; `--version` surfaces both.**
   `gatekeeper --version`/`-V` prints `gatekeeper <CARGO_PKG_VERSION> (rules schema v<SCHEMA_VERSION>)` —
   the tool version from `env!("CARGO_PKG_VERSION")` (compile-time, no new dependency) and the rules data
   format version from the existing `scan::SCHEMA_VERSION` const. **Pre-implementation guardrail
   (`[rev:codex-2026-06-09]`):** `scan::SCHEMA_VERSION` is private; rather than have `--version` and
   `doctor` reach into it (or re-declare `1`), introduce a small internal `version` seam (e.g.
   `version::tool()` / `version::rules_schema()`) that those **two** consumers depend on. *Not* `check
   docs` — the docs-coverage lint reads no schema version, so it is not a seam consumer. It is a
   visibility/ownership refactor, no new dependency. They are **decoupled on purpose**
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
   remains the distribution path for the other three harnesses. **This is a deliberate scope cut, not a
   capability gap (`[rev:codex-2026-06-09]`):** Codex CLI (`codex plugin marketplace`, verified 0.137.0)
   and OpenCode (`opencode plugin`, verified 1.14.22) *do* have plugin systems now (research B5); only
   Cursor (`.cursor/rules/*.mdc`) has none. Building native Codex/OpenCode plugins is a viable but
   explicitly deferred future extension — we package one plugin (Claude Code) this terminal phase rather
   than four. `adapt --harness claude` + `install.sh` remain the local-dev wiring.

6. **`doctor`, docs-coverage lint, and crates.io metadata are all in scope this phase.**
   - **`gatekeeper doctor`** — a read-only health check (binary version; `security/rules.toml` present and
     its `schema_version` supported; `instincts/` and `skills/` parse via their existing loaders; hooks
     present/executable; git pre-commit hook installed). Pure introspection over machinery that already
     exists; reuses existing parsers, no new dependency.
   - **Docs-coverage lint** — a new `gatekeeper check docs` arm (no `--feature`) enforcing repo-doc
     invariants the link-checker can't: every `skills/*/SKILL.md` has valid frontmatter, every
     `docs/verify/*.md` note *referenced from* `docs/ROADMAP.md` resolves to a file on disk (a phase that
     cites no note — e.g. Phase 0, the no-code Blueprint — is exempt), and every `docs/adr/00NN-*.md` is
     linked from `docs/adr/README.md`. This is the "coverage" half of "link/coverage lint" (lychee is the "link"
     half); it lives as a `check` arm (deterministic, testable via the existing CLI harness, instinct:
     [[gates-not-rules]]) and is wired into the justfile/CI.
     **Reconciliation precondition (`[rev:codex-2026-06-09]`):** the repo *already violates* these
     invariants, so a three-file reconciliation must land **before** the lint is enabled, or it red-flags
     its own repo on introduction: (a) `docs/adr/README.md`'s index stops at 0007 — add 0008, 0009, 0010;
     (b) `docs/ARCHITECTURE.md` tags shipped modules (`scan`, `instinct`, `learn`, `adapt`, `check
     research`, the adapters, `security-scan.sh`, `pre-commit.sh`) `[planned]` and says "static binary" —
     flip to `[built]` and fix the wording; (c) `README.md` advertises a `.claude-plugin/plugin.json` that
     does not yet exist (`README.md:28-29,78`) and repeats "static binary" (`:82`) — creating the manifest
     this phase (decision 5) resolves the phantom (code catches up to doc), and the wording is corrected.
     The "static binary" phrase itself overclaims: the macOS-arm64 target (decision 4) is a *single
     std-only executable* but dynamically links `libSystem` — it is not a fully static musl artifact.
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
