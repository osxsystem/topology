# Spec — distribution payload (Phase 7)

## Goal

A release artifact that is the unit of install — the tool without the workshop. A governed project
receives the operators and the enforcement binary, never the framework's source, docs, or git
history. Decisions: grilled 2026-06-10 (ROADMAP Track 2); state model in
[ADR-0013](../adr/0013-payload-read-only-artifacts-root-state.md).

## Shape

**One platform-neutral tarball** with a stable asset name, published on every tagged release beside
the existing four platform binaries:

```
topology-payload.tar.gz        # stable name: releases/latest/download/ resolves without knowing the version
SHA256SUMS                     # existing file; gains a payload entry
gatekeeper-<triple>            # existing four-target binary matrix, unchanged
```

The platform-specific binary is **not** in the tarball — `scripts/fetch-gatekeeper.sh` (which ships
in the payload) fetches it into the payload's `bin/` at install time. Rationale: the binary
pipeline already exists and is verified; one neutral payload is one artifact to build and checksum;
the `VERSION` file pins payload↔binary lockstep because both come from the same tag.

## Manifest

Tarball contents (flat — unpacks into the target directory, no wrapping top-level dir):

| Entry | Note |
|---|---|
| `hooks/skill-activation.sh` | UserPromptSubmit routing |
| `hooks/security-scan.sh` | PreToolUse veto |
| `hooks/pre-commit.sh` | pre-commit veto (installed into the **project's** `.git/hooks` — Phase 8) |
| `hooks/learn-capture.sh` | opt-in Stop hook |
| `hooks/skill-rules.json` | keyword/file → skill routing table |
| `skills/` | all skills, verbatim |
| `instincts/` | all instincts, verbatim |
| `security/rules.toml` | scan rules |
| `scripts/fetch-gatekeeper.sh` | run post-unpack by the installer; reads the payload `VERSION` (no more `plugin.json`) |
| `VERSION` | see format below |
| `AGENTS.md` | root-marker sentinel: `is_marked_root()` requires `skills/` plus one of `ROOT_MARKERS = ["AGENTS.md", "gatekeeper", ".claude-plugin"]`; without it the unpacked payload cannot be resolved as the framework root |
| `CONTRACT.md` | reserved slot — the rendered operating contract (Phase 10); absent until then |

**Excluded** (the workshop): `gatekeeper/` source, `docs/`, `RESEARCH.md`, `METHODOLOGY.md`,
`.github/`, `.claude-plugin/`, git history, `memory/TEMPLATE.handoff.md` (compiled into the binary
via `include_str!`), `adapters/` (documentation only), `hooks/ensure-gatekeeper.sh` and
`hooks/hooks.json` (plugin-only; the plugin channel retires in Phase 8).

The repo layout already equals the payload layout — assembly is a copy list, no restructuring.

## VERSION file

Line-anchored TOML, parseable by both the bash `grep -m1` idiom and the `toml` crate gatekeeper
already carries:

```toml
version = "0.4.0"
rules_schema = 1
```

Consumers: `doctor` (payload↔binary skew check), `fetch-gatekeeper.sh` (which binary to download),
the installer's stale-binary repair.

## Version resolution

- The installer downloads the **latest release** (`releases/latest/download/topology-payload.tar.gz`).
- `TOPOLOGY_VERSION=<ver>` overrides for pinning/rollback (constructs the versioned release URL).
- Single source of truth: `gatekeeper/Cargo.toml`. The CI version-guard asserts tag == Cargo.toml
  and drops its two `plugin.json` checks when the plugin channel retires (Phase 8).
- `TOPOLOGY_RELEASE_BASE_URL` stays as the offline/file:// test seam.

## State model changes in gatekeeper (ADR-0013)

The payload is read-only at runtime, so two write paths move from `framework_root()` to
`artifacts_root()`:

1. **`memory write/read/list`** → `<artifacts_root>/memory/` — governed: `.claude/topology/memory/`;
   framework repo: `docs/memory/` (one-time `git mv memory/artifacts docs/memory`).
2. **`learn capture/list`** → `<artifacts_root>/learn/ledger.md` — governed:
   `.claude/topology/learn/ledger.md`; framework repo: unchanged (`docs/learn/ledger.md`, since
   `artifacts_root()` = `docs/` there). `LEDGER_REL` becomes relative to the artifacts root.
3. **`learn promote`** refuses when project ≠ framework (exit non-zero, message pointing at the
   fork story). Capture is unaffected everywhere.

## CI assembly

In `release.yml`, after the binary matrix: a `payload` job checks out the tag, stages the manifest
copy list, writes `VERSION` from the tag, `tar -czf topology-payload.tar.gz -C stage .`, appends
its checksum to `SHA256SUMS`, uploads as a release asset. No new toolchain — bash + tar on
`ubuntu-latest`.

## Acceptance criteria

1. A tagged release carries `topology-payload.tar.gz` and its `SHA256SUMS` entry.
2. Unpacking the tarball + running its `scripts/fetch-gatekeeper.sh` yields a tree where
   `bin/gatekeeper --version`, `activate`, and `scan` all work with `TOPOLOGY_ROOT` pointed at it.
3. The tarball contains no `*.rs`, no `docs/`, no `.git`, no plugin files.
4. `VERSION` parses by both consumers (bash grep, `toml` crate) and matches the release tag.
5. In a governed fixture: `memory write` lands under `.claude/topology/memory/`, `learn capture`
   under `.claude/topology/learn/ledger.md`, and `learn promote` refuses with the fork message.
6. In the framework repo: `learn` paths are byte-identical to today; `memory` reads/writes
   `docs/memory/` after the one-time `git mv`.
7. `cargo test` covers: artifacts-root anchoring for memory/learn, the promote refusal, and
   VERSION parsing.
