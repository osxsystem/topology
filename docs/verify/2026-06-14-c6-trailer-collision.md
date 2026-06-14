# Verify: C6 — doctor check for the Co-Authored-By × approval_provenance trailer collision

- **Date:** 2026-06-14
- **Feature slug:** c6-trailer-collision
- **Plan:** `docs/plans/2026-06-14-c6-trailer-collision.md`

## Symptom (reproduced)

The agent harness stamps `Co-Authored-By: Claude …` on **every** commit. Under `[design] approval =
"human-commit"`, the design gate's `approval_provenance` check reads any agent `Co-Authored-By` trailer
on the approval commit as agent self-approval and **FAILs** — silently bitten by a harness rule that
lives in no file.

Reproduced live in this repo — every recent authored commit carries the trailer the gate keys on:

```
$ git log -n 5 --no-merges --format='%h %(trailers:key=Co-Authored-By,valueonly)'
d3cb721 Claude Opus 4.8 (1M context) <noreply@anthropic.com>
7e0f014 Claude Opus 4.8 (1M context) <noreply@anthropic.com>
220bb77 Claude Opus 4.8 (1M context) <noreply@anthropic.com>
83089c2 Claude Opus 4.8 (1M context) <noreply@anthropic.com>
e3f21dd Claude Opus 4.8 (1M context) <noreply@anthropic.com>
```

Under `human-commit`, a human approval commit here would FAIL with no hint that the harness rule is the
cause.

## Resolution (the new `gatekeeper doctor` probe)

`probe_approval_trailer_collision` detects the collision from git history and emits a single advisory
`WARN` (never affects exit code). Verified against the freshly-built release binary in isolated temp
repos (full control over config + history):

| # | Config | History | Expected | Observed |
|---|--------|---------|----------|----------|
| A | `approval="human-commit"` | commit w/ `Co-Authored-By: Claude …` | **WARN** | `approval trailer collision: WARN` ✓ |
| B | default (`status-line`) | commit w/ `Co-Authored-By: Claude …` | **n/a** | `approval trailer collision: n/a (approval=status-line…)` ✓ |
| C | `approval="human-commit"` | clean human commit (no trailer) | **ok** | `approval trailer collision: ok` ✓ |
| D | `approval="human-commit"` | commit w/ `Co-authored-by: copilot[bot]` | **WARN** | `approval trailer collision: WARN` ✓ |

Scenario A's full WARN line names the matched value and pattern and the remediation:

```
approval trailer collision: WARN: recent commits carry an agent Co-Authored-By trailer
("Claude Opus 4.8 (1M context) <noreply@anthropic.com>") matching agent_trailer_patterns pattern
"(?i)claude"; under [design] approval="human-commit" a human approval commit carrying this trailer
will FAIL the design gate (read as agent self-approval). Drop the always-add-Co-Authored-By rule from
your harness/commit template for approval commits, or relax [design] agent_trailer_patterns.
```

D confirms the probe reuses the *full* default pattern set (`(?i)copilot` and `(?i)\[bot\]` both fire),
not just `claude`, because it reads `cfg.design_agent_trailer_patterns` — the same config the gate reads.

## Bug found and fixed BY the verify gate

On the first run, **scenario C returned `n/a (git history unavailable)` instead of `ok`.** Root cause:
the probe short-circuited on *empty git stdout*, conflating "git failed / not a repo" with the
legitimate "commits exist but none carry a trailer" case. The pure matcher already returned `None` on
empty input correctly; the bug was a premature `if stdout.trim().is_empty()` branch in the probe.

**Fix:** removed that branch so empty-but-successful git output flows to the matcher → `None` → `ok`.
Git *failure* (non-zero exit / not a repo / git missing) still hits the `n/a (git history unavailable)`
arm via the `match out { Ok(o) if o.status.success() … , _ => n/a }` guard. Re-ran all scenarios →
C now correctly reports `ok` (table above). This is exactly the class of behavior a green unit suite
missed and the reproduce-then-resolve gate caught.

## Gate evidence

- **TDD:** 6 pure-fn unit tests written first; observed RED (compile errors — `approval_trailer_collision`
  did not exist) before GREEN.
- **fmt:** `cargo fmt -- --check` clean. **clippy:** `--all-targets -D warnings` clean.
- **Full suite:** 585 passed / 0 failed across all binaries (baseline 579; +6 new).
- **Finish gate:** `gatekeeper check finish -- cargo test …` → `PASS finish gate: test command exited 0`
  (`zero_test_floor` shadow: recognized count=585).
- **No protected files touched:** change is confined to `gatekeeper/src/doctor.rs` (unprotected); no
  `--no-verify` required.
