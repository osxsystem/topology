# Changelog

Earlier releases (≤ v0.4.0) predate this file; see the GitHub releases page for their artifacts.

## v0.9.0 — 2026-06-12

`adapt` v2: project integration (ROADMAP Phase 9). One `gatekeeper adapt` run in a governed project
now delivers everything the project needs — append-only, idempotent, never clobbering user content.

### Project integration (project installs: framework root ≠ project root)

- **Scaffold** `.claude/topology/{research,specs,plans,verify,reviews}/` (with `.gitkeep`) for the
  gate artifacts.
- **`GATEKEEPER_BIN` wiring** — `.claude/settings.json` is now **merged**, not regenerated: the hook
  wiring and `env.GATEKEEPER_BIN = <framework>/bin/gatekeeper` are set while every other top-level
  and `env` key the user added is preserved. `gatekeeper check …` resolves with no PATH/sudo step.
- **Contract delivery, append-only / import-first** — the operating contract is rendered to
  `.topology/CONTRACT.md`; for `--harness claude` an `@.topology/CONTRACT.md` import line is appended
  to `CLAUDE.md` (created with just that line if missing); for `--harness codex` a marker-delimited
  **managed block** is upserted in `AGENTS.md` (re-runs update it in place; malformed markers fail
  closed). User content in those files is never rewritten.
- **`--check`** reports drift (exit 1) without writing for all of the above — a missing import line,
  an out-of-date managed block, or absent scaffold/contract.

### Internals

- Two pure partial-file primitives in `adapt.rs` (`ensure_import_line`, `ensure_managed_block`) plus
  `merge_claude_settings`, all unit-tested; whole-file generation (`GenFile`/`apply_or_check`) is
  unchanged for adapt-owned files.
