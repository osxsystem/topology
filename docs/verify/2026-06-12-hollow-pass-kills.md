# Verify — hollow-pass-kills (Phase 14, v0.5.0)

Spec: [2026-06-11-hollow-pass-kills.md](../specs/2026-06-11-hollow-pass-kills.md) (rev 4.1, Status: approved).
Verified on `feat/hollow-pass-kills`, 2026-06-12, macOS arm64, after rebase onto main @ `2fd50a1`.

## Acceptance criteria walk

### AC-1 — hollow scoreboard

`cli_hollow.rs` landed red-first (commit `13a2140`, before any fix). At branch tip:
(a) approved-only spec, (b) empty verify, (e) `test_command = "true"`, (g) zero-test runner —
un-ignored and green. (c) `assert!(true)` red commit, (d) "Looks fine" review, (f) synonym-dodged
plan — `#[ignore]`-tagged naming Phases 15 / 17 / 17.

```evidence
$ cargo test --manifest-path gatekeeper/Cargo.toml --test cli_hollow
# expect: test result: ok. 4 passed; 0 failed; 3 ignored
```

### AC-2 — dispatch table, help contract

`grep -c 'const USAGE' gatekeeper/src/main.rs` → **0**. `cli_help_flags.rs` green **unmodified**
(zero diff vs main for that file across the branch). ADR-0014 committed and linked from
`docs/adr/README.md`.

Help diff vs the released v0.4.1 binary (`gatekeeper-aarch64-apple-darwin`, checksum-verified
release asset) is **8 diff lines, all §2-enumerated item 2 (column padding normalized)** — the
`check plan` / `check tdd` rows lose their alignment padding. Before/after capture:

```text
8c8
<   gatekeeper check plan   --feature <slug>
---
>   gatekeeper check plan --feature <slug>
10c10
<   gatekeeper check tdd    --feature <slug> [--base <ref>]
---
>   gatekeeper check tdd --feature <slug> [--base <ref>]
```

No gate-order or per-gate-help diffs were observed (items 1 and 3 sanctioned but not needed).

```evidence
$ cargo test --manifest-path gatekeeper/Cargo.toml --test cli_help_flags
# expect: test result: ok
```

### AC-3 — doc-sync test, wired + desync demo

`cli_doc_sync.rs` runs in `ci.yml` (`gate` job) and `release.yml` (`version-guard`), both with
`--manifest-path`. Desync demo (2026-06-12): a ghost row
``| `gatekeeper ghost-cmd` | … |`` injected under `## Command reference` →
`Assertion 1 FAILED — … missing from USER-GUIDE` (1 failed); reverted → green. The injected line
never reached a commit (`git diff --stat` empty post-revert).

```evidence
$ cargo test --manifest-path gatekeeper/Cargo.toml --test cli_doc_sync
# expect: test result: ok. 1 passed
```

### AC-4 — replay executes nothing by default

Booby-trap demo (2026-06-12): scratch root, artifact with `$ touch /tmp/boobytrap_executed`,
`allowed_command_prefixes = ["touch"]`, default mode. `check verify` exited 0 with
`result:"static"` (`1 evidence block(s), 1 command(s), all allowlisted: true`) and the trap file
**was not created** — presence mode is static even for allowlisted commands. Replay-mode
behaviors (zero blocks fail, malformed fail, metachar/env-assignment/non-allowlisted fail,
timeout + process-group kill with no orphan) are pinned by `cli_verify_replay.rs`:

```evidence
$ cargo test --manifest-path gatekeeper/Cargo.toml --test cli_verify_replay
# expect: test result: ok. 21 passed
```

### AC-5 — human-commit approval + negative dogfood

Scratch-repo fixtures cover: clean human commit passes; agent-trailer approval fails; old-git /
shallow / untracked / dirty-spec obstacles fail closed with specific messages. Substance floor
rejects fixture (a).

**Negative dogfood (live, 2026-06-12):** with `[design] approval = "human-commit"` against this
repo, `check design --feature hollow-pass-kills` **FAILED**, tracing the `Status: approved` line
to its authoring commit (post-rebase `0f37b7d`) and matching
`Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>` against `(?i)claude` — the check rejects
exactly the delegated-approval practice (recorded maintainer direction, spec §4) it exists to
catch. Exit 1, message names the trailer, the pattern, and the honest residual.

```evidence
$ cargo test --manifest-path gatekeeper/Cargo.toml --test cli_design_hardening
# expect: test result: ok
```

### AC-6 — finish zero-test floor

