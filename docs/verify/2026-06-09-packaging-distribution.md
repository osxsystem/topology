# Verify — Packaging & distribution (Phase 6)

- **Date:** 2026-06-09
- **Feature slug:** packaging-distribution
- **Branch:** `feat/packaging-distribution`
- **Commits verified:** `ede49de..5e975d1` (12 commits — research/ADR/spec/plan through Tasks 1–8)
- **Spec:** [docs/specs/2026-06-09-packaging-distribution.md](../specs/2026-06-09-packaging-distribution.md) — 12 acceptance criteria
- **ADR:** [docs/adr/0010-packaging-distribution.md](../adr/0010-packaging-distribution.md)
- **Method:** the quality gates were run, then every acceptance criterion was exercised with a
  re-runnable command and its exit code captured live. Functional CLI checks use the repo-built
  release binary `gatekeeper/target/release/gatekeeper`; the docs-lint and hook fail-policy checks
  run hermetically over `mktemp -d` scratch roots so they do not couple to the real tree. The
  `claude` CLI (2.1.169) **was** present, so `claude plugin validate .` ran for real (criterion 6).

---

## Quality gates (criterion 9)

| Gate | Command | Result |
|---|---|---|
| Format | `cargo fmt --manifest-path gatekeeper/Cargo.toml --check` | **exit 0** (clean) |
| Lint | `cargo clippy --manifest-path gatekeeper/Cargo.toml --all-targets -- -D warnings` | **clean** — no issues |
| Tests | `cargo test --manifest-path gatekeeper/Cargo.toml` | **213 passed, 2 ignored, 0 failed** (9 binaries) |
| Offline aggregate | `just check` (fmt-check · lint · test · shell · typos · **docs**) | **exit 0** — ends `check docs: ok` |
| Deps audit (network) | `just deny` | **exit 0** — advisories ok, bans ok, licenses ok, sources ok |
| Link check (network) | `just links` | **exit 0** — 155 total / 155 OK / 0 errors (4 redirects, advisory only) |
| No new dependency | `git diff main -- gatekeeper/Cargo.toml gatekeeper/Cargo.lock` | `Cargo.toml` adds only `[package]` metadata; **`Cargo.lock` unchanged** (ADR-0010 consequences) |

Per-binary test counts: `main.rs` unittests 113 (+2 ignored) · `cli_adapt` 8 · `cli_check` 13 ·
`cli_doctor` 5 · `cli_instinct` 8 · `cli_learn` 10 · `cli_memory` 11 · `cli_review` 1 · `cli_scan` 44.

---

## Acceptance criteria

`[n]` is the expected exit code. `$BIN` = `gatekeeper/target/release/gatekeeper`.

### 1. Version surface — ✅
```
$BIN --version   → exit 0   gatekeeper 0.1.0 (rules schema v1)
$BIN -V          → exit 0   gatekeeper 0.1.0 (rules schema v1)
$BIN --help      → header:  topology gatekeeper 0.1.0 (rules schema v1)
```
`0.1.0` equals `Cargo.toml`'s `version`; `v1` equals `scan::SCHEMA_VERSION`. The advertised↔accepted
round-trip and the seam delegation are pinned by in-crate unit tests (see criterion 9).

### 2. `doctor` is read-only, accurate, surfaces binary resolution — ✅
```
$BIN doctor                              → exit 0
    binary:  …/gatekeeper/target/release/gatekeeper
    version: gatekeeper 0.1.0 (rules schema v1)
    GATEKEEPER_BIN: not set
    PATH gatekeeper: /Users/…/.cargo/bin/gatekeeper
    repo build: …/gatekeeper/target/release/gatekeeper
    resolution split: scan prefers the repo build; activate prefers PATH; $GATEKEEPER_BIN overrides both
    security/rules.toml: ok | instincts/: ok | skills/: ok | hooks/*.sh: ok
    .git/hooks/pre-commit: n/a (no .git directory — plugin/PATH install)
GATEKEEPER_BIN=/nonexistent $BIN doctor  → exit 1   "GATEKEEPER_BIN: set → /nonexistent (FAIL: missing or not executable)" … "doctor: 1 probe(s) FAILED"
# tree unchanged across a run:
git status --porcelain (pre) vs (post)   → identical (doctor writes nothing)
```
Doctor prints the **resolved binary path + version + resolution mechanism**, exactly as criterion 2
requires. The schema-mismatch and non-executable-hook fault paths are additionally pinned hermetically
by `tests/cli_doctor.rs` (`doctor_schema_version_mismatch_exits_1`, part of the 213).