- The portable contract template gains a gate-shaped **first-session instruction** (bare project
  stub → write the project's docs above the import, then proceed); `AGENTS.md` regenerated from it.

### Carried to a follow-up (Phase 9.1)

- cursor/opencode contract delivery via their native always-on surface (they already carry the
  contract through generated rules), and the opt-in `adapt --init-agent` bootstrap.

## v0.8.0 — 2026-06-12

Contract split: portable template + generated AGENTS.md + skill wording sweep (ROADMAP Phase 10).

### Contract template

- `templates/CONTRACT.template.md`: the six portable sections of the operating contract
  (`AGENTS.md`) parameterized over `{{ARTIFACTS_ROOT}}`, `{{GATEKEEPER_CMD}}`, `{{BINARY_NOTE}}`.
  Shipped in the distribution payload.
- `render_contract` in `adapt.rs`: pure substitution, fail-closed — any unresolved `{{` after
  substitution is a hard error naming the offending placeholder (ADR-0016).
- `gatekeeper adapt --contract <framework|project>`: renders the template and prints to stdout
  (exit 0); render error → stderr message naming the placeholder, exit 2.

### Framework dogfooding

- `AGENTS.md` is now generated: `render_contract(template, framework ctx)` + a short trailer
  pointing at `docs/DEVELOPMENT.md`. An integration test asserts byte-equality — hand-edits or
  template drift break `just check`.
- `docs/DEVELOPMENT.md` (new): carries the two framework-dev sections (stack conventions, skill
  house format) under a one-line audience preamble. Never shipped in the payload.

### Skill wording sweep

Seven skills and the plan template switch from hardcoded `docs/<kind>/` paths to
`<artifacts-root>/<kind>/` phrasing with one definition parenthetical at first use per skill.
Instincts were already clean. `skill-rules.json` untouched; no names, descriptions, or behaviors
changed.

### ADR

ADR-0016 records the template, placeholder set, fail-closed render, generated AGENTS.md, dev-doc
location, and the Phase 9 delivery boundary.

## v0.7.0 — 2026-06-12

Global payload install + plugin channel retirement (ROADMAP Phase 8).

### Global scope consumes the payload

- `--global` now uses the release payload (tarball download + checksum verification) instead of
  git clone/pull. Both piped and checkout modes are supported, mirroring `--project` behaviour.
- Checkout mode assembles the payload via `build-payload.sh` into `TOPOLOGY_HOME`; the checkout
  is not used as `ROOT` itself. Dev self-governance (resolution step 2, Phase 11) is unaffected.
- `--build-from-source` global: checkout → builds binary into `$ROOT/bin`; piped → early failure
  with remedy (same rule as local scope).
- Global legacy-clone rescue: in-tree ledger and handoffs are copied to a timestamped sibling
  backup `${ROOT}-backup-<YYYYmmdd-HHMMSS>/` before deletion (no silent data loss).
- `git clone`/`git pull` global path removed.

### Plugin channel retirement

- `.claude-plugin/plugin.json`, `.claude-plugin/marketplace.json`, `hooks/ensure-gatekeeper.sh`,
  and `hooks/hooks.json` deleted.
- `release.yml` version guard reduced to tag == `gatekeeper/Cargo.toml` only (the two JSON probes
  are gone — without this change the next tag would fail on the deleted files).

### ROOT_MARKERS update

- `ROOT_MARKERS`: `["AGENTS.md", "gatekeeper", ".claude-plugin"]` → `["AGENTS.md", "gatekeeper"]`.
  A directory with only `skills/` + `.claude-plugin/` is no longer a marked root, preventing any
  unrelated Claude Code plugin checkout from claiming Topology self-governance.
- Doctor F1 message updated to name the two remaining markers.

### Install script cleanup

- `sudo ln -sf … /usr/local/bin/gatekeeper` post-install suggestion removed (superseded by
  `GATEKEEPER_BIN` wiring in Phase 9).
- Shared payload helpers (`_unpack_payload`, `_handle_existing_root`, etc.) hoisted so both scopes
  use the same download/verify/unpack machinery — no duplication.

### CI wiring

- New offline `installer` CI job in `ci.yml` running `just test-payload`, `just test-fetch`,
  `just test-e2e` — installer test suites now gate merges (they were previously not in CI).

## v0.6.0 — 2026-06-12

Root-resolution hardening + doctor provenance (ROADMAP Phase 11).

### Root resolution — kills the cwd-ancestor hijack class

- `resolve_root` is now a **pure function** over four explicit inputs (`start`, `env_override`,
  `exe_path`, `home`) — no process state inside the resolution logic.
- The bare cwd marker walk and the per-ancestor `.topology` probe (W1/W2/W3 from research) are
  **removed**. The new precedence chain:
  1. `$TOPOLOGY_ROOT` — explicit pin, returned verbatim.
  2. Self-governed project — nearest `.git` ancestor that is itself a marked root.
  3. Binary-adjacent — walks up from the binary's own path; handles both `bin/` installs
     and `gatekeeper/target/<profile>/` dev builds.
  4. `<project>/.topology` — vendored install at the project root.
  5. `~/.topology` — global install, by real path (fixes W2: projects outside `$HOME`).
  6. Fallback — cwd unchanged; `framework_root()` prints one stderr warning.
- `RootSource` enum + `ResolvedRoot` struct carry provenance through the call chain.
- Any ancestor directory that happens to have `skills/` + a marker no longer silently wins
  over an installed payload or the global install.

### Doctor — root provenance and new failures

- Prints `resolved by: <step>` immediately after `framework root:`.
- **F1 (FAIL, non-zero):** resolved root is not a marked Topology root (fallback landed on a
  plain directory, or a pinned root lost its markers).
- **F2 (FAIL, non-zero):** `cwd` is inside a payload install (`project == framework` and
  `VERSION` present) — user is running from the payload directory instead of their project.
- Dev checkout (`project == framework`, no `VERSION`) remains exit 0 (self-governance mode).
- Existing `version_skew` FAIL unchanged.

### Tests

- 5 new integration fixtures in `gatekeeper/tests/cli_root_resolution.rs`:
  (a) hijack-class ancestor no longer wins; (b) W2 global-home resolution;
  (c) binary-adjacent `bin/` layout; (d) doctor F1; (e) doctor F2.
- All integration test helpers updated to pin `TOPOLOGY_ROOT` to canonicalized cwd so
  binary-adjacent does not silently resolve to the actual topology repo in scratch-root tests.

## v0.5.1 — 2026-06-12

Shadow-verdict burn-in sink

- Each `emit_shadow` call now appends a JSON line to `<artifacts root>/logs/shadow.jsonl`
  (framework repo: `docs/logs/`, gitignored; governed project: `.claude/topology/logs/`) so
  the per-gate false-block rate is measurable across burn-in runs.
- `scripts/shadow-stats.sh` prints a per-(gate,check) evaluation table, lists every
  would-block verdict for human triage, and reminds of the flip criterion (≥50 evaluations,
  <2% human-triaged false-block rate).
- Stderr contract unchanged: the `SHADOW …` line is byte-identical to v0.5.0; only the
  file line gains a leading `ts` field. Sink is fail-silent — gates never block on I/O errors.

## v0.5.0 — 2026-06-12

Hollow-pass kills + drift-proof CLI surface
([spec](docs/specs/2026-06-11-hollow-pass-kills.md), ROADMAP Phase 14).

### Dispatch table + ADR-0014 (FM3)

- Replaced the hand-rolled `match` block and all nine `USAGE_*` constants in
  `gatekeeper/src/main.rs` with a static `SUBCOMMANDS` dispatch table
  (`grep -c 'const USAGE' gatekeeper/src/main.rs` → 0).
- Longest-prefix match: two-word keys (e.g. `"check verify"`) win over single-word
  prefixes. Group-level `check` behavior preserved exactly.
- Recorded as **ADR-0014 "dispatch table over clap"** (four-dep constraint, ADR-0007).

### CLI / doc sync safeguard (FM3)

- `gatekeeper/tests/cli_doc_sync.rs`: spawns `gatekeeper --help`, extracts
  in-scope `gatekeeper …` backtick spans from `## Command reference` in
  `docs/USER-GUIDE.md` and the gate table in `README.md`, and asserts bidirectional
  coverage with no ghost commands and no flag-spelling mismatches.
- Wired into `ci.yml` (`gate` job) and `release.yml` (`version-guard`) as
  `cargo test --manifest-path gatekeeper/Cargo.toml --test cli_doc_sync` — the v0.4.0
  doc-drift class dies at the tag.

### Verify gate — evidence replay (FM2, shadow)

- New fenced `evidence` block format in verify artifacts: `$ command` steps with
  optional `# expect:` / `# expect-re:` directives.
- `mode = "replay"` enforces fail-closed: zero blocks = fail; malformed directive = fail;
  metachar / env-assignment / non-allowlisted / timed-out commands = fail.
- Default (`mode = "presence"`): static analysis only — no commands ever executed.
  Results emitted as `SHADOW` JSONL lines on stderr (`result: "static"`).
- `GATEKEEPER_SHADOW=replay` env var triggers actual execution for baseline measurement
  without changing the exit code. No-op when `mode = "replay"` is already enforced.
- Config: `[verify]` table — `mode`, `replay_timeout_secs` (default 300),
  `allowed_command_prefixes` (token-boundary, read-only git defaults).
- Kills hollow fixture **(b)** (empty verify file).

### Design gate — substance floor + human-commit approval (FM2, shadow)

- `[design] substance_floor = true`: spec must have ≥ 2 `## ` headings and ≥ 1 body
  line. Shadow-computed when off. Kills fixture **(a)** (approval-marker-only spec).
- `[design] approval = "human-commit"`: traces the approval line through
  `git log -L` to its authoring commit; fails if any `Co-Authored-By:` value matches
  `agent_trailer_patterns`. Requires git ≥ 2.15, non-shallow clone, committed and
  clean spec. Obstacles fail closed when enforced, log `skip` in shadow.
- Default `agent_trailer_patterns` covers Claude, Copilot, Cursor, Codex, Gemini,
  Devin, Aider, `[bot]`.
- Negative dogfood: the spec's own approval commit (agent-executed at recorded
  maintainer direction) fails `human-commit` mode — demonstrating the check catches
  exactly the delegated-approval practice it exists to reject.

