# Design: Code-review critic gate

- **Date:** 2026-06-05
- **Feature slug:** code-review-gate
- **Status:** approved (2026-06-05)
- **Revision:** research-informed + twice cross-model-reviewed (2026-06-05).
  - *R1:* deep-research pass — confirmed the fresh-context critic and verdict+blocker gate,
    surfaced **prompt-injection** as the dominant threat, **refuted** the multi-*agent* design.
  - *R2:* cross-model review (Codex / GPT-5.5) returned `fail`/8 blockers → added HEAD-binding, a
    strict fail-closed grammar, dropped the duplicate-verdict rule, fixed a `finish` factual error,
    restored the Standards dimension.
  - *R3:* re-review of R2 confirmed 5/8 resolved and raised 7 deeper issues → **bind to a clean
    worktree + machine-checked merge-base + full HEAD SHA**, **gate-enforce** that a pass has an
    empty (`None.`) blocking section *and* documents both rubric dimensions, run git from the
    framework root, and fix the (previously invalid) example template. One residual (a fully
    subverted critic emitting `pass` on line 1) is **accepted and documented** — no deterministic
    parser can catch it.
  - *R4:* final verification of R3 confirmed 6/7 fixes hold and the residual is reasonable, and
    caught one real design blocker — a **clean-tree / current-HEAD paradox** (writing or committing
    the review artifact would either dirty the tree or move `HEAD`, making the gate unsatisfiable).
    Fixed: `HEAD:` binds to the reviewed *code* commit and the clean-tree check **excludes
    `docs/reviews/`**, so the untracked artifact doesn't dirty the gate. Example templates made
    literally parseable. **Converged.**
  See *Research basis* and *Cross-model review*.

## Problem

Topology's thesis is **gates, not rules**: every methodology transition should be an objective,
checkable condition, not an aspiration the agent can rationalize past. Five of the six stages honor
this. One does not.

`verify-before-done` asks **the same agent that wrote the code** to grade its own work — what the
research report (`RESEARCH.md:166`) calls "the weakest form" and the system's **main gap**. An
author auditing their own diff shares the author's blind spots and incentive to declare victory.

This is not just intuition. LLMs exhibit a measurable **self-preference / self-bias**: they score
their own generations higher than independent assessment warrants, the effect is worst exactly when
the output is wrong, and *self-refinement amplifies the gap with each iteration* (Zheng et al.,
NeurIPS 2023; Panickssery et al., NeurIPS 2024; Xu et al., 2024 — see *Research basis*). A separate
critic is the evidence-backed fix.

**Who it's for.** Anyone running Topology on real work — and the project itself, which today
violates its own headline principle.

**Success looks like:** a fresh-context critic subagent (no memory of writing the code) audits the
change against the acceptance criteria, the plan, *and the repo's documented standards*, writes its
findings to a review artifact **bound to the exact reviewed commit and diff**, and a new
deterministic gate — `gatekeeper check review` — **blocks the finish stage** unless that artifact
passes, is for the *current clean* `HEAD` diffed against the *correct merge-base*, documents both
review dimensions, and has no blocking findings. The gate treats the artifact as **untrusted
input**: the parser + git state — not the model — are the trust boundary, and every ambiguity
resolves to FAIL.

## Constraints

- **The gatekeeper is std-only Rust** (no deps, builds offline, one static binary — `main.rs:11`).
  It **cannot run a subagent**; it can only *inspect the artifact* and *check it against git state.*
  The model does the work inside the stage; deterministic code decides advancement — the Sequential
  pattern the research endorses (`RESEARCH.md:164`).
- **The artifact is untrusted** and the parser is **fail-closed**: any deviation from the strict
  grammar, or any git-state mismatch, is a FAIL — never a pass.
- **The gate shells out to git, from the framework root.** All git commands run as
  `git -C <framework_root()> …` (not process cwd) so nested repos/submodules can't shift the
  comparison (`framework_root()` already exists, `main.rs:63`). This matches `gate_finish`, which
  already uses `std::process::Command` (`main.rs:289`) — no new dependency.
