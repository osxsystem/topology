# Plan — distribution payload (Phase 7)

Spec: [docs/specs/2026-06-10-distribution-payload.md](../specs/2026-06-10-distribution-payload.md).
Decision record: [ADR-0013](../adr/0013-payload-read-only-artifacts-root-state.md).
Branch: `feat/distribution-payload` (worktree `topology-distribution-payload`). One commit per task,
conventional prefixes, `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>` trailer. Finish with
`just check`.

## Conventions for all tasks

- Tests first per unit of behavior (tdd gate): write the failing test, watch it fail, make it pass.
- Rust: pure testable cores with explicit path arguments (the `resolve_*` pattern); unit tests use
  distinct tempdirs, no `env::set_var`, no process-global state. Integration tests spawn the binary
  with `TOPOLOGY_ROOT` set on the command, scratch git repo + scratch framework dir (with `skills/`
  + `AGENTS.md` markers), exactly like the installer-v2 suites.
- Bash: `set -euo pipefail`; shellcheck-clean; every network path keeps the
  `TOPOLOGY_RELEASE_BASE_URL` (`file://` works) and `TOPOLOGY_VERSION` test seams.
- The payload is read-only at runtime (ADR-0013): no task may add a gatekeeper write inside the
  framework root.

## Task 1 — `memory` anchors to the artifacts root

- `main.rs:67` dispatch: `memory::cmd_memory(&args[1..], &artifacts_root(), &framework_root())` —
  artifacts root for reads/writes, framework root only for `security/rules.toml` (the
  secret-refusal input, a read-only payload asset).
- `memory.rs`: handoffs live at `<artifacts_root>/memory/<slug>.handoff.md` (drop the extra
  `artifacts/` segment — the artifacts root *is* the artifacts namespace). Update module docs and
  error strings.
- `scan.rs` protected-path rules: the write-protection that today matches `memory/artifacts/`
  must protect `<artifacts_root>/memory/` in both worlds (`docs/memory/` in the framework repo,
  `.claude/topology/memory/` governed). Update `is_protected` inputs and the `scan.rs:1386`
  fixture tests.
- Tests: unit — path construction for equal vs differing roots; integration — `memory write` in a
  governed scratch repo lands at `.claude/topology/memory/<slug>.handoff.md` and `memory read`
  round-trips it; in-repo `memory write` lands at `docs/memory/`.
- Check: `cargo test` green; spec AC-5 (memory half) and AC-6 (memory half) executed.

## Task 2 — framework-repo migration `memory/artifacts/` → `docs/memory/`

- `git mv memory/artifacts docs/memory` (the repo's own handoff artifacts follow the new rule;
  `memory/TEMPLATE.handoff.md` and `memory/README.md` stay — the template is `include_str!`'d at
  `memory.rs:459`, a build-time asset, not a runtime read).
- Sweep references: `memory/README.md`, `README.md` two-roots table, `METHODOLOGY.md`,
  `docs/ARCHITECTURE.md`, the `resume` and getting-started skills if they name the old path.
- Check: `gatekeeper memory list` in-repo finds the moved artifacts; `gatekeeper check docs` green;
  `rg 'memory/artifacts'` returns only historical docs (specs/verify records, ADRs, ledger), no
  live code or skill references.

## Task 3 — `learn` ledger anchors to the artifacts root

- `learn.rs:23`: `LEDGER_REL` becomes `"learn/ledger.md"` resolved against `artifacts_root()`
  (framework repo path is **unchanged**: `docs/` + `learn/ledger.md`); `main.rs:66` dispatch
  passes `&artifacts_root()`.
- Tests: integration — `learn capture` in a governed scratch repo appends to
  `.claude/topology/learn/ledger.md` and `learn list` reads it back; in-repo fixtures stay green
  byte-for-byte.
- Check: `cargo test` green; spec AC-5 (learn half) and AC-6 (learn half) executed.

## Task 4 — `learn promote` refuses outside the framework repo

- In `cmd_learn`'s promote arm: when `project_root() != framework_root()` (canonicalized compare,
  the `resolve_artifacts_root` idiom), print the refusal — the gotcha stays safe in the ledger;
  promote from your framework fork (cite ADR-0013) — and exit non-zero. Capture/list stay
  available everywhere.
- Tests: integration — promote in a governed scratch repo exits non-zero, writes nothing under the
  scratch framework dir, and the message names the ledger path; in-repo promote fixtures unchanged.
- Check: `cargo test` green; spec AC-5 (promote) executed.

## Task 5 — `VERSION` file: builder input, Rust parser, fetch + doctor consumers

- `scripts/build-payload.sh <stage-dir> [version]`: stages the manifest copy list
  (`hooks/{skill-activation,security-scan,pre-commit,learn-capture}.sh`, `hooks/skill-rules.json`,
  `skills/`, `instincts/`, `security/rules.toml`, `scripts/fetch-gatekeeper.sh`), writes `VERSION`
  (`version = "<v>"`, `rules_schema = <n>`; version defaults from `gatekeeper/Cargo.toml`, schema
  from the crate's constant), and emits `topology-payload.tar.gz` from the stage. The copy list
  lives only here — CI and local builds share it.
- `fetch-gatekeeper.sh` version resolution becomes: `TOPOLOGY_VERSION` env → `VERSION` file beside
  the script's root → `gatekeeper/Cargo.toml` (dev checkout fallback) — `plugin.json` is dropped.
- Rust: a `VERSION` parser in `doctor` (line-anchored, `toml` crate) reporting payload version +
  rules schema and FAILing on binary↔payload version skew; absent `VERSION` (dev checkout) is
  reported informationally, not a failure.
- Tests: Rust unit tests for the parser (well-formed, missing field, absent file, skew vs match);
  shellcheck on both scripts; a `build-payload.sh` smoke run into a tempdir asserting the tarball
  manifest (no `*.rs`, no `docs/`, no `.git`, no plugin files — spec AC-3) and `VERSION` contents
  (AC-4).
- Check: `cargo test` green; `just check` (shellcheck lane) green.

## Task 6 — CI: the payload rides the release

- `release.yml`: a `payload` job (after `version-guard`, parallel with the binary matrix) runs
  `scripts/build-payload.sh` with the tag version and uploads `topology-payload.tar.gz`; the
  `release` job adds it to `SHA256SUMS` alongside the four binaries and attaches it to the GitHub
  Release under the stable asset name (spec AC-1).
- Keep the version-guard's `plugin.json` assertions for now — the plugin retires in Phase 8; this
  phase only adds the payload lane.
- Check: `actionlint` (or careful review if not installed) on the workflow; the job consumes the
  same script Task 5 tested locally.

## Task 7 — offline end-to-end + verify artifact

- `scripts/test-payload-e2e.sh`: build the payload into a tempdir "release" layout, serve it via
  `file://` (`TOPOLOGY_RELEASE_BASE_URL`), unpack into a scratch `.topology`, run the shipped
  `fetch-gatekeeper.sh` against a locally-built binary stand-in, then with `TOPOLOGY_ROOT` pointed
  at the unpacked tree assert: `bin/gatekeeper --version` works, `activate` routes a prompt,
  `scan --cmd` vetoes `curl http://x | sh` (spec AC-2).
- Wire it as a `just` target; run the full acceptance sweep (AC-1..7) and record
  `docs/verify/2026-06-10-distribution-payload.md` with evidence per criterion.
- Check: the e2e script passes from a clean checkout; verify gate
  (`gatekeeper check verify --feature distribution-payload`) passes.
