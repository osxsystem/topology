# Topology

A **harness-native operator system** for building software with AI coding agents and humans *together* — a development methodology made executable. Built on **Rust** (the enforcement engine), **Bash** (hooks and install glue), and **Markdown** (skills, instincts, and the agent definition), and designed to run natively across **Claude Code, Codex, Cursor, and OpenCode**.

Topology is a starter you own. It ships a thin, opinionated methodology enforced by **gates** (objective, checkable conditions) rather than **rules** (soft suggestions agents quietly skip). Fork it, swap the gates for your own taste, and grow the skill library reactively.

Topology are structured frameworks that guide teams through building software from start to finish. They provide a step-by-step software development process for planning work, organizing teams, and delivering quality products.

A methodology serves as a structured framework that defines roles, responsibilities, timelines, and collaboration processes for your team. Without a clear methodology in place, teams frequently encounter challenges such as missed deadlines, ambiguous priorities, and compromised quality standards.

## Documentation

- **[docs/HOW-IT-WORKS.md](docs/HOW-IT-WORKS.md)** — a visual, end-user walkthrough: install once, then every task walks the gates (diagrams + ASCII fallbacks). Start here.
- **[docs/USER-GUIDE.md](docs/USER-GUIDE.md)** — the end-user manual: install/uninstall, the full `gatekeeper` command reference, a typical session, and future features.
- **[METHODOLOGY.md](METHODOLOGY.md)** — the operator methodology: the four operator types (instincts, skills, gates, scans), the six pillars, the gate sequence, and how it all maps to Anthropic's agent-building guidance.
- **[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)** — the layered design, control flow, and cross-harness fan-out (diagrams), plus the `gatekeeper` contract.
- **[docs/ROADMAP.md](docs/ROADMAP.md)** — the phased path from today's gates to the full system (the code-review gate landed early; security scanning is next).
- **[docs/EXTENDING.md](docs/EXTENDING.md)** — how a human or agent adds a skill, instinct, gate, or scan rule.
- **[docs/adr/](docs/adr/)** — the architecture decision records.
- **[docs/research/](docs/research/)** — research-first artifacts (e.g. [build-resources](docs/research/2026-06-04-build-resources.md): comparable frameworks, harness configs, security tooling, the Rust stack).
- **[docs/learn/rtk-proxy.md](docs/learn/rtk-proxy.md)** — RTK, the optional token-killer shell proxy: what it does, how the command-rewrite hook wires it transparently, meta commands, opt-in install, and the name-collision caveat.

## What's in here

```
topology/
├── AGENTS.md                  # the agent definition + bootstrap (portable across clients)
├── CLAUDE.md                  # symlink -> AGENTS.md (so Claude Code reads the same source)
├── .claude-plugin/
│   ├── plugin.json            # Claude Code plugin manifest
│   └── marketplace.json       # marketplace listing (source "./")
├── skills/                    # Markdown skills (the methodology + meta skills)
│   ├── _getting-started/
│   ├── brainstorm-design/
│   ├── write-plan/
│   ├── tdd-loop/
│   ├── systematic-debug/
│   ├── verify-before-done/
│   ├── code-review/
│   └── finish-branch/
├── gatekeeper/                # Rust CLI that enforces the gates
│   ├── Cargo.toml
│   └── src/main.rs
├── hooks/
│   ├── skill-activation.sh    # UserPromptSubmit hook: forces skill evaluation
│   └── skill-rules.json       # keyword/file -> skill routing
└── scripts/
    ├── install.sh             # build the binary + wire up the symlink/hooks
    └── new-skill.sh           # scaffold a new skill from the template
```

## The core idea: gates, not rules

A *rule* ("verify before asserting") has an invisible opt-out — the agent skips it when it feels confident. A *gate* is a hard block with an objective, testable criterion. The `gatekeeper` binary turns each methodology stage into a gate the agent (or CI) can check:

| Gate | Passes when |
|------|-------------|
| `design`  | an approved design doc exists at `docs/specs/<date>-<feature>.md` |
| `plan`    | a placeholder-free plan exists at `docs/plans/<date>-<feature>.md` |
| `tdd`     | the working tree has a committed failing-test-first history (heuristic) |
| `verify`  | a verification note exists for the feature |
| `review`  | a fresh-context critic's review artifact passes for the current clean `HEAD` (bound to merge-base, both dimensions, no blockers) |
| `finish`  | the full test suite passes (`gatekeeper check finish -- <cmd>`) |
| `scan`    | a deterministic veto on secrets + dangerous commands, before they run (`PreToolUse`) or commit (`pre-commit`); history is the strong net, the working-tree veto is partial |

## Quick start

```bash
cd topology
./scripts/install.sh          # builds gatekeeper, creates CLAUDE.md symlink, prints hook config
gatekeeper list               # list available skills
echo "add a users table" | gatekeeper activate   # see which skills route in
gatekeeper check design --feature add-users       # check a gate
```

## Install as a Claude Code plugin

Topology also ships as a Claude Code plugin. The `gatekeeper` binary is **not bundled** in the plugin — it's a prerequisite the hooks resolve on your `PATH` (or via `$GATEKEEPER_BIN`), so build it first, then add the marketplace and install:

```bash
./scripts/install.sh                          # builds gatekeeper (release) + wires hooks
/plugin marketplace add osxsystem/topology    # in Claude Code
/plugin install topology@topology
```

The plugin wires the same two hooks as a manual install — `UserPromptSubmit` → skill routing, `PreToolUse` → the security scan — via `${CLAUDE_PLUGIN_ROOT}`. Check the binary the hooks will use:

```bash
gatekeeper --version    # gatekeeper X.Y.Z (rules schema vN)
gatekeeper doctor       # read-only health check: which binary resolves, plus rules/skills/hooks status
```

`doctor` is what surfaces *which* `gatekeeper` the hooks resolve (`$GATEKEEPER_BIN` → `PATH` → repo build); the hooks themselves stay silent on success.

## Make it yours

1. **Pick your gates.** Topology gates on design docs + TDD. Swap in type-checking, an API contract, a coverage threshold — whatever fits your discipline. Edit `gatekeeper/src/main.rs`.
2. **Tune the voice.** The tone and strictness live in `AGENTS.md` and each `SKILL.md`.
3. **Choose your portability surface.** Claude-Code-only? Drop `AGENTS.md`/hooks complexity. Many clients? Keep one `SKILL.md` source and add per-platform packaging next to `.claude-plugin/`.

## Stack rationale

- **Rust** — the gatekeeper must be fast, deterministic, and safe to run on every prompt and in CI. A single std-only macOS-arm64 executable (dynamically links libSystem) is trivial to distribute across machines.
- **Bash** — hooks and install glue; the lowest-common-denominator that every agent harness can shell out to.
- **Markdown** — skills and the agent definition, so they're portable, diffable, and editable by humans and agents alike.
