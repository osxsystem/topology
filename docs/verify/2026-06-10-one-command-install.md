# Verify — one-command install

**Feature:** one-command-install
**Date:** 2026-06-10
**Spec:** [docs/specs/2026-06-10-one-command-install.md](../specs/2026-06-10-one-command-install.md)
**Verified by:** main-loop agent (Fable 5), independently re-running every acceptance criterion
after reviewing the delegated implementation — not by replaying the implementer's transcript.

## Symptom reproduced (the "before", on `main`)

`scripts/install.sh` on `main` hard-requires cargo (`error: cargo (Rust) is not installed` is its
first gate), assumes a checkout (`BASH_SOURCE` walk-up only), and ends with no record of what it
touched. The plugin path additionally required a separate binary build — ADR-0010 §1 records this
residual verbatim. Resolved below.

## AC-1 — Release matrix + widened guard (static)

```
ruby -ryaml … YAML OK
present: aarch64-apple-darwin / x86_64-apple-darwin / x86_64-unknown-linux-gnu
         / aarch64-unknown-linux-gnu / SHA256SUMS
```

The `version-guard` job extracts and compares all three manifest versions (Cargo.toml,
plugin.json, marketplace.json) against `${GITHUB_REF_NAME#v}`, `::error::`-naming the divergent
file; `build` needs it; `release` aggregates `sha256sum gatekeeper-* > SHA256SUMS` and publishes
five files. Live execution happens at the `v0.2.0` tag, post-merge.

## AC-2 — fetch-gatekeeper.sh round-trip (file:// fixture)

Fixture: the worktree's own release binary copied as `gatekeeper-aarch64-apple-darwin` + a real
`shasum -a 256` SUMS file under `v0.2.0/`.

- Clean fetch: `gatekeeper-aarch64-apple-darwin: OK` (stderr), stdout exactly one line — the
  installed path; the binary runs: `gatekeeper 0.2.0 (rules schema v1)`.
- Corrupted SUMS (first hex digit flipped): `shasum: WARNING: 1 computed checksum did NOT match`,
  exit non-zero, destination dir left empty.

**Bug found and fixed during this verification** (commit `fix(install): keep fetch stdout to the
path line…`): the shasum `OK` line originally leaked onto stdout, violating the path-only stdout
contract and garbling the SessionStart provisioning message. The implementer's self-check had
passed vacuously; the re-run above is post-fix.

## AC-3 — Hook resolution + fail policies

Temp `CLAUDE_PLUGIN_ROOT` with tagged wrapper scripts at both `bin/gatekeeper` and
`gatekeeper/target/release/gatekeeper`:

- `security-scan.sh` on a scan event printed `RESOLVED:bin` — the prebuilt outranks the repo build.
- Empty root + `PATH=/usr/bin:/bin` (no gatekeeper): the deny JSON
  (`"permissionDecision":"deny"`, reason naming `install.sh`) — fail-closed intact.
- Same empty setup through `skill-activation.sh`: the advisory line, exit 0 — fail-open intact.

## AC-4 — SessionStart self-provisioning

- Binary resolvable (worktree as root) + a deliberately unreachable
  `TOPOLOGY_RELEASE_BASE_URL=file:///nonexistent`: empty output, exit 0 — silent and provably
  network-free (a fetch attempt would have errored loudly).
- Empty chain + fixture, `CLAUDE_PLUGIN_DATA` set:
  `Topology: gatekeeper 0.2.0 (rules schema v1) provisioned at …/bin/gatekeeper`, exit 0, and the
  provisioned binary answers `--version`.
- Empty chain + unreachable URL: the advisory naming both remedies (`scripts/install.sh`,
  `cargo build --release`), exit 0.

## AC-5 — Piped installer end-to-end

`bash < scripts/install.sh` from an unrelated empty directory (so `BASH_SOURCE` is unset), with
`TOPOLOGY_HOME` pre-seeded by a local clone of this branch and the file:// fixture:

- Took the update path (`git pull --ff-only`), fetched the prebuilt into
  `$TOPOLOGY_HOME/bin/gatekeeper`, linked `CLAUDE.md`, installed the pre-commit copy, and ended
  with the manifest — binary, `CLAUDE.md`, chmod summary, and `.git/hooks/pre-commit` as absolute
  paths — followed by `gatekeeper doctor`: `doctor: all probes ok`.
- A second run from inside that checkout used it directly (no clone) and printed the same manifest.

*Honest scope note:* the fresh-clone leg clones GitHub `main`, which predates this branch, so it
cannot exercise the new fetch helper until merged; the curl one-liner against the real release is
the post-merge, post-tag live check. The clone/update branching itself is exercised above.

## AC-6 — Versions agree

`gatekeeper --version` → `gatekeeper 0.2.0 (rules schema v1)`; `0.1.0` absent from Cargo.toml,
plugin.json, marketplace.json.

## AC-7 — Quality gates

`just check` green on the branch: cargo fmt/clippy/tests (all suites pass, including the 44
pre-commit scanner tests), shellcheck over `hooks/*.sh scripts/*.sh`, typos, and
`gatekeeper check docs` (ADR-0011 linked from the ADR index; the ROADMAP verify token resolves to
this file).

## Additional finding fixed during review

`doctor`'s static `resolution split:` line still described the pre-ADR-0011 order; updated to name
the prebuilt `bin/` tiers (its integration test pins only the line's prefix, confirmed before the
change).