### 3. Docs-coverage lint passes clean and catches gaps — ✅
Hermetic CLI tests (`tests/cli_check.rs`, part of the 213):
```
check_docs_clean_root_exits_0                       → exit 0   (R1+R2+R3 satisfied)
check_docs_broken_skill_frontmatter_exits_1         → exit 1   (R1 gap)
check_docs_adr_absent_from_readme_exits_1           → exit 1   (R2 gap)
check_docs_roadmap_verify_pointer_missing_exits_1   → exit 1   (R3 gap)
```
Real-repo cleanliness is the gate's job, not a `cargo test` assertion: `just docs`
(`cargo run -- check docs`, folded into `just check` + pre-commit) → **exit 0**, prints `check docs: ok`.

### 4. CI workflow runs the existing gate — ✅
`.github/workflows/ci.yml` parses as valid YAML; triggers on `push` + `pull_request`. The blocking job
runs the recipes the human runs — `just check` and `cargo run -- check docs` — not re-spelled `cargo`
lines. A **separate** job with `continue-on-error: true` runs the network gates `just deny` + `just
links`, so flake cannot wedge a PR merge. The gate it mirrors is shown green locally above (`just check`
→ exit 0). *A green remote CI run is linked once the workflow has executed on push (external; see residuals).*

### 5. Release workflow guards the version and builds the artifact — ✅
`.github/workflows/release.yml` parses as valid YAML; triggers on `push: tags: 'v*'`. It contains a
**version-match guard** (`cargo_ver` from `Cargo.toml` vs `${GITHUB_REF_NAME#v}`, `::error::` + `exit 1`
on mismatch), a `cargo build --release --locked --target aarch64-apple-darwin` step, and a
`softprops/action-gh-release@v2` upload of the stripped arm64 binary. The guard logic was demonstrated
standalone:
```
tag v0.1.0  → MATCH (cargo=0.1.0)    → exit 0
tag v9.9.9  → MISMATCH (cargo=0.1.0) → exit 1
```

### 6. Plugin manifest validates — ✅ *(validated for real)*
```
claude plugin validate .   → "✔ Validation passed"
```
`.claude-plugin/plugin.json` is valid JSON: `name=topology`, `version=0.1.0` (== `Cargo.toml`),
`hooks=./hooks/hooks.json`; `hooks/hooks.json` is valid JSON wiring the scripts via
`${CLAUDE_PLUGIN_ROOT}`. (Field/JSON/version assertions also hold directly, per the ADR-0010
verifiability caveat — but the CLI was present, so the real validator ran.)

### 7. Marketplace manifest is installable — ✅
`.claude-plugin/marketplace.json` is valid JSON: `name=topology`, `owner={name: osxsystem, email:
osxsystem2014@gmail.com}`, and a `plugins[]` entry `name=topology, source=./, version=0.1.0`. The
install flow is documented in `scripts/install.sh`:
`/plugin marketplace add osxsystem/topology` → `/plugin install topology@topology`.

