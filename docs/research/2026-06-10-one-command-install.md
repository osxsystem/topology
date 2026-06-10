# Research — one-command install (prebuilt binary distribution)

## Problem

Installing Topology today is not one command. The dev path is clone → `./scripts/install.sh`
(which requires a Rust toolchain: `cargo build --release`) → paste hook config. The plugin path is
`/plugin marketplace add osxsystem/topology` → `/plugin install topology@topology` **and still**
build the binary separately — [ADR-0010](../adr/0010-packaging-distribution.md) §1 states this
residual plainly: *"'one-command install' is 'add marketplace + install plugin **and** build the
binary,' not a single step."* The same ADR pre-authorizes the fix: *"bundling them in `bin/` later
is a reversible upgrade, not a rewrite."* This research scopes that upgrade.

## What exists today (verified on this tree, 2026-06-10)

- **Release pipeline** (`.github/workflows/release.yml`): on `v*` tags, a version-match guard
  (tag == `gatekeeper/Cargo.toml`) and a single-target build, `aarch64-apple-darwin`, attached to a
  GitHub Release as `gatekeeper-aarch64-apple-darwin`. ADR-0010 §4 records single-target as a
  deliberate scope choice with the matrix as the named follow-up.
- **Hook resolution order** (the seam the upgrade slots into):
  - `hooks/security-scan.sh:19-29` — `$GATEKEEPER_BIN` → repo release build → repo debug build →
    PATH → **deny** (fail-closed).
  - `hooks/skill-activation.sh:17-28` — `$GATEKEEPER_BIN` → PATH → repo release → repo debug →
    **advisory line, exit 0** (fail-open).
  - The fail-policy split is a load-bearing ADR-0010 decision and must survive any resolver change.
- **`scripts/install.sh`** assumes it runs from inside a checkout (`BASH_SOURCE` walk-up), requires
  cargo unconditionally, and prints the hook config but no manifest of what it touched.
- **Versions**: `gatekeeper/Cargo.toml`, `.claude-plugin/plugin.json`, and
  `.claude-plugin/marketplace.json` all say `0.1.0`. The release guard only checks Cargo.toml.

## Claude Code plugin facts (official docs, fetched 2026-06-10 via code.claude.com/docs)

1. **Skills are already native.** A `skills/` directory at the plugin root is auto-discovered and
   registered as Agent Skills with no manifest field; each `SKILL.md` needs only `description`
   frontmatter (ours all carry `name` + `description`). The optional `"skills"` manifest field
   *adds* paths to the default scan rather than replacing it — pointing it at `./skills/` again
   risks a double registration of the same directory. Conclusion: the "register skills natively"
   ask is satisfied by the existing layout; the right move is to document it, not to add the field.
2. **`${CLAUDE_PLUGIN_DATA}`** is a persistent per-plugin directory (`~/.claude/plugins/data/<id>/`)
   that survives plugin updates; the docs' own example uses a **SessionStart hook** to provision
   dependencies into it on first load. `${CLAUDE_PLUGIN_ROOT}` is the install dir and is replaced
   on update — wrong home for a downloaded binary.
3. **`bin/` at the plugin root** is automatically added to the Bash tool's PATH while the plugin is
   enabled. Hook processes should still resolve it explicitly rather than rely on PATH inheritance.
4. **No direct-from-GitHub non-interactive plugin install exists.** `claude plugin install
   <name>@<marketplace>` works from a shell but only against a registered marketplace. So the true
   one-liner for non-plugin users is a hosted `install.sh` (`curl -fsSL … | bash`), and the plugin
   flow becomes one *logical* step once the binary self-provisions.

## Runner availability for a release matrix

Public-repo GitHub-hosted runners cover all four interesting targets at no cost: `macos-14`
(arm64, also cross-compiles `x86_64-apple-darwin` with `--target`), `ubuntu-latest` (x64), and
`ubuntu-24.04-arm` (arm64). `rustup target add` + `cargo build --target` suffices; no musl or
cross-rs needed (ADR-0010 already corrected "static binary" to "std-only, dynamically linked").

## Threat-model check ([[match-threat-model-before-importing-hardening]])

Downloading a binary in a SessionStart hook adds a network trust edge. Topology's documented
boundary is *mistakes, not a determined evader* (AGENTS.md security floor note). Proportionate
controls: HTTPS to github.com only, a version **pinned to the manifest** (no "latest" drift),
SHA-256 verification against the release's checksum file, and an `--version` smoke test before the
binary is moved into place. Residual trust = GitHub Releases infrastructure, stated in the ADR.
Crucially the **fail-closed scan hook is unchanged**: if provisioning fails, `security-scan.sh`
still denies — the floor never silently opens.

## Conclusion

All four asks from the request map onto pre-authorized or documented seams:

| Ask | Seam |
|---|---|
| Prebuilt binaries preferred by hooks | ADR-0010 §1's named "reversible upgrade" — add `bin/` + `${CLAUDE_PLUGIN_DATA}/bin` to both resolvers |
| One-liner installer | Rework `scripts/install.sh` to be pipe-able (clone if no checkout, download release asset, cargo only as fallback) |
| Skills registered natively | Already true via plugin auto-discovery — document, don't duplicate |
| Post-install manifest | Track every file the installer creates/modifies; print paths + binary location at the end |
