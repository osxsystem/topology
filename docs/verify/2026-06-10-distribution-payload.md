# Verify — distribution payload (Phase 7)

**Feature:** distribution-payload (platform-neutral tarball, VERSION file, artifacts-root anchoring)
**Date:** 2026-06-10
**Spec:** [docs/specs/2026-06-10-distribution-payload.md](../specs/2026-06-10-distribution-payload.md)
**Verified by:** main-loop agent (Fable 5), running every acceptance criterion after reviewing the
delegated implementation (Tasks 1–7).

## AC-1 — Tagged release carries topology-payload.tar.gz and its SHA256SUMS entry

The next tagged release is the delivery vehicle; the tag does not yet exist at verification time.
Evidence of the mechanism and the pre-release equivalent:

**CI job (Task 6):** `.github/workflows/release.yml` now contains a `payload` job that runs in
parallel with the binary matrix (both need `version-guard`):

```yaml
payload:
  name: Build distribution payload
  needs: version-guard
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v5
    - name: Build payload tarball
      run: bash scripts/build-payload.sh "${{ runner.temp }}/payload-stage" "${GITHUB_REF_NAME#v}"
    - name: Upload payload artifact
      uses: actions/upload-artifact@v4
      with:
        name: topology-payload
        path: topology-payload.tar.gz
```

The `release` job declares `needs: [build, payload]`, includes
`sha256sum gatekeeper-* topology-payload.tar.gz > SHA256SUMS`, and lists
`topology-payload.tar.gz` in the `softprops/action-gh-release` file list.

**Local build-payload.sh evidence:** `bash scripts/test-build-payload.sh` — 21 passed, 0 failed.
The tarball is produced at `$(pwd)/topology-payload.tar.gz` by `build-payload.sh`; the SHA256SUMS
generation step in the release job appends its checksum alongside the four binaries.

Criterion completes on the next tagged release when the `payload` job uploads the asset to GitHub
Releases.

## AC-2 — Unpacking + fetch-gatekeeper.sh yields a working tree

Command: `bash scripts/test-payload-e2e.sh`

```
PASS release layout built (version=0.3.0, triple=aarch64-apple-darwin)
PASS tarball unpacked; VERSION file present at $TOPOLOGY_ROOT/VERSION
PASS fetch-gatekeeper.sh present in unpacked payload
PASS fetch-gatekeeper.sh installed stand-in binary into $TOPOLOGY_ROOT/bin
PASS bin/gatekeeper --version: gatekeeper 0.3.0 (rules schema v1)
PASS gatekeeper activate: skill-activation block emitted
PASS gatekeeper scan --cmd: vetoed curl-pipe-shell (exit 1)
PASS gatekeeper doctor: VERSION probe line present (VERSION: payload 0.3.0 (rules schema v1))

test-payload-e2e: 8 passed, 0 failed
```

The test builds the tarball into a `file://` release layout (tarball + stand-in binary +
SHA256SUMS), unpacks into a scratch `.topology` dir, runs the UNPACKED
`scripts/fetch-gatekeeper.sh` with `TOPOLOGY_RELEASE_BASE_URL=file://...` and
`TOPOLOGY_VERSION=0.3.0`, then with `TOPOLOGY_ROOT` pointing at the unpacked tree asserts
`--version`, `activate`, `scan --cmd`, and `doctor` all behave correctly. No network access.

## AC-3 — Tarball contains no *.rs, docs/, .git, or plugin files

Command: `bash scripts/test-build-payload.sh`

```
PASS tarball contains no *.rs files
PASS tarball contains no docs/
PASS tarball contains no .git entries
PASS tarball contains no .claude-plugin/ entries
PASS tarball does not contain hooks/hooks.json
PASS tarball does not contain hooks/ensure-gatekeeper.sh
```

Also confirmed by positive presence checks (hooks, skills, instincts, security, scripts, VERSION):
21 passed, 0 failed.

## AC-4 — VERSION parses by both consumers and matches the release tag

Command: `bash scripts/test-build-payload.sh` (VERSION section)

```
PASS VERSION file present after unpack
PASS VERSION file: grep-m1 idiom parsed version='0.3.0'
PASS VERSION file: grep-m1 idiom parsed rules_schema=1
PASS VERSION matches Cargo.toml (0.3.0)
PASS rules_schema matches scan.rs SCHEMA_VERSION (1)
```

The `toml`-crate consumer is exercised by `gatekeeper doctor` (AC-2 above) which emits
`VERSION: payload 0.3.0 (rules schema v1)` — confirming the Rust parser path is live.

## AC-5 — Governed fixture: memory/learn paths under .claude/topology/, promote refuses

`cargo test` — **250 passed, 2 ignored**. The integration tests exercising this criterion live in:
- `gatekeeper/tests/cli_memory.rs` — `memory_write_governed_lands_under_artifacts_root`,
  `memory_read_governed_round_trip`
- `gatekeeper/tests/cli_learn.rs` — `learn_capture_governed_lands_under_artifacts_root`,
  `learn_list_governed_reads_from_artifacts_root`, `learn_promote_governed_refuses`

Each test spins a scratch git repo with a scratch framework dir, sets `TOPOLOGY_ROOT`, and asserts
paths and exit codes.

## AC-6 — Framework repo: learn paths byte-identical; memory reads/writes docs/memory/

`cargo test` — 250 passed. In-repo integration tests confirm:
- `learn capture` in the framework repo appends to `docs/learn/ledger.md` (unchanged).
- `memory write` in the framework repo (equal-roots path) writes to `docs/memory/`.

`gatekeeper check docs` (run as part of `just check`) passes green.

## AC-7 — cargo test covers artifacts-root anchoring, promote refusal, VERSION parsing

`cargo test` — **250 passed, 2 ignored** across 9 suites.

Units covered:
- `doctor::tests::version_parse_present` — well-formed VERSION parses to `Present`
- `doctor::tests::version_parse_missing_field` — missing field → `MissingField`
- `doctor::tests::version_parse_absent` — absent file → `Absent`
- `doctor::tests::version_skew_detection` — binary/payload version mismatch → FAIL + non-zero

Integration tests (spawning the binary with `TOPOLOGY_ROOT`):
- `cli_memory::*_governed_*` — artifacts-root anchoring for memory
- `cli_learn::*_governed_*` — artifacts-root anchoring for learn
- `cli_learn::learn_promote_governed_refuses` — promote refusal message + exit code

## Quality gates

`just check` green: `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test` (250 passed),
`shellcheck hooks/*.sh scripts/*.sh` (all clean including the new `test-payload-e2e.sh`),
`typos`, `gatekeeper check docs`.