### Finish gate — zero-test floor (FM2, shadow)

- `[finish] require_test_count = true`: finish gate fails when the test command
  produces no recognised runner summary or a recognised zero-test summary.
- Applies to both `test_command` (config) and `-- <cmd>` (CLI) invocations.
- Built-in patterns (first-match-wins): cargo (`test result: ok. N passed`), pytest
  (`N passed … in Xs`, `pytest -q`-compatible, no `===` anchor).
- `extra_count_patterns`: escape hatch for custom runners (one capture group each).
  Go and jest count support deferred to a later release; `extra_count_patterns` is the
  workaround.
- Shadow-computed when off. Kills fixtures **(e)** (unrecognised summary) and **(g)**
  (recognised zero-test summary).

### Hollow fixture scoreboard

Seven adversarial fixtures define the FM2 track. Four killed this release:

| # | Fixture | Killed by | Status |
|---|---------|-----------|--------|
| a | spec containing only `Status: approved` | design substance floor | killed |
| b | empty verify file | verify evidence replay | killed |
| c | `assert!(true)` test-only commit | Phase 15 red-green replay | `#[ignore]` |
| d | review body "Looks fine." | Phase 17 judge | `#[ignore]` |
| e | `test_command = "true"` (no recognisable summary) | finish zero-test floor | killed |
| f | plan dodging the denylist with synonyms | Phase 17 judge | `#[ignore]` |
| g | runner emitting zero-test summary | finish zero-test floor | killed |

