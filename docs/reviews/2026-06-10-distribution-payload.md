VERDICT: pass
HEAD: f1afc4d0cb640d20b9bc7a584a6235b620bc92d9
BASE: a617311271fedf3ed6af847125c3d4d1a1423dfe

# Review: distribution-payload (2026-06-10)

Fresh-context adversarial review of the full branch (a617311..f1afc4d), including round 1
(0782103, 10c2791, 1f9265b) and round 2 (4865ae3, 21112a5, aa84861, f1afc4d). The previous review
at HEAD 1f9265b failed on two blockers; both reproductions were re-run from scratch against the
release binary built at this HEAD — nothing below is taken on trust from the fix commits or their
messages. Fixtures: scratch marked-root git repos and scratch governed git projects under
/private/tmp (canonical paths), payload built by `scripts/build-payload.sh` and unpacked
standalone, hooks exercised through the REAL wrapper scripts with `CLAUDE_PLUGIN_ROOT` /
`GATEKEEPER_BIN` / `TOPOLOGY_ROOT` on the command.

## Blocking findings

None.

## Non-blocking notes

- **Cross-context framework-handoff hook coverage narrowed (new in this branch's design, judged
  non-blocking):** artifact entries (`protected_artifact_paths = ["memory"]`) anchor to the
  SESSION's artifacts root, so from a foreign cwd (e.g. a governed project) an absolute-path Write
  into the FRAMEWORK's own `docs/memory/` is not asked (reproduced: `scan --check-path
  <fw>/docs/memory/x.handoff.md` with cwd outside and `TOPOLOGY_ROOT=<fw>` → exit 0; the
  framework-anchored `security/rules.toml` → exit 1). At base, the framework-anchored
  `memory/artifacts` entry covered the old handoff dir from any cwd. Mitigations that keep this
  non-blocking: the payload ships no `docs/` at all; the staged lane still blocks committing a
  tampered framework handoff (reproduced); the Bash tamper rule still denies literal redirects
  into `docs/memory/`; and the round-2 doc claims (memory/README.md, rules.toml comments) are
  per-context and accurate — they do not promise cross-context coverage.
- **learn-capture.sh never checks `$ROOT/bin/gatekeeper`:** resolution is PATH → repo
  release/debug builds, but install.sh/fetch-gatekeeper place the prebuilt at `$ROOT/bin/`
  (security-scan.sh checks it; learn-capture.sh does not). In a payload install with no
  `gatekeeper` on PATH the optional Stop-hook capture silently degrades to "not built". Lines are
  identical at the merge-base (pre-existing), but the branch newly routes governed ledger
  anchoring through this wrapper, so it is worth a follow-up.
- **test-fetch-version Piece 1 decoy still decorative (carried, half-fixed):** aa84861 removed the
  dead `run_fetch_capture_stderr` helper and replaced the symlink with a `cp`, but Piece 1 still
  invokes `bash "$FETCH_SCRIPT"` (the repo copy), so the fake root's `VERSION = 9.9.9` is never
  read — with no VERSION at the repo root, Piece 1 actually tests env-over-Cargo.toml, not
  env-over-VERSION-file as its comment claims. Pieces 2–3 run the copied script and are sound.
- **Payload AGENTS.md audience (carried, half-fixed):** the spec manifest now documents AGENTS.md
  as the `is_marked_root` sentinel (round-2 spec hunk — accurate), but the shipped file remains
  the framework repo's contributor instructions, partially wrong audience at a payload root.
- **Carried, still true (recorded in the committed handoff doc):** `Integrity` is
  `#[serde(deny_unknown_fields)]` so pre-branch binaries fail closed on the new
  `protected_artifact_paths` key with `rules_schema` still 1 (bricked-session UX, safe direction);
  `build-payload.sh` `cp -R`s `skills/` and `instincts/` from the working tree (strays would ship
  on a dirty checkout; clean on CI); `scan --staged` runs `git -C <framework root>` so a governed
  project's index is never scanned (governed pre-commit is Phase 8 by spec); protected-path
  matching is lexical and does not follow symlinks (pre-existing design).

## Criteria checked

