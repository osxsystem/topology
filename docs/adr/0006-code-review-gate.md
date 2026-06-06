# 0006 — The code-review gate is a commit-bound, fail-closed critic artifact

- **Status:** Accepted
- **Date:** 2026-06-05

Topology's `verify` gate has the author grade their own work — the weakest form of review, and the
system's main gap. We add a `review` gate between `verify` and `finish`: a fresh-context critic
subagent audits the branch diff and writes a review artifact that a new `gatekeeper check review`
subcommand validates against git state.

## Why

LLMs exhibit measurable self-preference bias, worst exactly when the output is wrong, and
self-refinement amplifies it — so a *separate* critic is the evidence-backed fix (Zheng et al.
2023; Panickssery et al. 2024). The critic's verdict is untrusted: prompt injection coerces LLM
judges into passing verdicts at 30-73.8% on a code-review task (Maloyan & Namiot 2025). The trust
boundary is therefore the deterministic parser + git state, not the model.

## Decisions

- **(a) Artifact bound to a clean commit + merge-base.** The review counts only for the exact
  `HEAD` (clean worktree, excluding `docs/reviews/`), against the verified `git merge-base` of the
  integration branch. A stale, dirty, or wrong-based review cannot be replayed.
- **(b) Fail-closed grammar; `strip_comments` not reused.** A line-1 verdict, full-hex
  HEAD/BASE, exactly one blocking heading, pass <=> `None.`, both rubric dimensions required, and
  no HTML comments in parsed regions. Any deviation is a veto. `strip_comments` is fail-open on an
  unclosed comment, so the parser does not reuse it.
- **(c) A single two-dimension critic.** Deep research refuted that multiple parallel *agents*
  catch more than one critic, so we ship one critic — but it audits *both* the Spec/plan and
  Standards *dimensions*, gate-enforced. Multi-critic voting (the one evidence-backed reason to run
  more than one agent, for injection-robustness) is deferred.
- **(d) Pulled ahead of security scanning.** The self-review gap is the highest-leverage fix, so
  the gate ships before Phase 1 security scanning.

## Consequences

- `gatekeeper` grows a `review.rs` module (pure parser + git-state gate); no new dependency.
- A residual is accepted and documented: a fully-subverted critic emitting a clean `pass` for the
  correct head/base is undetectable by any parser. Reducers: `file:line` evidence, a different-model
  critic, and future voting.
- The review artifact is a transient gate input committed by `finish-branch`; re-running the gate
  after that commit fails (it is a pre-finish check, not a CI replay). A tree/diff-hash binding
  would make it replay-safe (future work).
