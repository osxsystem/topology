# Verify — root resolution hardening (Phase 11)

**Feature:** root-resolution-hardening
**Date:** 2026-06-12
**Spec:** `docs/specs/2026-06-12-root-resolution-hardening.md` (AC 1–9)
**Verified by:** main-loop agent (Fable 5), reviewing and re-validating the delegated
implementation (Sonnet subagent) at the branch head.

## AC-1…AC-7 — pure-function unit coverage of the precedence chain

All ten `resolve_root_*` unit tests drive the four-input pure function over tempdir
fixtures: env-override wins / invalid ignored (AC-2), self-governed beats vendored and
binary-adjacent (AC-4), binary-adjacent `bin/` layout (AC-3), project `.topology` plus the
Q4 git-less-nesting case (AC-5), global home (AC-6), and fallback identity (AC-1, AC-7):

```evidence
$ cargo test --manifest-path gatekeeper/Cargo.toml --bin gatekeeper resolve_root_
# expect: 10 passed
```

## AC-1/AC-3/AC-6/AC-7/AC-8 — end-to-end integration fixtures

Five fixtures run the real binary in scratch layouts (binary copied to `<root>/bin/`,
`HOME` remapped, `TOPOLOGY_ROOT` scrubbed): (a) hijack-class ancestor no longer wins and
the fallback warning appears **exactly once** on stderr; (b) governed project outside
`$HOME` resolves `~/.topology` (W2); (c) binary-adjacent `bin/` install resolves its root
from an unrelated cwd; (d) doctor F1 exits non-zero with no root anywhere; (e) doctor F2
exits non-zero from inside a payload clone with `VERSION`:

```evidence
$ cargo test --manifest-path gatekeeper/Cargo.toml --test cli_root_resolution
# expect: 5 passed
```

## AC-8 — doctor provenance line

The fixtures above assert `resolved by:` appears in doctor output; the zero-exit healthy
cases are covered by the existing doctor suite, which still passes unchanged (includes the
untouched `version_skew` tests, AC-9):

```evidence
$ cargo test --manifest-path gatekeeper/Cargo.toml --test cli_doctor
# expect: 0 failed
```

## AC-9 — full quality gate

`just check` chains fmt-check, clippy `-D warnings`, the full test suite (460 passed,
6 ignored — up from 452/6 on main: 10 new unit tests, 5 new fixtures, net of replaced
old-walk tests), shellcheck, typos, and the docs lint, ending in:

```evidence
$ just check
# expect: check docs: ok
```

## Manual smoke — fallback fixture transcript (not replayable: bare `env`/`mktemp` are
outside the evidence allowlist)

Recorded 2026-06-12 against the branch-head release build:

```
$ T=$(mktemp -d) && cp gatekeeper/target/release/gatekeeper "$T/gk" \
    && cd "$T" && git init -q proj && cd proj \
    && env -u TOPOLOGY_ROOT HOME="$T" "$T/gk" doctor 2>stderr.txt >/dev/null
$ echo "exit=$?"
exit=1
$ grep -c "no framework root found" stderr.txt
1
```

Before the orchestrator-review fix (`251e5e6`) the same fixture printed the warning **3×**
(handler duplicate + two un-guarded `framework_root()` calls) — the spec AC-7 "exactly
once" claim is now enforced by fixture (a)'s count assertion, not a `contains()`.

## Quality gates

- `gatekeeper check tdd --feature root-resolution-hardening`: PASS (test-only red commit
  `005390f` precedes all production commits).
- `gatekeeper check docs`: ok.
- `just check`: green at the branch head.
- No new dependencies (ADR-0007); protected-path commits (`main.rs`, `Cargo.toml`) carry
  the documented `--no-verify` override per the Track 2 grant.