### Prior blocker 1 (staged-lane inversion) — FIXED, reproduced both directions
Scratch marked-root repo (skills/, AGENTS.md, this branch's security/rules.toml; `git init`,
initial commit). Staged tamper of `docs/memory/x.handoff.md` → `scan --staged` printed
`BLOCK protected-path: staged change (A) to docs/memory/x.handoff.md`, exit 1 (was exit 0 at
1f9265b). Staged change to `memory/README.md` only → exit 0 (was spuriously blocked). The fix
(4865ae3) separates the target base from the per-set entry anchors: `is_protected_any` now takes
an explicit `target_base`, the staged lane passes the repo root (git paths are repo-root-relative —
and `git_raw` runs `git -C <root>`, so this holds regardless of process cwd), and only the entries
resolve against `artifacts_root`. `scan --check-path` relative vs absolute now agree (both exit 1
for `docs/memory/x.handoff.md` and `security/rules.toml` from the repo root). Alias probes:
`docs/../docs/memory/x.handoff.md` and `../<repo-dir>/docs/memory/x.handoff.md` both exit 1
(lexical collapse holds). New unit tests pin exactly the previously-masked shapes: a real staged
lane through `scan_staged` with relative git paths (`staged_handoff_in_docs_memory_is_blocked`,
`staged_memory_readme_in_framework_root_not_blocked`) and the target_base/anchor split in
`is_protected_tests` — the masking gap the previous review called out is closed.

### Prior blocker 2 (governed hook bypass through the real wrapper) — FIXED, reproduced
The fix (21112a5) removes `cd "$ROOT"` from hooks/security-scan.sh and hooks/learn-capture.sh;
the binary now runs from the session cwd and the framework root travels via
`TOPOLOGY_ROOT="${TOPOLOGY_ROOT:-$ROOT}"`. Reproduced the previous review's exact bypass setup:
payload built and unpacked at a standalone root, scratch governed git project, cwd = project,
the payload's own hooks/security-scan.sh invoked with `CLAUDE_PLUGIN_ROOT=<payload>`
`GATEKEEPER_BIN=<release binary>`:
- Write event with the ABSOLUTE path `<project>/.claude/topology/memory/x.handoff.md` →
  permissionDecision **"ask"** (was silent allow at 1f9265b).
- Write event with the RELATIVE spelling `.claude/topology/memory/x.handoff.md` → **"ask"**
  (relative hook paths now resolve against the session cwd; was unprotected).
- Ordinary project file → silent allow, exit 0.
- Evil sibling `<project>/.claude/topology/memory-evil/x.md` → silent allow (no over-match).
Learn anchoring: from project cwd with `TOPOLOGY_ROOT=<payload>`, `learn capture` wrote
`<project>/.claude/topology/learn/ledger.md`; `find <payload> -newer <tarball>` returned nothing
and `<payload>/docs` does not exist (the ADR-0013 data-loss path is closed). Same result through
the REAL hooks/learn-capture.sh wrapper (`TOPOLOGY_GOTCHA` set, binary on PATH): second ledger
entry appended to the project ledger, nothing under the payload.

### Prior round-1 blocker (first governed handoff metadata) — still FIXED
Fresh scratch git project on branch `feature-xyz`, payload as framework via `TOPOLOGY_ROOT`,
FIRST `memory write` with `.claude/topology/` absent: frontmatter recorded `branch: feature-xyz`
and the full 40-hex `head_sha` (reproduced).

### Round-2 diff (1f9265b..HEAD), hunk by hunk
- scan.rs target-base refactor: read in full; `cwd` computed once in `cmd_scan`
  (`current_dir().unwrap_or(root)`), threaded to hook/check-path lanes; staged lane passes `root`
  (correct: `git_raw` is `git -C root`, paths repo-relative regardless of cwd); `is_protected`
  kept as a test-only wrapper (`cfg_attr(not(test), allow(dead_code))`). Probed for new edge
  cases: relative check-path from a cwd that differs from any root resolves honestly against the
  cwd (relative and absolute spellings of the same file agree — both exit 0 from the foreign cwd,
  both exit 1 from the repo root); `..` aliases collapse and still match; evil siblings do not.
  The one semantic narrowing found is the cross-context note above.
