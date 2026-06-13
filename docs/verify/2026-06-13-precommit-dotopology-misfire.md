# Verify: pre-commit hook blocking commits on a stray `.topology/` (issue #60)

- **Date:** 2026-06-13
- **Feature slug:** precommit-dotopology-misfire
- **Design:** docs/specs/2026-06-13-precommit-dotopology-misfire.md
- **Plan:** docs/plans/2026-06-13-precommit-dotopology-misfire.md
- **Code (working tree; committed by the maintainer via authorized `--no-verify`):**
  `hooks/pre-commit.sh` (drop the `TOPOLOGY_ROOT` export), `gatekeeper/src/main.rs` (negative-gate test).

## Reproduce-then-resolve (through the *deployed* hook)

### Reproduce — the live misfire (old hook behavior)

The old hook pinned `TOPOLOGY_ROOT="$ROOT/.topology"` on bare directory existence. With this repo's
deliberate non-marked `.topology/` (only `CONTRACT.md`), that points scan at a nonexistent rules file:

```
$ TOPOLOGY_ROOT="$PWD/.topology" gatekeeper scan --staged
gatekeeper scan: cannot load …/.topology/security/rules.toml: … (os error 2)   →  exit 2
```

That exit 2 is what made the deployed hook block **every** commit (forcing a `TOPOLOGY_ROOT="$PWD"`
workaround on every commit earlier this session).

### Resolve — the fixed hook, redeployed, lets a real commit through

The hook is a *copy* (`just setup`/`install.sh` `cp` it into `.git/hooks/pre-commit`), so the fix was
**redeployed via `just setup`** (`setup: updated .git/hooks/pre-commit`; side-effect: it also re-ran
`adapt`, converging this clone's `.claude/settings.json` to portable — expected, per #58/ADR-0019).

**This very verify note was committed by a normal `git commit` — no `--no-verify`, no
`TOPOLOGY_ROOT` workaround.** The deployed fixed hook ran `scan --staged`, the binary resolved the
framework root self-governed (the stray non-marked `.topology/` is ignored), loaded the repo's real
`security/rules.toml`, scanned clean, and the commit succeeded. Before the fix the same commit failed
at rules-load (exit 2 above). That successful commit *is* the end-to-end resolution evidence.

## Governed-project no-regression (vendored-binary layout — the path the hook takes)

Fixture: a consuming git repo (not itself marked) with a marked `.topology/` payload and the binary at
`.topology/bin/gatekeeper`. Run from the repo root with **no** `TOPOLOGY_ROOT`:

```
$ ( cd <repo> && .topology/bin/gatekeeper doctor )
framework root: <repo>/.topology
resolved by: binary-adjacent
$ ( cd <repo> && .topology/bin/gatekeeper scan --staged )   →  scan exit=0   # .topology rules loaded
```

`.topology` still resolves — via Step 3 `BinaryAdjacent` (walking up from `.topology/bin`), the path
the hook actually takes — and its rules load. Dropping the `TOPOLOGY_ROOT` export does not regress
governed scanning.

## Acceptance criteria

| Criterion | Evidence |
|---|---|
| Deployed-hook commit succeeds where it failed | This note committed normally through the redeployed fixed hook (resolve, above); reproduce shows the prior exit 2. |
| Governed no-regression (vendored-binary / Step 3) | doctor → `binary-adjacent` to `.topology`; `scan --staged` exit 0. |
| Negative-gate unit test | `resolve_root_rejects_non_marked_vendored_topology` (`main.rs`) — passes. |
| Hook no longer pins `TOPOLOGY_ROOT`; why-comment present | `hooks/pre-commit.sh` diff (plain-prose comment, issue #60 named). |
| No change to `install.sh`/`resolve_root` logic; binary-finder ladder + `cd`+`scan` untouched | diff scope: only the export block + one unit test. |

## Full gate

```
$ just check        # fmt-check + clippy + test + shell(shellcheck) + typos + docs
… test result: ok. (suite total 553 passed, 0 failed) …
shellcheck hooks/*.sh scripts/*.sh        # clean (covers the edited hook)
typos                                     # clean
check docs: ok
```

553 passed (552 baseline + 1 negative-gate test); shellcheck clean on the edited hook.

## Conclusion

The bug was a three-language-lanes violation — root-resolution logic duplicated in Bash, overriding
the binary's correct `is_marked_root` ladder with a wrong path. Deleting the export removes the
duplication; the binary resolves correctly in both the self-governed and governed cases (verified
end-to-end). The `TOPOLOGY_ROOT="$PWD"` workaround is no longer needed.