- **It binds to a clean worktree — except review artifacts.** A review describes a committed *code*
  state; if the worktree is dirty, the finish command tests code the review never saw. The gate
  requires `git status --porcelain` to be empty **ignoring paths under `docs/reviews/`** — otherwise
  writing the review artifact would itself dirty the tree and the gate could never pass. `HEAD:`
  binds to the reviewed *code* commit; the freshly-written artifact is expected to be untracked when
  the gate runs (see *Workflow & ordering*).
- **Consistency with existing gates.** Reuses the *directory-reading* shape of `find_doc` but with
  **stricter matching** (below) and **does not reuse `strip_comments`** — that helper is fail-open
  on an unclosed comment (`main.rs:280` drops the document tail), unacceptable in a fail-closed
  boundary.
- **Portability.** The critic is dispatched via the harness's subagent primitive (Task/Agent in
  Claude Code; equivalent elsewhere). The gate and artifact contract are harness-agnostic.
- **Non-goals.** Not a security scanner (Phase 1). Not an auto-fixer. Not a replacement for human
  review. Not a *multiple-parallel-agent* critic (see *Decision*).

## Approaches considered

1. **Artifact-exists only** — passes if a review doc exists, like `design`/`verify`. A review full
   of blockers, or one for an old commit, would still pass. **Rejected.**
2. **Parsed verdict, no blockers, bound to commit+diff** *(chosen)* — strict machine header
   (verdict, HEAD, BASE) + a fail-closed body grammar; the gate verifies git state and content.
   A critic that found problems can't be rubber-stamped, and a review of stale/dirty/wrong-base
   code can't be replayed. The research endorses this "corroborate, don't trust a single free-form
   verdict" shape (Zheng et al.; Maloyan et al., 2025). **Chosen.**
3. **Fold into verify** — one subagent runs the suite and audits. Loses the author's evidence step
   and couples "works" with "good." **Rejected.**
4. **Multiple parallel agents (Standards agent + Spec agent, not merged)** — from Matt Pocock's
   `review` skill (`/Users/hugues_mini/Codes/hardSkills`). The research **refuted every empirical
   claim** that parallel agents catch more than one critic (0-3/1-2). **Deferred.** *Key
   distinction:* what was refuted is *multiple agents*, **not** the *two dimensions* — so the single
   critic still audits **both** Standards *and* Spec/plan, as gate-enforced rubric sections. The only
   evidence-backed reason to run >1 *agent* is injection-robustness via *voting* — also deferred.

## Decision

**A single fresh-context critic with a gate-enforced two-dimension rubric, placed after `verify`
and before `finish`, with a clean-worktree + commit + merge-base bound, fail-closed parser.**

```
design → plan → tdd → verify → review → finish
```

### Diff scope and integration branch *(decided)*

The critic reviews **`git diff <merge-base>...HEAD`** (three-dot) — the changes *this branch
introduces* relative to its fork point from the integration branch. The integration branch defaults
to **`main`**, overridable with `--base <ref>`; the gate fails with a clear message if the ref
doesn't resolve. *Correction (from R2 review):* this is **not** "what `finish` merges" — `finish`
only runs a test command, no merge (`main.rs:289`) — and three-dot does **not** reflect a post-merge
result if the integration branch advanced after branching (it won't surface conflicts with newer
main; a post-merge re-review is out of scope for v1). Large diffs default to the whole-branch diff;
chunking is **future work**.

### What the critic is instructed to do (the skill)

1. **Dispatch a fresh subagent** — a *different model* where the harness allows it (stronger; see Risks).
2. **Review two dimensions, separately, and document each** —
   (a) **Spec/plan conformance** — does the diff implement the acceptance criteria and plan; missing,
   partial, or scope-creep?
   (b) **Standards conformance** — does the diff follow `docs/adr/`, `AGENTS.md`, `METHODOLOGY.md`,
   and `CONTEXT.md` if present? Cite the standard.
3. **Require evidence per finding** — every blocking finding cites `file:line`. No location → not a blocker.
4. **Seek reasons to FAIL first.** **Skip tooling-enforced checks** (lint/format/types the `finish`
   gate or linters catch); distinguish hard violations from judgement calls.