- Hook wrappers: `TOPOLOGY_ROOT="${TOPOLOGY_ROOT:-$ROOT}"` honors a pre-set env, defaults to
  `CLAUDE_PLUGIN_ROOT`/hook-dir parent; cwd contract documented in both headers and verified
  empirically through both wrappers (above). hooks/pre-commit.sh deliberately keeps `cd "$ROOT"`
  for the staged lane — consistent with `git -C root`. shellcheck clean.
- Docs: memory/README.md and TEMPLATE.handoff.md now describe `protected_artifact_paths`
  resolved against the artifacts root — matches the code at HEAD; spec manifest gained the
  AGENTS.md row — matches the tarball (listed it). Committed handoff doc's lane numbers match my
  runs (257 tests), and its deferred-notes list matches what I independently re-verified.
- test-fetch-version.sh: dead helper gone; Piece-1 inaccuracy noted above; 3/3 pass.

### Spec/plan
Acceptance criteria re-verified at HEAD:
- **AC-1** (release carries tarball + SHA256SUMS entry): read .github/workflows/release.yml —
  `payload` job (needs version-guard) builds and uploads `topology-payload.tar.gz`; release job
  runs `sha256sum gatekeeper-* topology-payload.tar.gz > SHA256SUMS` and lists both in the
  release files. Unchanged in round 2; cannot exercise a real tag from here.
- **AC-2** (unpack + fetch yields working tree): ran `bash scripts/test-payload-e2e.sh` — 10
  passed, 0 failed (`--version`, `activate`, `scan --cmd` veto, doctor VERSION probe, all with
  `TOPOLOGY_ROOT` at the unpacked tree and the shipped fetch script over `file://`).
- **AC-3** (no `*.rs`/`docs/`/`.git`/plugin files): ran `bash scripts/test-build-payload.sh` —
  26 passed, 0 failed; independently built my own tarball and grepped the listing for
  `\.rs$|^docs/|\.git|plugin` — no hits.
- **AC-4** (VERSION parses both ways, matches tag): `bash scripts/test-fetch-version.sh` 3/3
  (bash grep consumer); doctor probe in e2e printed `VERSION: payload 0.3.0 (rules schema v1)`
  (toml-crate consumer); CI version-guard pins tag == Cargo.toml.
- **AC-5** (governed paths + promote refusal): reproduced all three in the scratch governed
  fixture — `memory write` → `.claude/topology/memory/first-test.handoff.md`, `learn capture` →
  `.claude/topology/learn/ledger.md` (direct AND via the real Stop hook), `learn promote --yes` →
  exit 2 with the ledger path, fork pointer, and ADR-0013 in the message.
- **AC-6** (framework repo paths): in the equal-roots scratch marked-root repo — `memory write` →
  `docs/memory/eqroots.handoff.md`, `learn capture` → `docs/learn/ledger.md`.
- **AC-7** (cargo test coverage): 257 passed / 2 ignored / 0 failed, now including the
  staged-lane regression tests with relative git-emitted paths and the target_base/anchor-split
  unit tests — the production invocation shapes that both previous masking gaps missed.

### Standards
ADR-0013 and repo conventions:
- §1 payload read-only at runtime: holds through BOTH hook wrappers now (find -newer over the
  payload after capture: empty) and for direct governed memory/learn invocations.
- §2/§3 anchoring + promote refusal: verified above.
- Plan test conventions (scratch git + scratch framework, `TOPOLOGY_ROOT` on the command, no
  `env::set_var`): followed by the new round-2 tests (scan.rs staged fixtures build their own
  marked-root git repos; cwd never mutated — target_base injected explicitly).

### Quality lanes (all run by this reviewer at HEAD f1afc4d)
- `cargo fmt --check` clean; `cargo clippy --all-targets -- -D warnings` clean.
- `cargo test --release` 257 passed, 2 ignored, 0 failed (9 suites).
- `shellcheck hooks/*.sh scripts/*.sh` clean; `typos` clean.
- `bash scripts/test-build-payload.sh` 26/26; `bash scripts/test-fetch-version.sh` 3/3;
  `bash scripts/test-payload-e2e.sh` 10/10.
- `./gatekeeper/target/release/gatekeeper check docs` → `check docs: ok`.