### 8. Plugin hooks resolve the binary via PATH with the correct **split** fail policy — ✅
Two genuinely-binary-less invocations (empty-ish `PATH=/usr/bin:/bin`, `GATEKEEPER_BIN` unset,
`CLAUDE_PLUGIN_ROOT` → a scratch dir with **no** `gatekeeper/target/…` build):
```
security-scan.sh   (PreToolUse)      → fail CLOSED: emits
    {"hookSpecificOutput":{…,"permissionDecision":"deny","permissionDecisionReason":
     "Topology: security scanner unavailable - run ./scripts/install.sh"}}    exit 0
skill-activation.sh (UserPromptSubmit)→ fail OPEN: prints
    "Topology: gatekeeper not built — run ./scripts/install.sh. Still: evaluate your skills before acting."  exit 0
GATEKEEPER_BIN=/nonexistent + scan    → fail CLOSED (same deny)              exit 0
```
The scan hook denies (a security veto must not fail open); the activate hook advises and continues
(routing is advisory). Both name the fix. Resolution order is `$GATEKEEPER_BIN → PATH → repo build`
(scan prefers the repo build over PATH; activate prefers PATH) — confirmed by reading
`hooks/security-scan.sh:19-29` and `hooks/skill-activation.sh:17-28`. On the happy path the scanner
is silent. *Note: the first attempt to prove this pointed `CLAUDE_PLUGIN_ROOT` at the real repo and the
hooks correctly found the repo-build fallback and ran — the no-binary case only holds against a root
with no `target/` build, which is what the recorded run uses.*

### 9. No new runtime dependency; suite stays green — ✅
`git diff main -- gatekeeper/Cargo.toml` shows only four `[package]` lines added (`authors`,
`repository`, `homepage`, `keywords`); `[dependencies]` is still exactly `regex` / `serde` /
`serde_json` / `toml`; `Cargo.lock` is unchanged. The version seam is pinned by in-crate unit tests
(part of the 213): `version::tests::tool_version_matches_cargo_pkg_version`,
`version::tests::rules_schema_delegates_to_scan` (the drift guard — pins that `rules_schema()` keeps
delegating to `scan`), and `version::tests::advertised_schema_is_accepted_by_parser` (the
advertised↔accepted round-trip — the one that can actually go red if the printed and parsed schema
decouple). fmt/clippy/test all clean (see Quality gates).

### 10. Packaging metadata present — ✅
```
cargo build --release --manifest-path gatekeeper/Cargo.toml          → exit 0 (1.5M binary)
cargo package --list --locked --manifest-path gatekeeper/Cargo.toml  → exit 0 (22 files listed)
```
`Cargo.toml` `[package]` carries `authors`, `repository`, `homepage`, `keywords`, plus the pre-existing
`description` and `license`.

### 11. Install + coexistence documented — ✅ *(with a recorded nuance)*
`scripts/install.sh` documents the binary prerequisite and the plugin path (lines 78–85): "The plugin
does NOT bundle the binary … hooks resolve gatekeeper via `$GATEKEEPER_BIN -> PATH`" and "The plugin
COEXISTS with adapt … it does not replace them." Coexistence proven live for all four harnesses:
```
# fresh worktree — configs never generated here:
adapt --harness {codex|cursor|opencode|claude} --check   → exit 1  "MISSING <config>"   (drift correctly detected)
# generate, then re-check:
adapt --harness {codex|cursor|opencode|claude}           → exit 0  (writes the config)
adapt --harness {codex|cursor|opencode|claude} --check   → exit 0  (now in sync)
```
**Nuance (recorded, not silently passed):** the per-harness configs (`.codex/config.toml`,
`.cursor/rules/*.mdc`, `opencode.json`, `.claude/settings.json`) are per-install artifacts — neither
committed nor gitignored. On a clean worktree where `adapt` has never run, `--check` therefore reports
`MISSING` (exit 1) rather than "clean" — it is correctly detecting that nothing is wired yet, not
failing. After `adapt --harness <h>` generates the file, `--check` is clean (exit 0), proving adapt
works alongside the `.claude-plugin/` manifests. The generated files were removed afterward; the tree
was confirmed byte-identical to its pre-run state.

### 12. Version-bump discipline is recorded — ✅
Setting an explicit `version` means **every release must bump `plugin.json` and `Cargo.toml` in
lockstep** (and any release tag `vX.Y.Z` must equal them), or installed plugins won't update and the
release fails. This is enforced, not merely documented: the release workflow's version-match guard
(criterion 5) fails the build on any divergence between the tag and `Cargo.toml`'s version. All three
are `0.1.0` today (criteria 1, 6, 7). This is the explicit, reviewed consequence of ADR-0010 §2 — not
an unstated gap.

