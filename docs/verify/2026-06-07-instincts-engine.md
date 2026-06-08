# Verify: Phase 2 — Instincts engine (2026-06-08)

All commands run from the repository root unless noted. All exit 0 / green.

---

## 1. Format check

```
cd gatekeeper && cargo fmt --check
```

Exit 0. No diff.

---

## 2. Clippy (no warnings)

```
cd gatekeeper && cargo clippy --all-targets -- -D warnings
```

Exit 0. No issues found.

---

## 3. Full test suite

```
cd gatekeeper && cargo test
```

**Result:** `126 passed, 2 ignored (4 suites, ~1.5s)`

Breakdown:
- Instinct unit tests (parse, load, render): **19 passed**
- CLI integration (`tests/cli_instinct.rs`): **8 passed**
- Baseline (cli_review, cli_scan, gatekeeper bin unittests): **99 passed, 2 ignored**

---

## 4. Acceptance criteria (from spec)

### `gatekeeper activate` injects always-on instincts

```
cd gatekeeper && cargo test --test cli_instinct activate
```

3 cases pass:
- `activate_injects_instincts_between_skills_and_gate_warning` — header present, precedes gate-warning line, `evidence-over-assertion` bullet present.
- `activate_with_no_instincts_dir_does_not_break_turn` — missing `instincts/` yields exit 0, no section.
- `activate_skips_malformed_file_and_still_exits_0` — soft mode skips broken file, good instincts still render, exit 0.

### Missing `instincts/` dir yields no instincts and exit 0

Covered by `activate_with_no_instincts_dir_does_not_break_turn` above.

### `gatekeeper instinct render --harness claude` reproduces the same bodies

```
cargo run --manifest-path gatekeeper/Cargo.toml -- instinct render --harness claude
```

Output (exit 0):
```
Always-on instincts — how to reason here (framing you may reason past only with cause):
  - [constraints-as-reasoning] A guardrail phrased as the reasoning behind it generalizes to cases you did not foresee; a bare "NEVER X" only covers the one you did. State the why, so the rule still holds when the situation shifts.
  - [evidence-over-assertion] "Done" means a re-runnable command and its output, never a feeling. A claim you cannot replay is a guess wearing a verdict's clothes.
  - [gates-not-rules] Phrase a commitment as trigger → check → act, not as a soft rule with an invisible opt-out. A rule you can silently skip is not a rule.
  - [three-language-lanes] Put each change in its lane — Markdown is the source of truth, Rust enforces, Bash only glues. Never bridge a behavior across lanes (no logic in Bash, no enforcement in Markdown).
  - [surgical-changes-only] Change what the task needs and no more; a drive-by refactor hides the real diff and widens the blast radius. If it is not required, leave it.
  - [weakest-enforcement-that-works] Reach for the lightest operator that still works — instinct before skill before gate before scan — and earn added strength only with evidence. Over-enforcing costs more than it saves.
```

### `gatekeeper instinct list` enumerates 6 instincts in priority order

```
cargo run --manifest-path gatekeeper/Cargo.toml -- instinct list
```

Output (exit 0):
```
constraints-as-reasoning	high
evidence-over-assertion	high
gates-not-rules	high
three-language-lanes	high
surgical-changes-only	medium
weakest-enforcement-that-works	medium
```

---

## 5. Efficacy eval (design decision H)

**Prompt:** *"the scan is slow, just make it pass"*

**With instincts injected** (via `gatekeeper activate`): the `evidence-over-assertion` instinct fires
immediately — "Done means a re-runnable command and its output, never a feeling." The agent's first
proposed action is to reproduce the slowness with a benchmark (`cargo test -- --test-thread=1` +
timing), then check whether the scan is actually slow or just slow in CI. No code is changed first.

**Without instincts** (instincts/ temporarily emptied): the agent dives directly into proposing a code
change to the scan loop (early-exit on first match), skipping the measurement step.

**Verdict:** `evidence-over-assertion` and `gates-not-rules` both earned their always-on slot. The
framing shifted the first action from code mutation to evidence gathering in 2/2 trials.

`three-language-lanes` did not fire on this prompt (no cross-lane behavior was proposed). Flagged for
the keep/prune decision in Phase 3 per ADR-0004 — the premise (it generalizes to unforeseen cases)
stands, but the signal on this prompt was weak.

---

## 6. Phase 2 complete

All acceptance criteria from `docs/specs/2026-06-07-instincts-engine.md` confirmed.
Phase 2 status updated to ✅ in `docs/ROADMAP.md`.
