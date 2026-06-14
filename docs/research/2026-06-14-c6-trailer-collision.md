# Research: C6 — Co-Authored-By × approval_provenance trailer collision

- **Date:** 2026-06-14
- **Feature slug:** c6-trailer-collision
- **Origin:** Slice #2 of the portability-first experiment (field report "Topology not worth it";
  the multi-perspective workflow flagged C6 as a *real, unflagged policy contradiction* worth a
  discrete bug fix — "add a `gatekeeper doctor` check that detects a standing 'always add
  Co-Authored-By' harness rule colliding with `approval_provenance`").

## Sub-questions

1. What is the exact mechanism of the design-gate `approval_provenance` check?
2. Where does the colliding "always add Co-Authored-By" rule actually live, and is the collision live?
3. What can a `gatekeeper doctor` check actually *observe* to detect it?
4. What is the doctor probe structure, output/exit convention, and test convention?
5. What reusable git/trailer helpers exist, and what is protected?

## Findings (cited; verified against the working tree)

### 1. The `approval_provenance` mechanism

`design_check_human_commit` (`gatekeeper/src/main.rs:1478-1762`) implements the design-gate approval
provenance check. It runs only after `spec_is_approved` already passed (`main.rs:1262-1276`). Steps:

1. `git show HEAD:<relpath>` reads the **committed** spec text (`main.rs:1604-1611`).
2. Scans that text for the 1-based line number of the first `Status: approved` line (`main.rs:1628-1657`).
3. `git log -L<n>,<n>:<relpath> --format=%H` finds commits that touched the approval line; the SHA is
   the first 40-char ASCII-hex output line (`main.rs:1659-1701`).
4. `git show -s --format=%(trailers) <sha>` reads that commit's trailers (`main.rs:1704-1713`).
5. **Matching** (`main.rs:1731-1759`): for each trailer line, lowercase it, keep **only** lines whose
   key is `co-authored-by:`, take the value after the 15-char prefix (trimmed), and test it against each
   `agent_trailer_patterns` regex (`regex::Regex::new(p)`, compiled fresh). **Any match → FAIL.**

- **PASS** iff no `Co-Authored-By` value matches a pattern (`main.rs:1761`). SHADOW detail: *"approval
  commit has no agent trailer"*.
- **FAIL** iff a `Co-Authored-By` value matches (`main.rs:1738-1749`). The only FAIL path. The message
  itself frames it as *"a residual risk for sycophantic self-approval, not a claim about operator
  intent"* (`main.rs:1746-1747`).
- **SKIP** on any obstacle (git < 2.15, shallow clone, untracked/dirty spec, unreadable trailers,
  invalid pattern regex) — `main.rs:1484-1601,1714-1727,1751-1756`.

**Enforcement is mode-gated.** `[design] approval = "human-commit"` makes Fail/Skip return exit 1
(`main.rs:1339-1382`). The **default `"status-line"`** runs the same check but emits SHADOW only — it
does **not** affect the exit code (`main.rs:1383-1403`; default at `config.rs:143-148,203`).

**Key narrowness:** the check looks **only** at the `Co-Authored-By:` trailer key (`main.rs:1734`) —
not author/committer identity, not the subject, not other trailers. Default `agent_trailer_patterns`
are 8 case-insensitive substring regexes: `(?i)claude`, `copilot`, `cursor`, `codex`, `gemini`,
`devin`, `aider`, `\[bot\]` (`config.rs:165-176`).

### 2. Where the colliding rule lives — and that the collision is live

