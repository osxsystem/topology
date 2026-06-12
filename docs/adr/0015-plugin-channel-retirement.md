# 0015 — Plugin channel retired: one install channel, payload-only provisioning

- **Status:** 🟢 Accepted
- **Date:** 2026-06-12
- **Amends:** [ADR-0010](0010-packaging-distribution.md) §5 (hand-authored plugin manifests),
  [ADR-0011](0011-prebuilt-binary-distribution.md) §§1–4 (three-manifest version guard,
  plugin-data hook resolution, SessionStart self-provisioning, manifest-pinned downloads)
- **Spec:** [installer v3](../specs/2026-06-12-installer-v3.md) · ROADMAP Phase 8

## Context

ADR-0010 established the Claude Code plugin (`.claude-plugin/` manifests, `hooks/hooks.json`)
as a supported distribution channel; ADR-0011 added SessionStart self-provisioning
(`hooks/ensure-gatekeeper.sh`) and widened the release version guard to three manifests.
Phase 7/8 then built the payload channel: a curated tarball consumed by `install.sh` for both
scopes. Maintaining two channels meant two provisioning paths, two version surfaces to guard,
and — after Phase 11's self-governed resolution step — a live correctness hazard:
`.claude-plugin/` sat in `ROOT_MARKERS`, and `skills/` + `.claude-plugin/` is the standard
layout of *any* Claude Code plugin checkout, so an unrelated plugin repo could claim to be a
Topology framework root.

## Decision

One install channel: `scripts/install.sh`, payload-only, both scopes.

1. **Deleted:** `.claude-plugin/plugin.json`, `.claude-plugin/marketplace.json`,
   `hooks/ensure-gatekeeper.sh`, `hooks/hooks.json`, and the README/USER-GUIDE plugin
   sections. The marketplace listing dies with the manifests.
2. **Version guard narrows** to tag == `gatekeeper/Cargo.toml` (amends ADR-0011 §1 — the
   other two manifests no longer exist). `Cargo.toml`/`Cargo.lock` are the only version
   manifests.
3. **`ROOT_MARKERS` drops `.claude-plugin`** → `["AGENTS.md", "gatekeeper"]`. Every install
   mode ships `AGENTS.md`, so nothing legitimate loses its marker; plugin-shaped repos stop
   qualifying (regression-tested at unit and binary level).
4. **Version resolution** (amends ADR-0011 §4): with the committed plugin manifest gone, the
   installer and `fetch-gatekeeper.sh` default to the **latest release**
   (`releases/latest/download/`), with `TOPOLOGY_VERSION` as the explicit pin for
   rollback/reproducibility. The payload's own `VERSION` file remains the post-install
   source of truth (`doctor` skew check).
5. **Global installs consume the payload** like local ones (download + SHA-256 verify, or
   checkout assembly via `build-payload.sh`); the `git clone`/`git pull` path is removed.
   Legacy global clones are rescued to a timestamped sibling backup
   (`${ROOT}-backup-<ts>/`) before replacement — there is no project artifacts root to
   rescue into. Rescue covers `docs/learn/ledger.md`, `docs/memory/*.handoff.md`, **and**
   clone-era `memory/artifacts/*` (the ADR-0013 consequence, both scopes).
6. **Non-interactive legacy-clone handling follows ADR-0012 §4** (apply the printed
   default): the prompt default is "N", so a headless run without `--yes` refuses with a
   remedy instead of deleting. Rescue + delete run only when replacement is confirmed.

## Consequences

- Exactly one provisioning path to maintain, test (offline e2e suite, CI `installer` job),
  and reason about; the SessionStart self-provisioning fail-open path is gone.
- Anyone on the plugin channel must reinstall via `install.sh` (the binary keeps working;
  hooks wired by `adapt` are unaffected — only the plugin's self-provisioning dies).
- "Latest by default" trades reproducibility for freshness at install time; pinning is one
  env var away, and the in-payload `VERSION` + doctor skew check catch drift after install.
- ADR-0010 §5 and ADR-0011 §§1–4 are amended rather than the ADRs superseded wholesale —
  their remaining decisions (release matrix, system-PATH binary model, CI-mirrors-justfile)
  stand unchanged.