5. **Emit the exact grammar below**, using `git rev-parse HEAD` and the computed merge-base verbatim;
   never put HTML comments or raw diff lines in the machine-parsed regions.

### The artifact contract

The critic writes `docs/reviews/<YYYY-MM-DD>-<feature-slug>.md`. **Lines 1–3 are the machine header**
(position-anchored); the body follows. A `pass` artifact looks **exactly** like:

```markdown
VERDICT: pass
HEAD: 9f3c1a7e5b2d8c4f0a1b6e7d9c2f5a8b3e4d6c1a
BASE: 2a7d4e1c9b6f3a8d5e2c1b0a9f8e7d6c5b4a3210

# Review: <feature> (<date>)

## Blocking findings
None.

## Non-blocking notes
- <nits — never gate on these>

## Criteria checked
### Spec/plan
- <acceptance criterion 1> — <how the diff satisfies it>
### Standards
- <ADR/AGENTS rule> — <conformance evidence>
```

A `fail` artifact is identical except line 1 is `VERDICT: fail` and `## Blocking findings` lists
one or more items instead of `None.`:

```markdown
VERDICT: fail
HEAD: 9f3c1a7e5b2d8c4f0a1b6e7d9c2f5a8b3e4d6c1a
BASE: 2a7d4e1c9b6f3a8d5e2c1b0a9f8e7d6c5b4a3210

# Review: <feature> (<date>)

## Blocking findings
- src/foo.rs:42 — <what's wrong and why it blocks>

## Non-blocking notes
- ...

## Criteria checked
### Spec/plan
- ...
### Standards
- ...
```