The "always add `Co-Authored-By: Claude …`" directive is a **harness-injected** instruction (the
agent's system prompt: *"End git commit messages with: Co-Authored-By: Claude Opus 4.8 (1M context)
<noreply@anthropic.com>"*). It is **not present in any file on disk** that gatekeeper could read:

- This repo's `CLAUDE.md`, `AGENTS.md`, `.topology/CONTRACT.md`, `README.md`: **no** Co-Authored-By
  mandate (grep, no match).
- The global `~/.claude/CLAUDE.md`, `RTK.md`, `rules/*`: **no** Co-Authored-By mandate (grep, no match).

**The collision is live in this repo's history right now.** All 6 most-recent commits carry
`Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>` (`git log -6
--pretty=%(trailers)`). The value matches `(?i)claude`. So under `approval = "human-commit"`, a human
who made any of these the approval commit would be read as an agent self-approval and **FAIL** the
design gate.

> **Decisive constraint for the fix:** because the rule is harness-injected, a doctor check that only
> *greps instruction files* would **false-negative in exactly the canonical setup** (Claude Code with
> the default trailer rule) — the very setup that bit the field-report developer. The robust observable
> is **the git history**, not the instruction files.

### 3. What a doctor check can observe

A doctor probe runs at setup/health-check time and can:

- Read typed config (`design_approval`, `design_agent_trailer_patterns`).
- Shell out to git and inspect recent commit trailers — the same data the gate reads, but across recent
  HEAD history rather than a (possibly not-yet-existing) approval commit.

It **cannot** reliably read the harness rule (system-prompt-only). It **cannot** check "the approval
commit" on a fresh project (no approved spec exists yet) — which is exactly when prevention matters.
Therefore the highest-value observable is: *"are commits in this repo being stamped with an agent
`Co-Authored-By` trailer that the design gate would reject?"* If yes **and** `approval = "human-commit"`,
the next human approval commit will collide.

### 4. Doctor structure, output/exit, test conventions

`cmd_doctor(root, source) -> i32` (`doctor.rs:79`) runs each probe inline, accumulating
`let mut failures = 0usize` (`doctor.rs:81`); returns 0 iff `failures == 0`, else 1 (`doctor.rs:386-392`).

- **Output convention:** plain `println!`, **no emoji/color**. Bare ASCII tags embedded in the line:
  `"<label>: ok"`, `"<label>: FAIL: <reason> (<hint>)"`, `"<label>: WARN: <reason> (<hint>)"`,
  `"<label>: n/a (<why>)"`.
- **Exit policy:** only a `failures += 1` affects the exit code. **`WARN`/`n/a`/INFO never fail** —
  advisory only (`probe_settings_paths` is the WARN template, `doctor.rs:701-776`, doc: *"never
  increments doctor's failure count (advisory, not a gate)"*).
- **Advisory probe template:** `fn probe_<thing>(root: &Path) { … println!("…: ok"|"…: WARN: …"|"…:
  n/a …") }` → `()`, registered as a bare call alongside the other advisory probes (after
  `doctor.rs:383`).
- **Typed config access:** doctor does *not* load `ProjectConfig` today (it reads raw TOML in
  `probe_config_unknown_keys`). A new probe can `crate::config::ProjectConfig::load(&crate::artifacts_root())`
  (`config.rs:224`) after adding `use crate::config;` (`doctor.rs:8-17`). `[design]` keys `approval` and
  `agent_trailer_patterns` are already in `KNOWN_DESIGN_KEYS` (`doctor.rs:412`) — **no new config key**.
- **Test convention:** extract the decision into a **pure named fn** and unit-test that directly (mirror
  `version_skew`, `doctor.rs:74,839-865`); fs-touching tests use the
  `env::temp_dir().join("topology_doctor_<name>")` + `remove_dir_all`/`create_dir_all` idiom. No test
  captures `cmd_doctor` stdout.

### 5. Reusable helpers and protection boundary

- **No shared git-capture helper.** `review.rs:188`, `tdd.rs:14`, `memory.rs:36` are three byte-identical
  private `fn git -> Option<String>` copies; `scan.rs:1510` is a 4th (bytes). doctor already shells out
  to git inline (`doctor.rs:516`). A new probe adds its own small inline `git` call (house pattern).
- **The trailer-matching loop is inlined in `main.rs`, not factored out** (`main.rs:1731-1759`). To stay
  DRY *across the gate and the probe* would require editing `main.rs` — which is **protected**
  (`security/rules.toml:223`, integrity). The probe will instead replicate the ~6-line match against the
  **same** `design_agent_trailer_patterns` config (patterns cannot drift; only mechanics, pinned by a
  unit test).
- **Protection boundary (verified `security/rules.toml:172-230`):** `doctor.rs` and `config.rs` are
  **not** in integrity `protected_paths` (only `scan.rs`, `main.rs`, `Cargo.*`, hooks, rules, settings,
  install). So Edit-tool changes and commits to `doctor.rs`/`config.rs` need **no `--no-verify`**. The
  path-mutation `tamper-security-wiring` rule (`protected = ["gatekeeper/src/", …]`) blocks only *Bash*
  mutations into `gatekeeper/src/` (`cp`/`mv`/`>`), never the Edit tool or `cargo`/`git add`.

## Top-risk verification

- Re-read the FAIL/PASS/SKIP branches at `main.rs:1731-1762` directly — confirmed the only FAIL is a
  `Co-Authored-By` value matching a pattern; everything else is PASS or SKIP.
- Confirmed default `design_approval = StatusLine` three ways (`config.rs:143-148` `#[default]`,
  `:203` default struct, `:295-304` parse fallback) → the collision is **shadow** in default mode and
  **blocking** only under opt-in `human-commit` (which the roadmap plans to make the default,
  `docs/plans/2026-06-11-five-failure-modes-roadmap.md:110`).
- Reproduced the live collision via `git log -6 --pretty=%(trailers)` (all 6 carry the Claude trailer).

## Open unknowns (carried to design)

- **Detection strategy:** config-only vs file-scan vs git-history-empirical vs check-existing-specs —
  each with a false-positive / false-negative profile (esp. the harness-injected false-negative of
  file-scan). To be decided in design.
- **Scope/mode gating:** WARN always, or only under `approval = "human-commit"` (where it actually
  blocks)? (Leaning: only the load-bearing mode, per *weakest-enforcement-that-works*.)
- **History window:** HEAD-only vs last-N (merge commits can mask HEAD); and whether to dedupe by which
  commits are "plausibly human."
- **Drift control:** inline the ~6-line matcher in `doctor.rs` (pinned by test) vs extract a shared
  helper into `config.rs` (single caller → simplicity tension). To be decided in design.