### USER-GUIDE additions (AC-9)

- Hardened-gate config tables: `[verify]`, `[design]`, `[finish]` with types, defaults,
  and one-line meanings.
- Evidence grammar: block format, directive rules, execution model, metachar/allowlist
  rejections, read-only/idempotent requirement, output capture and 1 MiB tail-cap.
- SHADOW JSONL schema: all seven fields (`gate`, `check`, `configured`, `artifact`,
  `command`, `result`, `detail`) with valid values; per-check semantics table.
- `GATEKEEPER_SHADOW=replay` semantics; documented `jq` aggregation procedure.
- Deferred-Go note for the finish floor.

### Also in this release (merged to main after v0.4.1, attributed separately)

- `just setup` now installs the pre-commit hook in the framework clone;
  `gatekeeper doctor` probes for it and reports the result (fix for issue #38,
  merged to main via PR #39 — not part of Phase 14).

## v0.4.1 — 2026-06-11

Payload-only patch: scan-rule additions and their regression harness. No gatekeeper code changes
([spec](docs/specs/2026-06-11-day-zero-containment.md), ROADMAP Phase 13).

### Security rules (`security/rules.toml`)

- New `jwt-structural` rule (**block**): three dot-joined base64url segments with the `eyJ`
  sigil — catches bearer JWTs regardless of labeling.
- Broadened `openai-key` (**block**): now tolerates hyphenated segment prefixes and
  underscore-bearing tails (`sk-proj-…`, `sk-ant-…`); rule id unchanged so existing allowlists
  stay valid.
- New `labeled-secret-assignment` rule (**warn**): credential-labeled assignments
  (api key / auth token / password / bearer followed by a 16+-char value). Warn posture is
  deliberate — promotion to block is a Phase 2 decision made on bench data.

### Tests

- Secrets benchmark (`gatekeeper/tests/cli_scan_bench.rs` + `tests/fixtures/secrets-bench/`):
  eleven positive classes assembled at runtime (no secret-shaped literals in the tree), six
  literal negative fixtures. Asserts the 9/11 in-scope detection floor with rule-id attribution
  and zero false positives on the negatives — the standing regression wall for every future rule.

### Ops & docs

- GitHub push protection enabled on the repository (host-side backstop); bypass flow documented
  in the user guide's Security scanning section.
- Process-weight baseline recorded in `docs/research/2026-06-11-process-weight-baseline.md`
  via the new `scripts/metrics.sh` — the FM1 denominator for the remediation track.
- Ships the `check` usage-text fix that landed on `main` just after the v0.4.0 tag.
