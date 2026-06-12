# Verify — installer v3: global payload + plugin retirement (Phase 8)

**Feature:** installer-v3
**Date:** 2026-06-12
**Spec:** `docs/specs/2026-06-12-installer-v3.md` (AC 1–9)
**Verified by:** main-loop agent (Fable 5), reviewing and re-validating the delegated
implementation (Sonnet subagent) at the branch head.

## AC-1/AC-2/AC-3 — global payload installs, checksum refusal, legacy rescue (offline e2e)

The e2e suite gained 15 global-scope scenarios (piped install against a `file://` release
fixture with remapped `HOME`/`TOPOLOGY_HOME`; corrupted-checksum refusal leaving an existing
root untouched; checkout assembly where the checkout is not `ROOT`; in-place re-run upgrade;
legacy-clone rescue into `${ROOT}-backup-<ts>/` with ledger + handoff contents; `--yes`
replacement; no-`PROJECT_PATH`-writes guard) alongside the 29 pre-existing local-scope
scenarios, which ran unmodified as the regression net:

```evidence
$ just test-e2e
# expect: test-payload-e2e: 44 passed, 0 failed
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

## Known gap (recorded, not hidden)

The interactive-refusal branch of the legacy-clone prompt (`answer != y` → exit 1, clone
intact) is not exercised by the e2e suite: `can_prompt()` probes `/dev/tty`, which does not
exist in the offline/CI harness, so only the non-interactive (`--yes`/no-tty) path is
testable there. The `--yes` replacement and the rescue itself are covered; the refusal
branch is 6 lines of `case` shared verbatim with the long-tested local path.

## Quality gates

- `gatekeeper check tdd --feature installer-v3`: PASS — the branch was restructured
  (pre-push) so a genuine test-only red commit (`603a770`, `gatekeeper/tests/`) precedes all
  production commits; the delegated implementation had placed the red fixture inside
  `main.rs`'s test module, which the gate's path heuristic correctly refuses to count.
- `gatekeeper check docs`: ok. Verify gate replayed under `GATEKEEPER_SHADOW=replay`.
- Version 0.7.0 in `Cargo.toml`/`Cargo.lock` — the only version manifests left.
- Protected-path commits carry the documented `--no-verify` override per the Track 2 grant.
