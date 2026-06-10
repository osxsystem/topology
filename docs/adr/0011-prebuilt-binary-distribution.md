# 0011 — Prebuilt-first binary distribution: release matrix, installer download, plugin self-provisioning

- **Status:** 🟢 Accepted
- **Date:** 2026-06-10

## Context

[ADR-0010](0010-packaging-distribution.md) §1 chose system-PATH distribution and stated the
residual plainly: one-command install was really "install plugin **and** build the binary." It also
pre-authorized this change: bundling prebuilt binaries "is a reversible upgrade, not a rewrite."
The [research](../research/2026-06-10-one-command-install.md) confirmed the upgrade path against
the official plugin docs and the runner matrix. This ADR exercises that authorization.

## Decisions

1. **Release matrix replaces the single target** (revises ADR-0010 §4). Four std-only targets —
   `aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-unknown-linux-gnu`,
   `aarch64-unknown-linux-gnu` — each attached as `gatekeeper-<triple>` plus one `SHA256SUMS`. The
   version guard widens to tag == Cargo.toml == plugin.json == marketplace.json. Windows stays out
   of scope (no hook story there), a recorded choice, not an omission.

2. **Prebuilt binaries outrank built ones in hook resolution** (extends ADR-0010 §1). Both hooks
   insert `$ROOT/bin/gatekeeper` then `$CLAUDE_PLUGIN_DATA/bin/gatekeeper` immediately after the
   `$GATEKEEPER_BIN` override; everything downstream — including the deliberate **fail-policy
   split** (PreToolUse scan fail-closed, UserPromptSubmit activate fail-open) and each hook's
   legacy ordering — is unchanged. Binaries are never committed to git (`/bin/` is ignored);
   "bundled" means placed at install time or provisioned at session start.

3. **The plugin self-provisions via SessionStart into `${CLAUDE_PLUGIN_DATA}/bin`** — the
   officially documented dependency-provisioning pattern, into the directory that survives plugin
   updates. The hook is silent and network-free when any binary already resolves, and fail-open
   (advisory, exit 0) when offline, because a session must still start; the security floor is
   independent — the scan hook keeps denying until a binary exists.

4. **Download trust is proportionate to the documented threat model** (mistakes, not a determined
   evader — see [[match-threat-model-before-importing-hardening]]): HTTPS to github.com only, the
   version **pinned to the committed manifest** (never "latest"), SHA-256 verification against the
   release's checksum file, and a `--version` smoke test before the atomic move into place.
   Residual trust is GitHub Releases infrastructure, accepted and stated. Test seams
   (`$TOPOLOGY_RELEASE_BASE_URL`, `$TOPOLOGY_VERSION`) keep the whole path verifiable offline via
   `file://` fixtures.

5. **The installer becomes the one-liner, prebuilt-first, with a manifest.** `scripts/install.sh`
   works piped (`curl … | bash`: clones to `${TOPOLOGY_HOME:-$HOME/.topology}`) or from a checkout;
   tries the release download first and falls back to `cargo build` (`--build-from-source` forces
   it); and ends by printing the resolved binary path and **every file it created or modified** —
   the install is auditable from its own output.

6. **No `"skills"` manifest field.** Plugin skill auto-discovery already registers `skills/`
   natively; the field *adds* scan paths rather than replacing them, so pointing it at `./skills/`
   invites double registration. The native-skills story is documentation, not configuration.

## Consequences

- A clean machine without Rust gets a working install in one command on the four covered platforms;
  everyone else keeps the cargo path with a clear message.
- A plugin-only install is now genuinely complete after `/plugin install` — first session
  provisions the binary.
- The fetch path adds a network dependency at install/first-session time only; steady-state
  sessions do a few `-x` tests and never touch the network.
- Releasing now requires the three manifests to move together; the widened guard turns drift into a
  failed release instead of a broken installer.
