# Research — installer v3 (Phase 8)

## Problem

ROADMAP Phase 8 wants both install scopes consuming the release payload, the install
verifying the world the user will run in, and the plugin channel retired. Audit of
`scripts/install.sh` and the plugin surface (2026-06-12, main @ v0.6.0) shows the phase is
**half-shipped**: the `--project` scope and the safety machinery landed with the
one-command-install work; the global scope and the plugin retirement did not.

## Already delivered (verified in source, not just claimed)

- **`--project` scope is fully payload-based** (`install.sh:179–341`): checkout mode builds
  the payload via `build-payload.sh`; piped mode downloads `topology-payload.tar.gz` +
  `SHA256SUMS` with checksum verification, then unpacks.
- **Pre-commit target bug is fixed** (`install.sh:452–470`): hook installs into
  `$PROJECT_PATH/.git/hooks/pre-commit` (the repo the developer commits to), copied not
  symlinked.
- **Post-install doctor runs from the project root** (`install.sh:723–732`), with
  `TOPOLOGY_ROOT` passed explicitly.
- **Legacy-clone migration for `--project`** (`install.sh:193–258`): `_rescue_legacy_clone`
  copies `docs/learn/ledger.md` and `docs/memory/*.handoff.md` into
  `<project>/.claude/topology/{learn,memory}/` (never overwrites), prompts before `rm -rf`
  (`--yes` honors non-interactive), refuses unidentifiable trees.
- **`--build-from-source`** exists and builds into the payload layout for `--project`
  checkout installs; piped+`--build-from-source` fails early with a remedy
  (`install.sh:151–158`).
- **Payload already excludes the plugin files** (`build-payload.sh:75–85`; asserted by
  `test-build-payload.sh:98–115`), and `topology-payload.tar.gz` + `SHA256SUMS` ship in
  every release (`release.yml:85–123`, verified on v0.6.0).

## Remaining work

**R1 — global scope still git-clones.** `install.sh:162–178`: a piped `--global` install
clones `https://github.com/osxsystem/topology.git` into `~/.topology` (or `git pull
--ff-only` an existing clone); a checkout run uses the checkout itself as `ROOT`. None of
the payload machinery (checksum verification, `_handle_existing_root`, legacy-clone rescue)
applies to global. Every global install is a legacy clone factory — the exact thing the
`--project` migration path exists to clean up.

**R2 — plugin channel retirement.** Inventory of everything that must change together:
- Delete: `.claude-plugin/plugin.json`, `.claude-plugin/marketplace.json`,
  `hooks/ensure-gatekeeper.sh`, `hooks/hooks.json` (referenced only by `plugin.json` — the
  dev clone wires its hooks via user-level settings, not `hooks.json`, so dogfooding is
  unaffected; verified: no `.claude/` dir in-repo, no other live references).
- `release.yml:24–44` version guard reads `plugin.json` + `marketplace.json` — **must drop
  those two probes in the same change or the next tag fails the guard.**
- `gatekeeper/src/main.rs:297` `ROOT_MARKERS` includes `".claude-plugin"`, echoed in the
  doctor F1 message (`doctor.rs:103`). Beyond staleness there is a correctness argument for
  removing it: `.claude-plugin/` + `skills/` is the standard layout of *any* Claude Code
  plugin checkout, and since Phase 11 a git repo that `is_marked_root` self-governs
  (resolution step 2) — an unrelated plugin repo would claim to be a Topology framework
  root. `AGENTS.md` and `gatekeeper/` remain as markers; the shipped payload carries
  `AGENTS.md` (`build-payload.sh:110–115`), so no install mode loses its marker.
- Docs: README `## Install as a Claude Code plugin` (line 129); USER-GUIDE `### Option B —
  As a Claude Code plugin` (145–169), hook-inventory lines mentioning `ensure-gatekeeper.sh`
  (157, 228), plugin uninstall steps (317–322).
- `main.rs` and the release workflow are protected/sensitive paths — documented `--no-verify`
  override per the Track 2 grant.

**R3 — `sudo ln -sf … /usr/local/bin` suggestion** (`install.sh:720`) — remove per ROADMAP
(superseded by `GATEKEEPER_BIN` wiring in Phase 9); the stale-PATH repair
(`install.sh:567–599`) stays.

**R4 (verify mechanism, not a roadmap bullet) — the installer test suites are not in CI.**
`justfile` has `test-payload`, `test-fetch`, `test-e2e` recipes but `check` excludes all
three; CI runs only `just check`. The offline e2e suite (`test-payload-e2e.sh`, 450 lines)
already covers most of the Phase 8 verify criteria for `--project`. Wiring the offline
suites into CI (and extending e2e to the new global path) is how this phase's verify
criteria stay true after the phase ends.

## Open questions for the spec

1. **Checkout-mode global install** (`BASH_SOURCE` resolves): today `ROOT` = the checkout
   itself. Options: (a) keep as the dev mode; (b) assemble the payload into `~/.topology`
   like the piped path, mirroring `--project` checkout behavior. Recommend (b) for one
   consistent world — the dev repo still self-governs via resolution step 2 (Phase 11), so
   dev workflows lose nothing.
2. **Global legacy-clone rescue destination**: `--project` rescues into the project's
   artifacts root; a global clone has no project context. Recommend: rescue
   `docs/learn/ledger.md` + `docs/memory/*.handoff.md` into a timestamped sibling backup
   (`~/.topology-backup-<date>/`) before deletion, with the same prompt/`--yes` discipline —
   no silent data loss, no invented project.
3. **ROOT_MARKERS**: remove `".claude-plugin"` in this phase (argument above) or defer?
   Recommend: remove here — it is the plugin channel's last live tendril, and the Phase 11
   self-governed step widened its blast radius.
4. **CI wiring**: add `test-payload` + `test-fetch` + `test-e2e` to `just check` (slower but
   one entrypoint) or as a separate CI job? Recommend a separate offline `installer` CI job —
   keeps `just check`'s edit-loop latency, still gates merges.
