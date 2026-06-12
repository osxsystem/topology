# Code review — hollow-pass-kills (Phase 14, v0.5.0)

Branch: `feat/hollow-pass-kills` (vs `origin/main` @ `2fd50a1`), reviewed 2026-06-12.
Reviewer: delegated full-diff review (Sonnet subagent, independent of the implementing agents)
+ orchestrator pass; all dispositions below applied or parked at the orchestrator's direction.

## Scope checked

Dispatch-table refactor (longest-prefix match, `check` group routing, `check_help_or_unknown`
contract incl. first-`--` stop); evidence replay engine (parser, metachar/env-assignment/
token-boundary screens, timeout + process-group kill sequencing, `GATEKEEPER_SHADOW` no-op
under enforced replay, child env scrub on both cfg paths); design gate (git version/shallow
floors, untracked/dirty checks, `git log -L` SHA extraction, trailer regex); finish gate
(two-thread tee — no deadlock: unbounded channel, senders dropped at EOF, `wait()` after
drain; 1 MiB front-strip cap; first-match-wins counting; exit-code-before-floor ordering);
config strictness (enum keys hard-error, unparsable TOML → hardened gates exit 2, non-gate
warn-and-default); `cli_doc_sync` normalization grammar; CI/release workflow wiring.
446 tests green at review tip (6 ignored as specified).

## Blocking bugs

**None found.**

## Findings and dispositions

| # | Severity | Finding | Disposition |
|---|---|---|---|
| 1 | non-blocking | Untracked/dirty-spec obstacles emitted SHADOW `result:"fail"`; spec D7 + USER-GUIDE classify every obstacle as `"skip"` (burn-in pipelines key on this) | **FIXED** (`1c7ffb2`): returns `Skip`; enforced mode unchanged (still fail-closed, same message); regression test `shadow_dirty_spec_obstacle_logs_skip` |
| 2 | non-blocking | `# expect: ` with empty-after-trim value parsed as an expectation matching everything — hollow-assertion vector | **FIXED** (`1c7ffb2`): empty value → malformed block; two parser tests |
| 3 | wording | `gatekeeper check --foo` error says "unknown gate '--foo'" — flag-shaped input described as a gate; exit 2 + group usage correct per spec | parked — wording only, spec satisfied |
| 4 | detail-string | Passing-step detail reports post-truncation line count without the truncation note (failures do carry it) | parked — affects no decision or matching |
| 5 | spec note | = finding 3 viewed as spec divergence; spec does not pin wording | parked with 3 |
| 6 | spec note | Spec §6 says "runner image preinstalls cargo"; implementation correctly adds `dtolnay/rust-toolchain@stable` to `release.yml` version-guard | implementation right, spec comment stale — noted here, no code change |

Also found during verification (not by this review, recorded for completeness):
`GATEKEEPER_SHADOW` leaked into replayed children, violating D5 — fixed in `e1d7e54` with
regression test (details in the verify artifact).

## End-to-end replay record (AC-7 closure)

After the verify artifact was committed (`fa22210`), and re-run at review tip:

```text
$ GATEKEEPER_SHADOW=replay gatekeeper check verify --feature hollow-pass-kills
exit 0; 7 evidence commands executed, 7 × "result":"pass", 0 fail/skip
```

## Process notes

- Three protected-path commits on this branch (`87b64d4` finish floor, `fa22210` version bump,
  `1c7ffb2` review fixes) used `git commit --no-verify` under the maintainer's recorded
  in-session delegation (2026-06-12, "Delegate override to me"), each documented in its commit
  message — the dev-clone veto (live since PR #39) was not silently bypassed.
- The spec's approval commit remains the deliberate negative dogfood for `human-commit` mode
  (verify artifact AC-5).

## Verdict

Approve for merge to main and the v0.5.0 tag: blocking-bug-free at review tip, all nine
acceptance criteria verified (see the verify artifact), findings 1–2 fixed with regression
tests, 3–6 parked as cosmetic/spec-note follow-ups.
