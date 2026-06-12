# Spec — installer v3: global payload + plugin retirement (Phase 8)

**Status:** approved
**Research:** `docs/research/2026-06-12-installer-v3.md`

## Goal

One install channel (`scripts/install.sh`), one provisioning format (the release payload) for
both scopes. No git clones at install time, no plugin channel, and the installer test suites
gate merges in CI.

## 1. Global scope consumes the payload (research R1)

`--global` mirrors the `--project` machinery, with `ROOT="${TOPOLOGY_HOME:-$HOME/.topology}"`:

- **Piped mode:** download `topology-payload.tar.gz` + `SHA256SUMS` (same
  `TOPOLOGY_RELEASE_BASE_URL` override, same `curl` flags, same checksum verification as the
  local path — extracted into shared helpers, not duplicated), then unpack into `$ROOT`.
- **Checkout mode (research Q1, decided):** assemble the payload via
  `scripts/build-payload.sh` into `$ROOT`, exactly like the `--project` checkout path — the
  checkout is no longer used as `ROOT` itself. Dev workflows lose nothing: the dev repo
  self-governs via resolution step 2 (Phase 11) no matter what lives in `~/.topology`.
- **`--build-from-source`:** checkout mode builds the binary into `$ROOT/bin` after payload
  assembly; piped mode fails early with the existing remedy text (same rule as local).
- **Existing `$ROOT` handling:** reuse `_handle_existing_root` semantics — `VERSION` present
  → in-place payload upgrade; `.git` present → legacy clone: rescue then prompt before
  `rm -rf` (`--yes`/no-tty honors the non-interactive warning path); neither → refuse with
  remedy. The `git pull --ff-only` update path is removed.
- **Global legacy-clone rescue (research Q2, decided):** a global clone has no project
  context, so `docs/learn/ledger.md` and `docs/memory/*.handoff.md` are rescued into a
  timestamped sibling backup `${ROOT}-backup-<YYYYmmdd-HHMMSS>/` (never overwrite an
  existing backup file; print every rescued path). No silent data loss, no invented project.

## 2. Plugin channel retirement (research R2)

All in one PR, because the release guard couples them:

- Delete `.claude-plugin/plugin.json`, `.claude-plugin/marketplace.json`,
  `hooks/ensure-gatekeeper.sh`, `hooks/hooks.json`.
- `release.yml` version guard drops the `plugin.json` / `marketplace.json` probes and
  asserts tag == `gatekeeper/Cargo.toml` only. (Without this, the next tag fails the guard
  on the deleted files.)
- `ROOT_MARKERS` (research Q3, decided): remove `".claude-plugin"` →
  `&["AGENTS.md", "gatekeeper"]`; update the doctor F1 message to match. Rationale:
  `.claude-plugin/` + `skills/` is the standard layout of any Claude Code plugin checkout,
  and since Phase 11 a marked git root self-governs — an unrelated plugin repo would claim
  to be a Topology framework root. Every install mode keeps `AGENTS.md`
  (`build-payload.sh:110–115`), so nothing legitimate loses its marker. Regression test: a
  repo with `skills/` + `.claude-plugin/` only is NOT a marked root.
- Docs: remove README `## Install as a Claude Code plugin`; USER-GUIDE Option B section,
  the `ensure-gatekeeper.sh` hook-inventory lines, and the plugin uninstall steps. The
  hooks table documents the four payload hooks only.
- Version bump to **0.7.0** in `Cargo.toml` + `Cargo.lock` — the only manifests left.

## 3. PATH suggestion cleanup (research R3)

Remove the `sudo ln -sf … /usr/local/bin/gatekeeper` suggestion (`install.sh:720`); the
stale-PATH detection/repair block (`install.sh:567–599`) is unchanged. Post-install notes
point at `gatekeeper doctor` and the existing hook/`GATEKEEPER_BIN` wiring instead.

## 4. CI wiring (research R4/Q4, decided)

New **offline `installer` job** in `ci.yml` (separate from the `just check` gate job to keep
edit-loop latency): runs `just test-payload`, `just test-fetch`, `just test-e2e`. Extend
`test-payload-e2e.sh` with a global-scope scenario: piped install against
`TOPOLOGY_RELEASE_BASE_URL=file://<staged payload>`, asserting the unpacked `~/.topology`
(remapped `HOME`/`TOPOLOGY_HOME`) contains payload files only (`VERSION` present, no `.git`,
no `*.rs`, no `docs/`), plus the legacy-global-clone rescue path (backup dir contents,
prompt-refusal leaves the clone intact, `--yes` replaces).

## Out of scope

- `GATEKEEPER_BIN` wiring in project settings (Phase 9) and the contract template (Phase 10).
- `adapt` changes; hook script changes beyond deleting the two plugin-only files.
- Any new dependency (ADR-0007). The payload contents list is unchanged.

## Acceptance criteria

1. Piped `--global --yes` install in an offline fixture (`TOPOLOGY_RELEASE_BASE_URL=file://…`,
   `HOME` remapped) unpacks the payload at `~/.topology`: `VERSION` present, no `.git`, no
   `*.rs`, no `docs/`; checksum verification runs (corrupted tarball fixture fails the
   install before touching an existing root).
2. Checkout `--global` install assembles the payload into `~/.topology` (the checkout is not
   `ROOT`); re-run upgrades in place.
3. Global legacy clone: rescue lands ledger + handoffs in `${ROOT}-backup-<ts>/`;
   interactive refusal leaves the clone intact (exit 1); `--yes` replaces with warning.
4. The four plugin files are deleted; `git grep` for `ensure-gatekeeper|hooks.json|claude-plugin`
   over the worktree returns only historical artifacts (docs/research, docs/specs,
   docs/plans, docs/reviews, docs/adr, CHANGELOG, ROADMAP) — no live references in
   `scripts/`, `hooks/`, `README.md`, `docs/USER-GUIDE.md`, `.github/`, `gatekeeper/src/`.
5. Release guard asserts tag == Cargo.toml only and passes a dry parse (the two JSON probes
   are gone).
6. `ROOT_MARKERS == ["AGENTS.md", "gatekeeper"]`; unit test: `skills/` + `.claude-plugin/`
   alone is not a marked root; doctor F1 message names the two remaining markers.
7. The `sudo ln` line is gone; the stale-PATH repair block is byte-identical.
8. `ci.yml` carries the offline `installer` job; `test-payload-e2e.sh` covers AC-1/AC-3
   scenarios and passes offline.
9. Full suite + `just check` green; version 0.7.0 in Cargo.toml/Cargo.lock; no new
   dependencies; protected-path commits (`main.rs`, `Cargo.toml`, workflows) carry the
   documented `--no-verify` override.
