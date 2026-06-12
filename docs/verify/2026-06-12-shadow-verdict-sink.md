# Verify — shadow-verdict sink

**Feature:** shadow-verdict-sink
**Date:** 2026-06-12
**Spec:** none — pre-Phase-15 enabler, scoped in the 2026-06-12 grilling session (decisions in
`docs/ROADMAP.md` intro note: per-gate flip criterion ≥50 evaluations, human-triaged
false-block <2%). The full gate sequence was waived by the maintainer for this enabler;
verify + review artifacts retained.
**Verified by:** main-loop agent (Fable 5), reviewing and re-validating the delegated
implementation (Sonnet subagent) at the branch head.

## AC-1 — append_line_at creates the parent dir, appends in order, and is fail-silent

```evidence
$ cargo test --manifest-path gatekeeper/Cargo.toml --bin gatekeeper append_line_at
# expect: 2 passed
```

The fail-silent case uses a path whose parent is an existing regular file — the append is
silently dropped, no panic.

## AC-2 — file line carries a leading ts plus the exact 7-field SHADOW contract

```evidence
$ cargo test --manifest-path gatekeeper/Cargo.toml --bin gatekeeper file_line_shape
# expect: 1 passed
```

## AC-3 — stderr SHADOW contract is byte-identical to v0.5.0

The integration test that pins the exact stderr field set still passes unchanged:

```evidence
$ cargo test --manifest-path gatekeeper/Cargo.toml --test cli_verify_replay shadow_lines_have_exact_field_set
# expect: 1 passed
```

## AC-4 — no doc drift from the USER-GUIDE addition

`scripts/shadow-stats.sh` is documented as a script, not a `gatekeeper` subcommand, so the
CLI↔doc sync test stays green:

```evidence
$ cargo test --manifest-path gatekeeper/Cargo.toml --test cli_doc_sync
# expect: 1 passed
```

## AC-5 — end-to-end: governed fixture writes the sink and shadow-stats renders it

Manual smoke (not replayable here — runs in a throwaway fixture outside the allowlist).
Transcript from 2026-06-12:

```
$ cd $(mktemp -d)/govproj && git init -q .
$ TOPOLOGY_ROOT=<worktree> gatekeeper check finish -- echo "test result: ok. 1 passed; 0 failed"
SHADOW {"gate":"finish","check":"zero_test_floor","configured":"default","artifact":null,...,"result":"pass",...}
PASS finish gate: test command exited 0   (exit=0)
$ cat .claude/topology/logs/shadow.jsonl
{"ts":1781244113,"gate":"finish","check":"zero_test_floor","configured":"default",...,"result":"pass",...}
$ bash scripts/shadow-stats.sh .claude/topology/logs/shadow.jsonl
finish               zero_test_floor               1     1     0     0        0            0.0%
Would-block details (0 verdict(s) to triage) — (none)
Flip criterion (per gate): >=50 evaluations AND human-triaged false-block rate <2%
```

The governed-project sink path is `.claude/topology/logs/shadow.jsonl` (artifacts root), not
the payload — the payload stays read-only at runtime.

## Quality gates

`just check` (fmt-check, clippy `-D warnings`, full test suite, shellcheck, typos, docs lint)
green at the implementation commit: 452 passed, 6 ignored (up from 449/6 — the 3 new unit
tests above). `scripts/shadow-stats.sh` is shellcheck-clean, POSIX awk only, no jq.