**Strict grammar (the parser's contract):**
- Strip an optional leading UTF-8 BOM; normalize CRLF→LF; trim trailing whitespace per line.
- **Line 1** must equal exactly `VERDICT: pass` or `VERDICT: fail` (uppercase keyword, one space,
  lowercase value). Anything else → FAIL-CLOSED.
- **Line 2** must equal `HEAD: <sha>` where `<sha>` is the **full** 40- or 64-hex output of
  `git rev-parse HEAD`. **Line 3** must equal `BASE: <sha>` (full hex). Malformed → FAIL-CLOSED.
- A verdict-shaped line **anywhere else** is ignored (line 1 is sole authority) — honest reviews may
  quote `VERDICT:` in findings without false-failing.
- Exactly **one** `## Blocking findings` heading. Zero or >1 → FAIL-CLOSED.
- For a **pass**, the blocking section must be the single token `None.` (no list items). If a pass
  artifact contains any blocking list item → FAIL-CLOSED (contradiction). For a **fail**, it must
  contain ≥1 item. *(There is no "resolved/checked blocker" state — a fresh per-commit review either
  has blockers or it doesn't, so checkboxes are gone.)*
- Exactly one `## Criteria checked` heading containing **both** `### Spec/plan` and `### Standards`
  subheadings, each with ≥1 non-empty content line. Missing either → FAIL-CLOSED. *(Gate-enforces
  that both dimensions were actually reviewed.)*
- An HTML comment (`<!--`, even unclosed) anywhere in the header, the `## Blocking findings`
  section, or the `## Criteria checked` section → FAIL-CLOSED. (`strip_comments` is **not** used.)
- **Section boundary:** a section runs from its heading to the next line that — after
  normalization — begins exactly with `## ` (an H2), or to EOF. `### ` (H3) subheadings do **not**
  end a `## ` section, so `### Spec/plan` stays inside `## Criteria checked`. The machine-parsed
  sections admit no fenced code, so there is no `## `-inside-a-fence ambiguity.

### The gate logic (`gatekeeper check review --feature <slug> [--base <ref>]`)

1. **Establish git state** (all via `git -C <framework_root>`):
   - `rev-parse HEAD` → `head`. If it fails (not a repo) → FAIL.
   - `status --porcelain`, **ignoring paths under `docs/reviews/`**, non-empty → FAIL (uncommitted
     code). The freshly-written review artifact is untracked at gate time and is excluded by design.
   - resolve integration branch (`--base` or `main`); `merge-base <branch> HEAD` → `base`. Unresolvable → FAIL.
2. **Select the fresh artifact.** Among `docs/reviews/*-<slug>.md`, parse each line-2 `HEAD:`; keep
   those equal to `head`. zero → FAIL (stale/missing); >1 → FAIL (ambiguous, print all paths);
   exactly one → proceed. *(Replaces `find_doc`'s nondeterministic first-match; a re-review on a new
   commit auto-supersedes stale ones.)*
3. **Validate** the chosen artifact against the strict grammar, **and** check line-3 `BASE:` equals
   the computed `base`. Any violation → FAIL-CLOSED. Verdict `fail` → FAIL.
4. Otherwise PASS, printing the artifact path (`PASS review gate: <path>`).

Exit codes: `0` pass, `1` veto, `2` usage error. **Every ambiguity resolves to FAIL.**

### Workflow & ordering *(resolves the clean-tree / HEAD paradox)*

The review artifact is a *transient gate input*, not part of the reviewed code:

1. Code is committed at `HEAD = X`, worktree otherwise clean.
2. The critic writes `docs/reviews/<date>-<slug>.md` with `HEAD: X` — now the only untracked path,
   under `docs/reviews/`, so it does not dirty the gate.
3. `gatekeeper check review` runs: clean-except-reviews ✓, artifact `HEAD` == `X` ✓, `BASE` matches ✓ → PASS.
4. On PASS, `finish-branch` commits the artifact (and merges) as its final step.

*Re-run caveat (accepted):* committing the artifact advances `HEAD` to `Y`, so re-running
`check review` afterward finds no artifact for `Y` and fails — the gate is a **pre-finish check, not
a CI replay**. A replay-safe binding (hash the reviewed tree/diff instead of naming `HEAD`) is
future work.

## Threat model & parser hardening

**Threat:** the diff under review is attacker-controllable (or accidentally contains verdict-shaped
text). LLM judges are coerced into passing verdicts by injected content at **30–73.8%** ASR against
GPT-4 / Claude-3-Opus *on a code-review task* (Maloyan & Namiot, 2025; Maloyan et al., 2025).

**Trust boundary:** the LLM critic is **corruptible**; the std-only parser + git state are the
boundary. Defenses by *structure*, not trust:

- **Commit + diff binding** — the review counts only for the exact `head`, against the verified
  `base`, on a *clean* tree. It cannot be replayed against later, dirtier, or differently-based code,
  and a stale review is auto-superseded.
- **Position anchoring** — the verdict is line 1 only; verdict-shaped lines elsewhere are ignored
  (no false-fail on honest quoting; no authority for transcribed/injected text).
- **Strict fail-closed grammar** — exactly one blocking heading; pass ⇔ `None.`; both rubric
  dimensions required; no comments in parsed regions. Unknown syntax fails closed.

**Residual risk (accepted, documented):** a critic that is *itself* fully subverted could emit a
clean `VERDICT: pass` on line 1, `None.` blockers, and plausible rubric text *for the correct
head/base on a clean tree*. **No deterministic parser can detect this** — it is the irreducible
floor of "trust the critic's judgement." Reducers, not eliminators: `file:line` evidence; a
**different-model** critic (this spec was itself cross-reviewed by GPT-5.5); and future
**multi-critic voting** (a 7-model committee cut the strongest attack 73.8%→19.3%; Maloyan & Namiot,
2025). Voting is the one evidence-backed reason to run more than one *agent*.

## Risks & open questions

- **Same-model fresh-context is necessary but not sufficient.** Self-bias is partly driven by
  *familiarity*, not only authoring memory; a same-family critic can over-reward fluent output. A
  **different model** is stronger where the harness allows it — the skill recommends it. *(Medium.)*
- **Evidence is largely out-of-code-domain.** Strongest self-bias demos are translation/math/text;
  code evidence is thinner and the original Self-Refine found self-feedback *helps* code. Direction
  holds; magnitude uncertain for a 2026 frontier critic. Don't over-engineer a possibly-small effect.
- **Integration-branch assumption.** The base bind assumes a resolvable integration branch (`main`
  or `--base`). Detached/unusual setups must pass `--base`; the gate fails loudly otherwise.
- **Three-dot misses target-branch drift** (accepted for a pre-merge critic).
- **Forgery.** Hand-authoring a fake passing review for the current head/base on a clean tree is
  undetectable, as with every doc-based gate; the binding raises the cost. Accepted.
- **Re-run after artifact commit.** Committing the review artifact advances `HEAD`, so a later
  `check review` finds no artifact for the new `HEAD`; the gate is pre-finish, not a replay. A
  tree/diff-hash binding would make it replay-safe (future work).
- **Open — large diffs:** chunking/prioritization unproven; default whole-branch.
- **Open — cross-model critic value for code:** unquantified; candidate for a Topology A/B.
- **Open — two-dimension vs. two-agent:** the single critic covers both dimensions; whether separate
  agents do better is unsettled by the literature. Deferred.

## Acceptance criteria

- [ ] `gatekeeper check review --feature <slug> [--base <ref>]` exists; in `--help` and the `//!` list.
- [ ] **Fresh pass:** clean tree, artifact `HEAD`==`git rev-parse HEAD`, `BASE`==computed merge-base,
      `pass`, `None.` blockers, both rubric dimensions present → exit `0`, `PASS review gate: <path>`.
- [ ] **Stale HEAD:** artifact `HEAD` ≠ current → exit `1`.
- [ ] **Dirty worktree:** uncommitted/untracked changes **outside `docs/reviews/`** → exit `1`.
- [ ] **Artifact doesn't self-dirty:** the freshly-written, untracked review artifact under
      `docs/reviews/` does **not** by itself fail the clean-tree check (the normal workflow passes).
- [ ] **Wrong base:** artifact `BASE` ≠ computed merge-base → exit `1`.
- [ ] **Not a repo / unresolvable `--base`** → exit `1` with a clear message.
- [ ] **Ambiguity:** two artifacts for the slug both naming current `HEAD` → exit `1`, prints all paths.
- [ ] **Abbreviated SHA:** a 7-char `HEAD:`/`BASE:` → exit `1` (full SHA required).
- [ ] Line-1 `VERDICT: fail` → exit `1`.
- [ ] **Pass with blockers:** `VERDICT: pass` but a blocking list item present → exit `1` (fail-closed).
- [ ] **Fail-closed line 1/2/3:** non-verdict line 1, malformed `HEAD:`/`BASE:` → exit `1`.
- [ ] **Heading count:** zero or >1 `## Blocking findings` → exit `1`.
- [ ] **Missing dimension:** no `## Criteria checked`, or missing `### Spec/plan` or `### Standards`,
      or an empty subsection → exit `1`.
- [ ] **Comment in parsed region:** an HTML comment (incl. unclosed `<!--`) in header or blocking
      section → exit `1`.
- [ ] **No false-fail on honest quoting:** a pass review quoting `VERDICT: pass` in a non-blocking
      note still passes (line 1 sole authority).
- [ ] BOM / CRLF / trailing-whitespace on the header handled per the lexical rules.
- [ ] **Sample-template validity:** the pass and fail examples in this spec — with their literal
      example SHAs — each parse to their stated verdict (regression guard against the R2 invalid
      template; the fail example uses literal SHAs, not `<full sha>` placeholders).
- [ ] Missing `--feature` → exit `2`.
- [ ] `cargo test` covers every case above, **including invoking the gate from a nested
      subdirectory** to prove git runs with `-C <framework_root>` (not process cwd).
- [ ] `skills/code-review/SKILL.md` exists: mandates fresh-subagent dispatch (different model where
      possible); defines the 3-line header + body grammar; requires `file:line` per blocking finding;
      mandates and documents the **two-dimension rubric**; instructs seek-to-FAIL and skip
      tooling-enforced checks; forbids comments/raw-diff in parsed regions; writes the artifact
      atomically (temp-file-then-rename).
- [ ] `hooks/skill-rules.json` routes `code-review` (`require`) on "review", "audit", "critique",
      "before merge".
- [ ] `verify-before-done` and `finish-branch` reference the `verify → review → finish` order;
      `README.md` gate table, `METHODOLOGY.md` sequence, `ROADMAP.md` updated (pulled forward from
      Phase 5 with a rationale).
- [ ] A new ADR records: (a) artifact-bound-to-clean-commit+merge-base contract; (b) fail-closed
      grammar + not-reusing-`strip_comments`; (c) single two-dimension critic, deferring multiple
      agents; (d) pulling the gate ahead of security scanning.

## Research basis

Deep-research pass, 2026-06-05 (5 angles, 21 sources, 25 claims 3-vote-verified, 13 confirmed).
Local resource: Matt Pocock's `review` skill (`/Users/hugues_mini/Codes/hardSkills`).

**Confirmed (high):** *fresh-context critic* (Zheng et al., NeurIPS 2023, arXiv:2306.05685;
Panickssery et al., NeurIPS 2024, arXiv:2404.13076; Xu et al., 2024, arXiv:2402.11436);
*verdict+blocker gate / corroborate-don't-trust-one-verdict* (arXiv:2306.05685; Maloyan et al.,
arXiv:2505.13348); *prompt-injection is the dominant threat*, 30–73.8% ASR on a code-review task
(Maloyan & Namiot, arXiv:2504.18333; arXiv:2505.13348; securityweek / coesecurity, 2025).

**Refuted (don't cite as support):** *multiple parallel agents catch more* (arXiv:2511.16708 0-3;
arXiv:2403.14274 0-3) — ship a single critic; this refutes multiple *agents*, not the two
*dimensions*. *Specific mitigation magnitudes* — cite only direction.

**Caveats:** 2023-era bias magnitudes have shrunk in modern models (direction holds). Code
self-review evidence is thinner than general-text. Diff-scoping returned no verified claim — the
merge-base choice rests on convention + branch-scope reasoning.

## Cross-model review (GPT-5.5, 2026-06-05)

Two adversarial, read-only, different-model passes — the spec's own thesis dogfooded.

**R1 → R2** (8 blockers, all accepted): HEAD-binding; strict grammar; dropped duplicate-verdict;
`finish` factual fix; restored Standards dimension; expanded acceptance criteria.

**R2 → R3** — re-review confirmed **5/8 resolved** (blocking-heading count, `strip_comments`
rejection, blocker grammar, `finish` correction, acceptance coverage) and raised **7 deeper issues**,
6 accepted into R3 and 1 accepted-as-residual:
1. Clean-worktree bind (dirty tree no longer passes).
2. Machine-checked merge-base (`BASE` is now verified, not informational).
3. Full SHA required (abbreviated would false-fail).
4. Pass ⇔ `None.` blockers (no trusting checkbox state).
5. Fixed the invalid sample template; added a template-validity acceptance test.
6. Gate-enforced both rubric dimensions (an empty review now fails).
7. Git run from `framework_root` (no cwd/submodule drift).
8. *(residual)* Line-1 injection by a fully-subverted critic — undetectable by any parser; documented
   in the threat model with the cross-model + voting reducers.

**R3 → R4** — final verification confirmed **6/7 R3 fixes hold** and the residual is reasonable, and
caught **one genuine design blocker** plus two minor items, all folded in:
- *Design blocker:* the **clean-tree / current-HEAD paradox** — as specified, writing the artifact
  dirtied the tree and committing it moved `HEAD`, so the gate was unsatisfiable. Resolved: `HEAD:`
  binds to the reviewed code commit, and the clean-tree check excludes `docs/reviews/`; the untracked
  artifact is an expected transient input (see *Workflow & ordering*).
- *Minor:* the fail example now uses literal SHAs so it parses; added a nested-directory test for
  `git -C <framework_root>`.

This 4-round cross-model loop is **converged**: design holes (R1→R2) → binding completeness
(R2→R3) → a single workflow-satisfiability blocker (R3→R4). Remaining items are
implementation/test-time. The design is ready to implement.
