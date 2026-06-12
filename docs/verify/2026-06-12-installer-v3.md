# Verify — installer v3: global payload + plugin retirement (Phase 8)

**Feature:** installer-v3
**Date:** 2026-06-12
**Spec:** `docs/specs/2026-06-12-installer-v3.md` (AC 1–9)
**Verified by:** main-loop agent (Fable 5), reviewing and re-validating the delegated
implementation (Sonnet subagent) at the branch head.

## AC-1/AC-2/AC-3 — global payload installs, checksum refusal, legacy rescue (offline e2e)

The e2e suite gained 23 global-scope/review-driven scenarios (piped install against a
`file://` release fixture with remapped `HOME`/`TOPOLOGY_HOME`; corrupted-checksum refusal
leaving an existing root untouched; checkout assembly where the checkout is not `ROOT`;
in-place re-run upgrade asserting the re-run's own exit code + upgrade marker; legacy-clone
rescue into `${ROOT}-backup-<ts>/` with ledger + handoff + clone-era `memory/artifacts/*`
contents (ADR-0013); `--yes` replacement; no-`PROJECT_PATH`-writes guard; the trailing-slash
`TOPOLOGY_HOME` data-loss regression from the PR #43 review, proven red without the one-line
fix — 47/1 — and green with it; the AC-3 interactive-refusal branch via the
`PROMPT_INPUT_FD` seam, including no-backup-littered-on-refusal; the headless-without-`--yes`
refusal applying the printed "N" default per ADR-0012 §4) plus a clone-era-handoff assertion
in the local legacy scenario, alongside the 29 pre-existing local-scope scenarios as the
regression net:

```evidence
$ just test-e2e
# expect: test-payload-e2e: 53 passed, 0 failed
```

## AC-4/AC-5 — plugin channel retired

`.claude-plugin/{plugin,marketplace}.json`, `hooks/ensure-gatekeeper.sh`, `hooks/hooks.json`
deleted; the release version guard now asserts tag == `gatekeeper/Cargo.toml` only (same
commit — never split). Remaining grep hits for the retired names are historical artifacts
and absence-assertions only (the AC-6 regression test, `build-payload.sh` exclusion comment,
`test-build-payload.sh` negative assertions) — checked by hand, no live wiring. The payload
manifest test still proves the tarball ships none of them:

```evidence
$ just test-payload
# expect: test-build-payload: 26 passed, 0 failed
```

## AC-6 — `.claude-plugin` is no longer a root marker

Unit regression (`ROOT_MARKERS == ["AGENTS.md", "gatekeeper"]`):

```evidence
$ cargo test --manifest-path gatekeeper/Cargo.toml --bin gatekeeper skills_and_claude_plugin_alone_is_not_a_marked_root
# expect: 1 passed
```

Behavior-level: a git repo shaped like a Claude Code plugin checkout (`skills/` +
`.claude-plugin/`) no longer self-governs — doctor resolves `fallback (cwd)` and FAILs (F1).
This fixture was committed red-first (`603a770`, force-run failed against the
pre-retirement code) and un-ignored after the retirement:

```evidence
$ cargo test --manifest-path gatekeeper/Cargo.toml --test cli_root_markers
# expect: 1 passed
```

## AC-7/AC-8 — PATH cleanup and CI wiring

`sudo ln` suggestion removed (`grep -c 'sudo ln' scripts/install.sh` → 0); the stale-PATH
repair block is unchanged (diff-audited). `ci.yml` gained the offline `installer` job
running the three suites; version-resolution precedence still holds:

```evidence
$ just test-fetch
# expect: test-fetch-version: 3 passed, 0 failed
```

## AC-9 — full quality gate

461 unit tests passed / 6 ignored (main: 460/6 — net +1 from the marker regression and the
two red fixtures), fmt-check, clippy `-D warnings`, shellcheck over every touched script,
typos, docs lint:

```evidence
$ just check
# expect: check docs: ok
```

## Formerly known gap — closed by the PR #43 review response

The interactive-refusal branch of the legacy-clone prompt was initially recorded as
untestable offline (`can_prompt()` probed `/dev/tty` only). Per the reviewer's suggestion,
the existing `PROMPT_INPUT_FD` seam (already used by the stale-PATH repair) was extended to
`can_prompt()`/`ask()` — default `/dev/tty` behavior unchanged when unset — and e2e test K
now exercises the branch: answering `n` aborts with exit 1, the intact-clone message, and an
untouched `.git` + ledger. The same review found a real data-loss bug (trailing-slash
`TOPOLOGY_HOME` made the rescue backup a child of `ROOT`, deleted with the clone after
printing "rescued"); fixed by `ROOT="${ROOT%/}"` normalization with regression test J.

## Quality gates

- `gatekeeper check tdd --feature installer-v3`: PASS — the branch was restructured
  (pre-push) so a genuine test-only red commit (`603a770`, `gatekeeper/tests/`) precedes all
  production commits; the delegated implementation had placed the red fixture inside
  `main.rs`'s test module, which the gate's path heuristic correctly refuses to count.
- `gatekeeper check docs`: ok. Verify gate replayed under `GATEKEEPER_SHADOW=replay`.
- Version 0.7.0 in `Cargo.toml`/`Cargo.lock` — the only version manifests left.
- Protected-path commits carry the documented `--no-verify` override per the Track 2 grant.
