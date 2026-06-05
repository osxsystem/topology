# Research — Resources for Building Topology

- **Date:** 2026-06-04
- **Status:** Research artifact (research-first gate). Informs the [roadmap](../ROADMAP.md); no code.
- **Method:** A `deep-research` workflow (106 agents; 24 sources fetched → 119 claims → 25 adversarially verified, 3-vote, 0 killed) covering angles 1–2, then a **targeted gap-fill** (direct WebFetch + `context7` on current library docs) for angles 3–5, which the workflow surfaced but left unverified.
- **How to read confidence:** each finding is tagged **[verified N-0/2-1]** (adversarially vote-verified in the workflow), **[direct]** (single-source fetch, not vote-verified), or **[context7]** (current primary library docs). Weight them accordingly.

> **Scope honesty.** The workflow's adversarial pass only produced surviving claims for angles 1–2. Angles 3–5 here rest on direct fetches and context7, which are credible (mostly primary sources) but did **not** go through 3-vote verification. Treat angle 1–2 facts as hardened and angle 3–5 facts as well-sourced-but-single-pass.

---

## 1. Executive summary

Topology's design is **well-supported by prior art and primary sources**, with three load-bearing conclusions:

1. **The Markdown-source premise is standard, not novel.** [Agent Skills / `SKILL.md`](https://www.anthropic.com/engineering/equipping-agents-for-the-real-world-with-agent-skills) (progressive disclosure, two required frontmatter fields) and the [AGENTS.md](https://agents.md/) open standard (Linux Foundation Agentic AI Foundation) are exactly Topology's "author once in Markdown" model. **AGENTS.md is read natively by 3 of 4 targets** — Codex, Cursor, OpenCode — but **not Claude Code** (which reads `CLAUDE.md`). [verified 3-0]
2. **A near-twin already exists: [`affaan-m/ECC`](https://github.com/affaan-m/ECC)** ("Everything Claude Code"), whose self-description — *"Skills, instincts, memory, security, and research-first development for Claude Code, Codex, Opencode, Cursor"* — matches Topology's six pillars and four harnesses almost verbatim, including the phrase "harness-native operator system." It proves the manifest-driven, author-once/generate-native pipeline works. **But its stack is Bash/Python/JS, not Rust** — so it's an architecture/vocabulary comparable, not a code template. [verified 3-0]
3. **The security pillar cannot be uniform across harnesses.** OpenCode's only pre-tool-use veto is a **JavaScript/TypeScript plugin hook**, so Topology's codegen *must* emit a JS shim for OpenCode. And every harness has documented veto **coverage gaps** that a "gates, not rules" engine must defend against explicitly (below). [verified 3-0]

The Rust implementation is also well-served: the **`regex` crate's `RegexSet`** matches many scan rules in one linear-time pass, and the **`toml` crate** parses the `[[rule]]` config natively (and can serialize it back for the learn/promote loop).

---

## 2. Angle 1 — Comparable frameworks & the two standards

### The standards Topology builds on
- **`SKILL.md` / Agent Skills** — a skill is a directory whose `SKILL.md` starts with YAML frontmatter requiring exactly **`name` + `description`** (optional: `license`, `compatibility`, `metadata`, `allowed-tools`). **Progressive disclosure has three levels**: metadata pre-loaded at startup (~100 tokens/skill) → full `SKILL.md` body loaded on relevance → bundled files on demand. This is precisely the discovery/loading mechanism Topology's `gatekeeper activate` replicates. [verified 3-0] — [Anthropic](https://www.anthropic.com/engineering/equipping-agents-for-the-real-world-with-agent-skills), [spec](https://agentskills.io/specification), [Claude docs](https://platform.claude.com/docs/en/agents-and-tools/agent-skills/overview)
- **AGENTS.md** — plain Markdown, **no required fields**, arbitrary headings; nested files combine with parents (more-specific wins). Read natively by Codex, Cursor, OpenCode, plus Gemini CLI, Copilot, Aider, goose, Zed, Warp, Jules, Devin, Windsurf, Amp (60k+ repos). **Claude Code is the exception** (`CLAUDE.md`). [verified 3-0] — [agents.md](https://agents.md/)

### Prior-art systems (adopt / avoid)

| System | What it is | Adopt for Topology | Avoid / contrast |
|---|---|---|---|
| **[Superpowers](https://github.com/obra/superpowers)** (obra) [verified 3-0] | Complete methodology of composable, auto-triggering skills; ships natively to all 4 targets since v5.0 | The multi-harness skill-distribution model; composable auto-triggering skills | Its **mandatory** 7-stage workflow is the explicit foil to Topology's lighter "gates, not rules" |
| **[Spec-Kit](https://github.com/github/spec-kit)** (GitHub) [verified 2-1/3-0] | `specify init --ai <agent>` writes agent-specific configs for 30+ agents | **The proof-of-pattern for `gatekeeper adapt`** — author once, generate native per harness (distinct dirs: `.claude/skills`, `.cursor/rules/*.mdc`, Codex `.agents/skills`+`AGENTS.md`, opencode) | Dir layouts drift across versions — pin per installed version |
| **[ECC](https://github.com/affaan-m/ECC)** (affaan-m) [verified 3-0] | Near-twin: same 6 pillars, same 4 harnesses; one `manifests/install-modules.json` → per-harness adapters; one shared hook-script layer mapped to each harness's event names | **The closest architectural blueprint** — study its manifest→adapter pipeline and shared-script hook mapping | **Stack is Bash/Python/JS, not Rust**; star/commit metrics are unreliable (partly hallucinated) — don't cite popularity |

Others named but not separately verified: BMAD-METHOD, agent-os, claude-flow, OpenSkills, awesome-claude-code. Treat as leads, not findings.

---

## 3. Angle 2 — Per-harness integration surface (hardened)

### Integration matrix

| Harness | Config (path/format) | Instructions | Pre-tool-use veto | Notable | Source |
|---|---|---|---|---|---|
| **Claude Code** | `settings.json` | `CLAUDE.md` (not AGENTS.md) | **(A)** exit 0 + JSON `hookSpecificOutput.permissionDecision:"deny"` (+reason); **(B)** **exit code 2** (stderr → Claude). Pick one per hook | Recommended: pair JSON-deny with exit 2 | [hooks docs](https://code.claude.com/docs/en/hooks) [verified 3-0] |
| **Codex CLI** | `~/.codex/config.toml`; project `.codex/config.toml` **only when trusted** (TOML) | `AGENTS.md` (`project_doc_max_bytes` def 32768) | Lifecycle hooks (`PreToolUse`, `PermissionRequest`, …) via `[hooks]`/`hooks.json` | MCP under `[mcp_servers.<id>]` (stdio/http) | [config](https://developers.openai.com/codex/config-reference), [hooks](https://developers.openai.com/codex/hooks) [verified 3-0/2-1] |
| **Cursor** | `.cursor/rules/*.mdc` (**`.mdc` required** — plain `.md` is ignored) | also reads root `AGENTS.md` | **None** (static rules; no exec hooks) | `.mdc` frontmatter = exactly `description`, `globs`, `alwaysApply` → 4 modes | [rules](https://cursor.com/docs/rules) [verified 3-0] |
| **OpenCode** | `opencode.json(c)` | `AGENTS.md` (+ global `~/.config/opencode/AGENTS.md`) | **JS/TS plugin only** — `tool.execute.before`, veto by `throw` | Plugins in `.opencode/plugins/`, registered in `opencode.json` | [plugins](https://opencode.ai/docs/plugins/) [verified 3-0] |

### ⚠️ Veto coverage gaps — load-bearing for the "gates, not rules" security pillar
A gatekeeper **cannot assume uniform veto coverage**:
- **Claude Code**: `PreToolUse` deny is **not honored for MCP-server tools** ([#33106](https://github.com/anthropics/claude-code/issues/33106), closed not-planned) and allegedly not for `Edit` (#37210); built-in Bash/Read/Write **do** enforce it. [verified 3-0]
- **Codex**: `PreToolUse`/`PostToolUse` **do not fire for `apply_patch` edits or most MCP calls** ([codex#16732](https://github.com/openai/codex)); command hooks work, prompt/agent handlers are "parsed but skipped." [verified 2-1]
- **OpenCode**: `tool.execute.before` **does not intercept subagent/task-tool calls** ([opencode#5894](https://github.com/anomalyco/opencode/issues/5894), open) — a documented policy-bypass. [verified 3-0]

**Implication:** the security pillar is strongest as **defense-in-depth** — the pre-tool-use hook is best-effort per harness, so back it with the deterministic **pre-commit** hook (which every harness shares via git) as the real floor.

### Author-once → generate-native
Confirmed feasible and demonstrated by **Spec-Kit** and **ECC**. The pattern: one Markdown/manifest source → a generator (`gatekeeper adapt`) emits each harness's native files. **The one place pure codegen is insufficient is OpenCode's veto**, which needs a generated JS/TS plugin shim (Rust/Markdown can't express it).

---

## 4. Angle 3 — Security scanning (gap-fill: direct + context7)

### Secret scanners

| Tool | Lang | Verification | Speed (Sentry repo) | Pre-commit | License | Fit for Topology |
|---|---|---|---|---|---|---|
| **[ripsecrets](https://github.com/sirwart/ripsecrets)** | **Rust** | probability model (flags if P(random) < 1/10,000) | **0.32s** | `--install-pre-commit` | — | **Best Rust-native fit**: single static binary, no deps, non-zero exit on find — mirrors `gatekeeper`'s own shape [direct] |
| **gitleaks** | Go | none (pattern) | <1s on diffs | yes | MIT | Strong default if shelling out; fast diff + CI [direct] |
| **TruffleHog** | Go¹ | **700+ types via live API checks** | 31.2s | yes | MIT | Best for periodic *verified* full-history sweeps, not per-keystroke [direct] |
| **detect-secrets** | Python | none; baseline allowlist | 73.5s | yes | Apache-2.0 | Good baseline/allowlist onboarding; too slow for pre-tool-use [direct] |

¹ One secondary [comparison blog](https://devsecops.ae/secrets-scanners-comparison-2026/) mislabeled TruffleHog as Python — it is Go. Flagged to show why single-source angle-3 facts aren't hardened.

**Recommendation:** for Topology's Rust + "single fast binary" ethos, **ripsecrets** is the natural reference for the secret-detection slice (or vendor its probability approach into `gatekeeper scan`); keep **gitleaks** as an optional shell-out for teams that already standardize on it.

### Beyond secrets — dangerous commands & prompt injection
- **[LlamaFirewall](https://arxiv.org/abs/2505.03574)** (Meta, arXiv:2505.03574, May 2025) is the reference open guardrail system: **PromptGuard 2** (jailbreak/prompt-injection detection), **Agent Alignment Checks** (CoT auditor), and **CodeShield** (online static analysis for dangerous code). Crucially, it emphasizes *"developer extensibility through customizable scanners for regex patterns"* — validating Topology's `security/rules.toml` rule-driven design. [direct]
- Dangerous-command detection (e.g. `rm -rf /`, pipe-to-shell, history rewrite) is regex/pattern work that fits the `RegexSet` engine (§6) and wires into the same pre-tool-use/pre-commit veto paths.

---

## 5. Angle 4 — Context engineering, memory & continuous learning (gap-fill)

These back **pillar 3 (memory optimization)** and **pillar 4 (continuous learning)**.

- **[Effective context engineering for AI agents](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents)** (Anthropic) — treat context as a finite budget; load just-in-time. This is the primary source for progressive disclosure beyond skills. [direct/primary]
- **[Effective harnesses for long-running agents](https://www.anthropic.com/engineering/effective-harnesses-for-long-running-agents)** (Anthropic, Nov 26 2025) — concrete memory protocol Topology's `memory/` pillar should copy: an **initializer** writing `init.sh` + a **progress log** + a **feature-list JSON** with pass/fail status; **git history as durable memory**; **compaction is "not sufficient" alone** for multi-window work; an **orientation protocol** (read logs/progress/git before acting); and *use **JSON not Markdown** for specs* because it's "more resistant to inappropriate model edits." [direct/primary]
- **[Reflexion](https://arxiv.org/abs/2303.11366)** (Shinn et al., 2023) — the academic backbone for pillar 4: agents **verbally reflect on failure** and store reflections in an **episodic memory buffer** to improve on later trials **without weight updates** (91% pass@1 on HumanEval vs GPT-4's 80%). This *is* Topology's capture-gotcha → promote loop, with primary-source evidence it works. [direct/primary]

**Synthesis:** Topology's "capture failure → promote into an operator" is a Reflexion-style episodic loop made *durable and shared* (written to `docs/learn/`, promoted to a versioned instinct/skill/rule), and its `memory/` protocol should follow Anthropic's long-running-agents recipe (progress file + git history + JSON state, with compaction as a supplement, not the plan).

---

## 6. Angle 5 — Rust ecosystem for the gatekeeper (gap-fill: direct + context7)

| Concern | Finding | Recommendation for `gatekeeper` |
|---|---|---|
| **Arg parsing** | clap is the recommended default but "pulls in several dependencies… increases binary size significantly"; **pico-args** = "zero dependencies, quick to compile, negligible binary size" (no help-gen); lexopt for hand-rolled; guide says don't hand-roll *unless necessary* [direct, [sunshowers](https://rust-cli-recommendations.sunshowers.io/cli-parser.html)] | gatekeeper is currently **std-only** and tiny — **keep it** (or adopt **pico-args**) to preserve the "dependency-free static binary" property; reach for clap only if the subcommand surface grows large |
| **Regex / scanning** | `regex` crate: **`RegexSet` matches many patterns in ONE scan** and reports which indices matched; **guaranteed O(m·n) linear time**; **no backreferences or look-around** (deliberate); UTF-8 *and* byte-slice search [context7 `/rust-lang/regex`] | Build `gatekeeper scan` on **`RegexSet`** — all `security/rules.toml` patterns compiled once, matched in a single pass. **Design caveat:** rules can't use backrefs/lookaround; express secret patterns accordingly (or anchor + post-filter) |
| **Config (TOML)** | `toml` crate: serde `Deserialize` via `toml::from_str`; **native `[[rule]]` arrays-of-tables**; full Serde 1.0 incl. **serialization** [context7 `/websites/rs_toml`] | Use `toml` to read `security/rules.toml` and `instincts` frontmatter; its serialize path supports **`learn promote`** writing rules/operators back |
| **Binary size** | `[profile.release]`: `opt-level="z"`(or `"s"`), `lto=true`, `codegen-units=1`, `panic="abort"`, `strip=true` [direct, [min-sized-rust](https://github.com/johnthagen/min-sized-rust)] | gatekeeper already sets `opt-level="z"`, `lto`, `strip`; **add `codegen-units=1` + `panic="abort"`**. For Linux static distribution, build against **musl** (not covered by min-sized-rust; verify separately) |
| **CLI testing** | `assert_cmd` (+ `predicates`) for black-box CLI integration tests [report source [docs.rs/assert_cmd](https://docs.rs/assert_cmd)] | Add `assert_cmd` as a **dev-dependency** (keeps the runtime binary dep-free) for gate/scan end-to-end tests |
| **Token/cost tooling** | `tiktoken-rs` (Rust BPE tokenizer) for token counting [report source [crates.io/tiktoken](https://crates.io/crates/tiktoken)]; RTK already in this user's stack | Optional, for the memory-optimization pillar's budgeting; not core |

---

## 7. Decisions this research forces (mapped to the roadmap)

1. **[Phase 1 / security] Emit a JS/TS shim for OpenCode.** Pure Rust/Markdown codegen can't veto OpenCode tool calls; the `adapt` step must generate a `.opencode/plugins/*.ts` hook. *(New constraint vs. the blueprint, which assumed config-only adapters.)*
2. **[Phase 1 / security] Make pre-commit the real floor.** Because pre-tool-use veto coverage is non-uniform (MCP on Claude Code, `apply_patch` on Codex, subagents on OpenCode), the deterministic pre-commit hook is the dependable gate; pre-tool-use is best-effort defense-in-depth.
3. **[Phase 1 / security] Scanner core = `RegexSet`; rules can't use backrefs/lookaround.** Bake this into `security/rules.toml` rule authoring guidance. Reference ripsecrets' probability approach for variable-assignment secrets; LlamaFirewall for the prompt-injection/dangerous-code dimension.
4. **[Phase 4 / adapters] Study ECC and Spec-Kit before building `adapt`.** ECC's manifest→adapter + shared-hook-script layer and Spec-Kit's `init --ai <agent>` are working implementations of exactly this; adopt their structure, not their stack.
5. **[Phase 5 / memory] Copy Anthropic's long-running-agents protocol.** Progress file + git-history-as-memory + JSON state; compaction supplements, doesn't replace. Model pillar 4 on Reflexion's episodic loop.
6. **[Phase 6 / packaging] Keep the binary dependency-light.** Stay std-only or pico-args; add `toml`, `regex`, and dev-only `assert_cmd`; tune `[profile.release]`. Re-verify every harness's config paths/hook semantics against installed versions before generating native configs.

---

## 8. Caveats & time-sensitivity

- **Fast-moving surface (2025–2026).** Version-pinned facts that *will* drift: Codex hooks enabled-by-default vs flag-gated (sources conflict — verify against installed version); Cursor's new `RULE.md` folder format (2.2) vs `.mdc` (**use `.mdc`** for now); Spec-Kit's `.claude/commands`→`.claude/skills` migration; Superpowers/ECC feature sets. **Re-verify before codegen.**
- **Angles 3–5 not adversarially verified.** Credible (mostly primary) but single-pass. The TruffleHog-language error in one blog is the cautionary example.
- **Don't cite popularity metrics** for ECC/Superpowers — wildly inconsistent across sources, partly hallucinated.

---

## 9. Open questions (carry forward)

1. Which secret engine to *ship* — vendor ripsecrets' probability approach into `gatekeeper scan`, or shell out to gitleaks? (Decide in Phase 1.)
2. Exact dangerous-command + prompt-injection rule set, and how each wires into each harness's veto path given the coverage gaps.
3. `memory/` artifact format specifics (progress-file schema, JSON state shape) following the long-running-agents recipe.
4. musl/static-linking + cross-compilation matrix for one-binary distribution across macOS/Linux.

---

## 10. Sources

**Primary / verified (angles 1–2):** [Agent Skills](https://www.anthropic.com/engineering/equipping-agents-for-the-real-world-with-agent-skills) · [agentskills.io spec](https://agentskills.io/specification) · [Claude Skills docs](https://platform.claude.com/docs/en/agents-and-tools/agent-skills/overview) · [AGENTS.md](https://agents.md/) · [Superpowers](https://github.com/obra/superpowers) · [Spec-Kit](https://github.com/github/spec-kit) · [ECC](https://github.com/affaan-m/ECC) · [Codex config](https://developers.openai.com/codex/config-reference) · [Codex hooks](https://developers.openai.com/codex/hooks) · [Codex MCP](https://developers.openai.com/codex/mcp) · [Cursor rules](https://cursor.com/docs/rules) · [OpenCode plugins](https://opencode.ai/docs/plugins/) · [Claude Code hooks](https://code.claude.com/docs/en/hooks) · issues [claude-code#33106](https://github.com/anthropics/claude-code/issues/33106), [opencode#5894](https://github.com/anomalyco/opencode/issues/5894)

**Gap-fill (angles 3–5):** [ripsecrets](https://github.com/sirwart/ripsecrets) · [secret-scanner comparison 2026](https://devsecops.ae/secrets-scanners-comparison-2026/) · [LlamaFirewall (arXiv:2505.03574)](https://arxiv.org/abs/2505.03574) · [Effective context engineering](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents) · [Effective harnesses for long-running agents](https://www.anthropic.com/engineering/effective-harnesses-for-long-running-agents) · [Reflexion (arXiv:2303.11366)](https://arxiv.org/abs/2303.11366) · [Rust CLI parser recs](https://rust-cli-recommendations.sunshowers.io/cli-parser.html) · [min-sized-rust](https://github.com/johnthagen/min-sized-rust) · context7: [`/rust-lang/regex`](https://docs.rs/regex), [`/websites/rs_toml`](https://docs.rs/toml), [assert_cmd](https://docs.rs/assert_cmd)