`cli_finish_floor.rs` (17 tests): `test_command = "true"` fails; recognized zero-count summary
fails; unrecognized runner fails — via config **and** `-- <cmd>`; `pytest -q` summaries parse
(no `===` fence anchor); multi-binary cargo summaries sum; first-match-wins prevents
cargo/pytest double-count; `extra_count_patterns` admits a custom runner and still floors zero;
defaults unchanged (floor off → zero-count passes with a SHADOW line). The real suite passes the
floor (see AC-9 evidence — `just check` runs the full suite under the gate's own runner).

```evidence
$ cargo test --manifest-path gatekeeper/Cargo.toml --test cli_finish_floor
# expect: test result: ok. 17 passed
```

### AC-7 — SHADOW JSONL + legacy baseline

All four checks emit the seven-field schema (`gate`,`check`,`configured`,`artifact`,`command`,
`result`,`detail`) — pinned by `shadow_lines_have_exact_field_set`. Env-free default verify runs
emit `result:"static"`.

**Legacy baseline (recorded, no threshold — D2).** Documented loop over `docs/verify/*.md`
(12 artifacts predating this phase), quiet committed tree:

```sh
for f in docs/verify/*.md; do
  slug=$(basename "$f" .md | sed 's/^[0-9-]*//;s/^-//')
  GATEKEEPER_SHADOW=replay gatekeeper check verify --feature "$slug" 2>> shadow.jsonl >/dev/null
done
sed 's/^SHADOW //' shadow.jsonl | jq -s 'group_by(.result) | map({result: .[0].result, n: length})'
```

Result: **41 extracted commands → 1 pass, 5 fail, 35 skip.**
- 35 `skip`: legacy `$ `-lines failing the metachar/allowlist screen (curl pipes, shell
  one-liners, non-allowlisted tools) — extraction records, never executes them.
- 5 `fail`: every one is a `cargo test` variant written without `--manifest-path` — replay runs
  from the project root, which has no root `Cargo.toml` (exit 101). Honest finding: legacy
  artifacts assumed `cwd=gatekeeper/`.
- 1 `pass`: the one legacy command qualified with `--manifest-path`.

**Measurement found a real bug (fixed in this branch, commit `e1d7e54`):** the engine leaked
`GATEKEEPER_SHADOW=replay` into replayed children, so a replayed `cargo test` re-triggered
execution inside nested gatekeeper invocations — violating D5's "explicit, never implied" and
failing two `cli_verify_replay` tests in nested runs. `spawn_child` now `env_remove`s the
variable; regression test `shadow_env_not_inherited_by_replayed_children` (a replayed
`printenv GATEKEEPER_SHADOW` must exit non-zero). An earlier baseline run was additionally
contaminated by concurrent doc edits in the same tree (doc-sync red → nested `cargo test` 101);
the recorded numbers above are from a quiet, committed tree.

This artifact's own evidence blocks replay green — see the end-to-end record below.

### AC-8 — config strictness

Live demos (2026-06-12): `[verify] moode = "replay"` → doctor prints
`config.toml [verify]: unrecognized key(s): moode (ignored — forward compat; possible typo?)`,
exit 0. `[verify] mode = "repaly"` → `check verify` exits **2** with the expected-values
message. Unparsable-TOML → exit 2 for all three hardened gates and warn-and-default for non-gate
commands are pinned by `cli_verify_replay.rs` (`malformed_config_toml_exits_2_for_*`,
`invalid_verify_mode_exits_2`).

### AC-9 — gates + docs

`just check` (fmt-check, lint, test, shell, typos, docs) green at branch tip; CHANGELOG has the
v0.5.0 section; USER-GUIDE documents the three hardened-gate config tables, the evidence
grammar, the read-only/idempotent-evidence requirement, the SHADOW schema + jq aggregation,
`GATEKEEPER_SHADOW=replay`, and the deferred-Go note.

```evidence
$ just check
# expect: check docs: ok
```

## End-to-end replay of this artifact

After this file was committed, `GATEKEEPER_SHADOW=replay gatekeeper check verify --feature
hollow-pass-kills` was run against it: every evidence block above executed and passed
(per-command `SHADOW … "result":"pass"` lines; exit 0). The record of that run lives in the
review artifact alongside this file.

## Residuals

- `configured:"off"` is defined in the SHADOW schema but never emitted today
  (`ShadowConfigured::Off` is `#[allow(dead_code)]`) — burn-in tooling should treat it as
  reserved.
- Non-Unix timeout kill is direct-child only (documented residual, D8).
- The default allowlist includes `cargo test` / `cargo run` / `just` — broader than "read-only
  git"; USER-GUIDE documents the widen-deliberately caveat.
- Legacy verify artifacts are not retrofitted; their baseline numbers above are the Phase 15/17
  starting point.