---

## Residuals (accepted this phase — ADR-0010 consequences)

1. **The binary is not bundled in the plugin (PATH prerequisite, ADR-0010 §1).** The plugin resolves
   `gatekeeper` via `$GATEKEEPER_BIN → PATH`; a clean-machine install must build/install the binary
   first (`scripts/install.sh`). Shipping per-platform binaries in the plugin's `bin/` is a reversible
   future upgrade (a spec non-goal this phase).
2. **"CI green on a fresh clone" and the real `claude plugin validate` in CI are externally provable.**
   What is locally proven here: the workflow YAMLs parse, invoke the real recipes, and the recipes pass
   locally (`just check`/`just deny`/`just links` all green). The green *remote* run is linked once the
   workflow executes on push — it depends on GitHub-hosted runners and tool installs, which this note
   cannot exercise.
3. **`doctor` reports `.git/hooks/pre-commit: n/a` in this worktree.** This is a linked git *worktree*,
   so `.git` is a file (not a directory) and the probe takes the documented "no `.git/` directory →
   plugin/PATH install" path. That is the intended behaviour (a plugin install has no `.git`), exercised
   incidentally here; in a primary dev clone the probe checks the hook and FAILs if it is missing.
4. **macOS arm64 only (ADR-0010 §4).** The release matrix builds `aarch64-apple-darwin` alone; other
   targets are a one-workflow change later.

---

## Verdict

**All 12 acceptance criteria satisfied.** Quality gates green: fmt 0, clippy clean, **213 passed / 2
ignored / 0 failed**, `just check` exit 0, network gates (`deny`, `links`) green, no dependency change.
`claude plugin validate .` passed against the real CLI. Two items recorded honestly rather than
glossed: criterion 8's no-binary proof only holds against a root without a `target/` build (the
repo-build fallback otherwise — correctly — runs), and criterion 11's `adapt --check` reports `MISSING`
on a never-adapted worktree and is clean only after generation. **Phase 6 is delivered.**

---

### How to re-run

```sh
# Gates:
just check && just deny && just links
cargo fmt --manifest-path gatekeeper/Cargo.toml --check
cargo clippy --manifest-path gatekeeper/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path gatekeeper/Cargo.toml

BIN=gatekeeper/target/release/gatekeeper
cargo build --release --manifest-path gatekeeper/Cargo.toml

# 1 version / 2 doctor:
$BIN --version; $BIN -V; $BIN doctor
GATEKEEPER_BIN=/nonexistent $BIN doctor   # exit 1

# 3 docs lint (hermetic via tests) + real repo:
cargo test --manifest-path gatekeeper/Cargo.toml --test cli_check
just docs                                  # exit 0

# 4/5 workflows:
python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml')); yaml.safe_load(open('.github/workflows/release.yml'))"

# 6/7 manifests:
claude plugin validate .                   # if the CLI is present
python3 -c "import json; json.load(open('.claude-plugin/plugin.json')); json.load(open('.claude-plugin/marketplace.json')); json.load(open('hooks/hooks.json'))"

# 8 split fail policy (scratch root with NO target/ build):
S="$(mktemp -d)"
echo '{"tool_name":"Bash","tool_input":{"command":"ls"}}' | env -u GATEKEEPER_BIN PATH=/usr/bin:/bin CLAUDE_PLUGIN_ROOT="$S" bash hooks/security-scan.sh    # deny + exit 0
echo '{"prompt":"hi"}'                                    | env -u GATEKEEPER_BIN PATH=/usr/bin:/bin CLAUDE_PLUGIN_ROOT="$S" bash hooks/skill-activation.sh  # advisory + exit 0
rm -rf "$S"

# 11 coexistence (generates then removes per-install configs):
for h in codex cursor opencode claude; do $BIN adapt --harness "$h" && $BIN adapt --harness "$h" --check; done
rm -rf .codex .cursor .opencode opencode.json .claude/settings.json
```
