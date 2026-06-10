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
| **Git** | the gates read history; the pre-commit hook guards commits | a real `.git` repo |
| **An AI client** | runs the hooks | Claude Code, Codex, Cursor, or OpenCode |
| **Rust toolchain** (`cargo`) *(optional)* | builds `gatekeeper` from source | only needed without a prebuilt binary; install from <https://rustup.rs> |
| **RTK** *(optional)* | token-saving shell proxy | see [docs/learn/rtk-proxy.md](learn/rtk-proxy.md) |

---

## Installation

### Option A — One-command install (no Rust required)

```bash
curl -fsSL https://raw.githubusercontent.com/osxsystem/topology/main/scripts/install.sh | bash
```

The installer **asks** (when a terminal is available) which scope and harness to use. Without a tty
(piped, CI) it prints the defaults it assumed and the flags that override them.

**Installer flags:**

| Flag | Default | Effect |
|---|---|---|
| `--global` | ✓ | Install at `${TOPOLOGY_HOME:-~/.topology}` (shared across projects) |
| `--project <path>` | — | Vendor at `<path>/.topology` and wire that project; mutually exclusive with `--global` |
| `--harness <h>` | ask / `claude` | Wire `claude`, `codex`, `cursor`, `opencode`, or `none` |
| `--yes` | — | Accept all defaults non-interactively |
| `--build-from-source` | — | Build gatekeeper from source instead of downloading |

This one command:

1. **Acquires the framework** into `${TOPOLOGY_HOME:-$HOME/.topology}` for global scope (clones the repo, or pulls if already present), or for local scope downloads the **distribution payload** tarball — a curated, checksum-verified snapshot of just the operators (hooks, skills, instincts, scan rules, `scripts/fetch-gatekeeper.sh`, `AGENTS.md`, `VERSION`) — and unpacks it at `<path>/.topology`. Re-running the installer upgrades a payload install in place; a `VERSION` file at `<path>/.topology/VERSION` records the installed version so `gatekeeper doctor` can detect binary↔payload skew. If an older clone-based install is found at `<path>/.topology`, the installer rescues any in-tree state (learn ledger, memory handoffs) to `<path>/.claude/topology/` before prompting to replace.
2. **Downloads** the prebuilt `gatekeeper` binary for your platform, verifies its SHA-256 checksum,
   smoke-tests `--version`, and places it at `$ROOT/bin/gatekeeper`.
3. **Falls back** to `cargo build --release` if no prebuilt binary is available for your platform.
4. **Links** `CLAUDE.md → AGENTS.md` so Claude Code reads the same operating contract as every other harness.
5. **Marks** the hook and helper scripts executable.
6. **Installs the git `pre-commit` hook** (a *copy* of `hooks/pre-commit.sh` — re-run install to update it). For `--project` installs the copy goes into the **project's** `.git/hooks/`, the repo you actually commit to — not the vendored clone's.
7. **Wires the harness** — for `--project` installs runs `gatekeeper adapt --harness <h>` from the project dir, generating `.claude/settings.json` (or the equivalent for other harnesses) with hook paths pointing at the framework. For global-only installs it prints the exact command to run inside any project.
8. **Appends `.topology/` to `<project>/.gitignore`** (local scope only) if not already present. **Commit these wiring files** (`git add .claude/settings.json .gitignore && git commit -m "chore: wire topology governance"`) before your first review gate run — the gate's cleanliness check requires a clean working tree and will fail with "uncommitted changes" if the installer's own files are still untracked. The installer prints an exact hint (or offers interactively with a tty) at the end of its output.
9. **Detects stale PATH binaries** — if a `gatekeeper` on PATH has a different version, with a tty you're offered an in-place overwrite (`cp`); without one, a warning names the path and both versions.
10. **Prints a manifest** of every file created or modified, then runs `gatekeeper doctor` as a live health check — from the *project* directory for `--project` installs, so the check validates the layout your sessions will actually run in.

If you already have a checkout, run `./scripts/install.sh` from inside it — the script detects the checkout via `BASH_SOURCE` and builds the payload locally rather than downloading it. Pass `--build-from-source` to skip the prebuilt gatekeeper download and always build from source (requires Rust; not supported in piped mode for local installs, since no source tree is available).

For `--harness none`, the installer prints the hook config block — paste it into `.claude/settings.json` *inside the repo*:

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

### Gate artifact layout for governed projects

When governing an external project — any repo that isn't the framework checkout itself, whether the
framework is global (`~/.topology`) or vendored (`<project>/.topology`) — gate artifacts live under
**`<project>/.claude/topology/`**, not in the project's root `docs/`. The framework repo itself
keeps its root `docs/` layout unchanged.

| Artifact type | Location in governed project | Location in the framework repo |
|---|---|---|
| `research/` | `.claude/topology/research/` | `docs/research/` |
| `specs/` | `.claude/topology/specs/` | `docs/specs/` |
| `plans/` | `.claude/topology/plans/` | `docs/plans/` |
| `verify/` | `.claude/topology/verify/` | `docs/verify/` |
| `reviews/` | `.claude/topology/reviews/` | `docs/reviews/` |
| memory handoffs | `.claude/topology/memory/` | `docs/memory/` |
| learn ledger | `.claude/topology/learn/ledger.md` | `docs/learn/ledger.md` |

The last two rows are why upgrades are safe: the vendored framework is **read-only at runtime**
(ADR-0013) — everything `gatekeeper` writes lands in the project's committed artifacts root, never
inside `.topology/`, so replacing the payload on upgrade can't delete your handoffs or learned
gotchas.

**One-time migration** (if existing gate artifacts live in a governed project's root `docs/`):

```bash
mkdir -p .claude/topology
git mv docs/research  .claude/topology/research
git mv docs/specs     .claude/topology/specs
git mv docs/plans     .claude/topology/plans
git mv docs/verify    .claude/topology/verify
git mv docs/reviews   .claude/topology/reviews
git commit -m "chore: migrate gate artifacts to .claude/topology/"
```

### Stale-PATH repair

After install, if a `gatekeeper` binary on PATH has a different version (e.g. an old `~/.cargo/bin/gatekeeper` from a previous `cargo install`):

- **With a tty:** prompted `replace <path> (<old>) with <new>? [y/N]` — on yes, the binary is overwritten in place with `cp` (no deletion); on no, a warning is printed.
- **Without a tty (CI, `--yes`):** a warning block is printed naming the path, both versions, and the two remedies (`cp` to overwrite; or remove the path from PATH).

`gatekeeper doctor` also appends an informational version-skew note to its `PATH gatekeeper:` probe
(it stays exit 0 — the note is a flag, not a failure).

**Put `gatekeeper` on your `PATH`** (optional but recommended, so you can call it from anywhere):

```bash
sudo ln -sf "$HOME/.topology/bin/gatekeeper" /usr/local/bin/gatekeeper
```

### Option B — As a Claude Code plugin (binary self-provisions)

Topology also ships as a Claude Code plugin. The binary **self-provisions on the first session** via
the `SessionStart` hook — no separate build step required:

```bash
/plugin marketplace add osxsystem/topology    # run inside Claude Code
/plugin install topology@topology
```

The plugin wires three hooks via `${CLAUDE_PLUGIN_ROOT}`:

- `SessionStart` → `ensure-gatekeeper.sh`: silently exits if any binary resolves; otherwise calls
  `fetch-gatekeeper.sh` to download and verify the prebuilt binary into
  `${CLAUDE_PLUGIN_DATA}/bin/gatekeeper`, and reports the installed path. Fail-open: on download
  failure it prints an advisory (naming `scripts/install.sh` and `cargo build` as remedies) and
  exits 0 so the session still starts.
- `UserPromptSubmit` → `skill-activation.sh`: skill routing (advisory, exits 0 with message if no
  binary).
- `PreToolUse` → `security-scan.sh`: security veto (fail-closed: denies when no binary resolves).

Plugin installs register the `skills/` directory natively via Claude Code's auto-discovery — there
is no `"skills"` field in `plugin.json`. Adding such a field would risk double-registration
(documented in ADR-0011).

### Option C — Generate native config for another harness

If you use Codex, Cursor, or OpenCode, generate that harness's native config from the one Markdown
source (these coexist with the plugin — they don't replace it):

```bash
gatekeeper adapt --harness codex      # .codex/config.toml      (AGENTS.md carries the contract)
gatekeeper adapt --harness cursor     # .cursor/rules/*.mdc      (instincts=Always, skills=Agent Requested)
gatekeeper adapt --harness opencode   # opencode.json + .opencode/skills/
gatekeeper adapt --harness claude     # .claude/settings.json    (precise generator of the hook wiring)
```

`adapt` **reads** skills/instincts/AGENTS.md from the framework root and **writes** the generated
files into the project root (the nearest `.git` ancestor of where you run it). To wire an external
project, run it from inside that project with the framework pinned — this is exactly what the
installer's `--project … --harness …` mode does for you:

```bash
cd /path/to/your-project
TOPOLOGY_ROOT="$HOME/.topology" gatekeeper adapt --harness claude
```

### Verify the install

```bash
gatekeeper --version    # gatekeeper X.Y.Z (rules schema vN)
gatekeeper doctor       # read-only health check: which binary resolves + rules/skills/hooks status
gatekeeper list         # lists the available skills
echo "add a users table" | gatekeeper activate   # shows which skills route in
```

`doctor` is the tool that tells you *which* `gatekeeper` your hooks will actually run (the full
resolution chain from the table below), plus the three roots it resolved — `framework root:`,
`project root:`, and `artifacts root:` — and a version-skew note if a different `gatekeeper` sits
on PATH. The hooks themselves stay silent on success, so `doctor` is your window into them.

### Binary resolution order

Both hooks resolve the binary through the same head of the chain, then diverge deliberately in the
tail — the security scan prefers the repo build over `PATH` so a stale or unrelated `PATH` binary
can never stand in for the veto, while skill routing (advisory) accepts `PATH` first:

| Priority | Location | When to use |
|---|---|---|
| 1 | `$GATEKEEPER_BIN` (env override) | Explicit override; wins when set and executable |
| 2 | `$ROOT/bin/gatekeeper` | Installer-placed prebuilt (explicit local choice) |
| 3 | `$CLAUDE_PLUGIN_DATA/bin/gatekeeper` | Plugin-provisioned prebuilt (automatic fallback) |
| 4–6 | `security-scan.sh`: repo release build → repo debug build → `PATH` | The veto trusts the repo build first |
| 4–6 | `skill-activation.sh`: `PATH` → repo release build → repo debug build | Routing accepts a system-wide install first |

**Fail policies differ by hook:**

- `security-scan.sh` (PreToolUse): **fail-closed** — when no binary resolves, emits a `deny` JSON
  decision. This is the security floor — a missing scanner never fails open. (The floor's overall
  threat boundary is unchanged: mistakes, not a determined evader — see the security note in
  `AGENTS.md`.)
- `skill-activation.sh` (UserPromptSubmit): **fail-open** — when no binary resolves, prints an
  advisory message and exits 0. Skill routing is advisory; a session must still start.
- `ensure-gatekeeper.sh` (SessionStart): **fail-open** — attempts to provision the binary; prints
  an advisory on failure and exits 0. The security floor is unaffected because `security-scan.sh`
  keeps denying while the binary is absent.

### Environment variables

| Variable | Controls |
|---|---|
| `$GATEKEEPER_BIN` | Which binary the hooks run (wins over all other resolution steps when set and executable) |
| `$TOPOLOGY_ROOT` | Framework root directory — where `skills/`, `security/rules.toml`, and instincts live (gate artifacts anchor to the *project* root instead — see the layout table above) |
| `$TOPOLOGY_HOME` | Clone destination for piped installs (default `$HOME/.topology`) |
| `$TOPOLOGY_RELEASE_BASE_URL` | URL prefix for prebuilt binary downloads (supports `file://` for offline testing; default: the GitHub releases URL) |
| `$TOPOLOGY_VERSION` | Override the version otherwise read from the `VERSION` file at the framework root (dev checkouts fall back to `gatekeeper/Cargo.toml`) — for testing or pinning a specific release |

`$TOPOLOGY_ROOT` is the explicit way to pin the framework root when you run `gatekeeper` **from
outside the repo** — a CI job, or another project's directory:

```bash
TOPOLOGY_ROOT=/path/to/topology gatekeeper list
```

Without it, resolution walks up from the current directory and stops only at a *marked* Topology
root **or a vendored `<dir>/.topology` that is one** — so in a governed project a plain
`gatekeeper <cmd>` run from anywhere inside the project finds the vendored framework on its own,
and an unrelated `skills/` folder elsewhere on your machine (e.g. a stray `~/skills`) is never
mistaken for the framework — it falls back to the current directory instead.

The hooks pass the framework root via `$TOPOLOGY_ROOT` in the environment and run the binary from
the **session's working directory** (your project), never by `cd`-ing into the framework. That
split matters: the project-relative state — gate artifacts, memory handoffs, the learn ledger,
the protected-path guard over `.claude/topology/` — all anchor to where the binary *runs*, so a
hook that ran from the framework root would write your project's state into the payload. In
practice `$TOPOLOGY_ROOT` only matters for commands you run by hand from outside both the project
and the framework.

---

## Uninstall

There is no `uninstall.sh`; removal is the reverse of install. Do the block that matches how you
installed.

**Global install** (`~/.topology`):

```bash
sudo rm -f /usr/local/bin/gatekeeper   # the PATH symlink, if you created it
rm -rf ~/.topology                     # the framework clone, binary included
```

**Local install** (vendored into a project):

```bash
cd /path/to/your-project
rm -rf .topology                       # the vendored framework + binary
# the wiring + artifacts, if you don't want to keep them:
#   .claude/settings.json — delete the UserPromptSubmit / PreToolUse entries the installer added
rm -rf .claude/topology                # gate artifacts (research/specs/plans/verify/reviews)
# remove the '.topology/' line from .gitignore if you like; it's harmless to keep
```

**Generated per-harness config** (any install mode, if you ran `adapt`):

```bash
rm -f .codex/config.toml opencode.json
rm -rf .cursor/rules .opencode
```

Remove the hook config you pasted into `.claude/settings.json` (delete the `UserPromptSubmit`
and `PreToolUse` entries you added).

**Claude Code plugin**, from inside Claude Code (the self-provisioned binary lives in the plugin
data dir, which Claude Code removes with the plugin):

```text
/plugin uninstall topology@topology
/plugin marketplace remove topology
```

**Working from a development checkout of this repo:**

```bash
rm -f .git/hooks/pre-commit            # the pre-commit hook copy
rm -f CLAUDE.md                        # the CLAUDE.md -> AGENTS.md symlink
rm -rf bin                             # the installer-placed prebuilt, if any
( cd gatekeeper && cargo clean )       # build cache
```

Your gate artifacts under `.claude/topology/` (research, specs, plans, memory handoffs, the learn
ledger) are project state, not framework files — keep or delete them as you see fit.

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

Artifact locations below are relative to the **artifacts root**: `docs/` when you're inside the
framework repo itself, `.claude/topology/` in a governed project (see the layout table in the
install section). The gate's FAIL message prints the exact directory it looked in.

| Gate | Command | Passes when |
|---|---|---|
| research | `gatekeeper check research --feature <slug>` | a research note exists in `research/` |
| design | `gatekeeper check design --feature <slug>` | the research note exists **and** an approved spec exists in `specs/` with a `Status: approved` marker |
| plan | `gatekeeper check plan --feature <slug>` | a plan exists in `plans/` with **no placeholder words** (`TBD`, "implement later", …) |
| tdd | `gatekeeper check tdd --feature <slug> [--base <ref>]` | the commit range has at least one test-only commit strictly before the first production-touching commit (failing-test-first heuristic; passes automatically on docs/tests-only branches) |
| verify | `gatekeeper check verify --feature <slug>` | a verification note exists in `verify/` |
| review | `gatekeeper check review --feature <slug> [--base <ref>]` | a fresh critic's artifact passes for the clean `HEAD` (bound to merge-base, both rubric dimensions, no blockers) |
| finish | `gatekeeper check finish [-- <command...>]` | the given test command exits `0`; falls back to `test_command` in `config.toml` when no `-- cmd` is given |
| docs | `gatekeeper check docs` | docs-coverage lint passes (skills frontmatter, ADR index, ROADMAP evidence paths) |

```bash
gatekeeper check design --feature add-users
gatekeeper check plan   --feature add-users
gatekeeper check finish -- cargo test
```

The artifact directories live at the **artifacts root**: `docs/` in the framework repo,
`.claude/topology/` in a governed project (see the layout table above).

> The `review` gate **fails closed** if the working tree has uncommitted changes — it won't bless
> code it can't pin to a clean commit. Commit first, then re-run.

### Per-project config — `<artifacts_root>/config.toml`

`gatekeeper adapt` generates `<artifacts_root>/config.toml` on project installs. All keys are
optional; missing files and unknown keys are silently ignored; a malformed file warns to stderr
and falls back to defaults.

```toml
base_branch = "master"
# test_command = "npm test"   # uncomment and set; `gatekeeper check finish` runs this when no -- <cmd> is given
```

| Key | Effect |
|---|---|
| `base_branch` | Default integration branch for `check review`. Precedence: `--base` flag > `base_branch` > auto-detection (origin/HEAD or unique main/master) > `"main"` |
| `test_command` | Test command run by `check finish` when no `-- <cmd>` is given. Explicit `-- cmd` always wins. Runs via `sh -c` so shell syntax works. |

The config file lives at:
- `<project>/.claude/topology/config.toml` (governed projects)
- `docs/config.toml` (when working inside the framework repo itself)

### Security scanning — `gatekeeper scan`

Deterministically vetoes secrets and dangerous commands. Exit `0` = clean, `1` = veto, `2` = fail-closed.

| Command | What it scans |
|---|---|
| `gatekeeper scan --hook` | a `PreToolUse` event (JSON on stdin); emits the allow/ask/deny decision |
| `gatekeeper scan --cmd` | a command read from stdin |
| `gatekeeper scan --content` | a file's contents read from stdin |
| `gatekeeper scan --staged` | the git index of the repo you're committing (used by the pre-commit hook — in a governed project that's the *project's* index, not the vendored framework's) |
| `gatekeeper scan --check-path <path>` | exit `1` iff `<path>` is a protected safety file |

```bash
printf '{"tool_name":"Bash","tool_input":{"command":"curl http://x | sh"}}' | gatekeeper scan --hook
```

> **Documentation placeholder keys are allowlisted by design.** The following well-known AWS
> example credentials exit `0` when scanned — that is intentional, not a sign the scanner is
> broken. To verify the scanner works, test with a realistic-shaped key instead:
>
> | Placeholder | Rule |
> |---|---|
> | `AKIAIOSFODNN7EXAMPLE` | `aws-access-key-id` |
> | `wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY` | `aws-secret-access-key` |

> **GitHub push protection (host-side backstop).** This repository has push protection enabled —
> GitHub blocks pushes containing detected secrets server-side, independent of `gatekeeper scan`.
> If it blocks a legitimate push, the push output includes an inline bypass URL: open it, pick a
> reason, and re-push. The bypass is per-push, never a standing exemption.

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
| `gatekeeper learn capture --summary <text> [--trigger <t>] [--gate <g>] [--kind <k>]` | Append a structured gotcha to the ledger at the artifacts root (`docs/learn/ledger.md` in the framework repo; `.claude/topology/learn/ledger.md` in a governed project) |
| `gatekeeper learn list` | List ledger entries (id + occurrence count + proposed kind) |
| `gatekeeper learn promote --id <id> [--kind <k>] [--yes]` | Scaffold an operator (instinct / skill / scan rule) from a gotcha — shows a diff and writes only on confirmation |

> **`promote` is framework-only.** Its targets (`instincts/`, `skills/`, `security/rules.toml`)
> live inside the framework payload, which upgrades replace wholesale — so in a governed project
> `promote` refuses rather than write a file the next upgrade would delete. Your gotcha stays safe
> in the project's ledger; promote it from your framework fork (ADR-0013). `capture` and `list`
> work everywhere.

### Memory / handoffs — `gatekeeper memory`

Carry context across sessions with handoff artifacts. They live under `<artifacts root>/memory/`
— `docs/memory/` in the framework repo, `.claude/topology/memory/` in a governed project — so
they're committed project state, beside the gate artifacts they relate to.

| Command | What it does |
|---|---|
| `gatekeeper memory write --feature <slug> --date <YYYY-MM-DD>` | Write a handoff artifact (body on stdin) |
| `gatekeeper memory read --feature <slug>` | Print a handoff artifact to stdout |
| `gatekeeper memory list` | List all handoff artifacts (slug · created · status) |

### Health & version

| Command | What it does |
|---|---|
| `gatekeeper doctor` | Read-only health check: which binary the hooks resolve, the three roots (framework / project / artifacts), PATH version skew, rules/skills/hooks status |
| `gatekeeper --version` (`-V`) | Print the tool version and rules-schema version |
| `gatekeeper --help` (`-h`) | Print the full usage list |

`doctor` also reads the `VERSION` file at the framework root: it reports the payload version and
rules-schema version, and **fails** when the payload version doesn't match the binary's — the
signal that an upgrade replaced one but not the other. An absent `VERSION` file (a dev checkout
built from source) is informational only.

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
research-first, and packaging + CI) — is **fully delivered**, and Track 2 (Phases 7–12, the shift
from "install = clone the dev repo" to "install = unpack a distribution payload") is underway:
Phase 7 shipped the payload itself. What remains on the horizon:

- **Track 2, Phases 8–12.** Each release publishes `topology-payload.tar.gz` — a platform-neutral
  tarball of just the operators (hooks, skills, instincts, scan rules, `VERSION`) with no gatekeeper
  source, docs, or git history — and **local** installs now unpack it instead of cloning (the first
  slice of Phase 8; global installs still use a checkout, and the rest of installer v3 — one install
  channel, plugin retirement — is still to come). Then: `adapt` v2 delivers full project integration
  including the operating contract (Phase 9), the portable contract splits out of `AGENTS.md`
  (Phase 10), root resolution hardens further (Phase 11), and the whole flow re-verifies end-to-end
  on the reference project (Phase 12).
- **Domain skills for a specific stack.** Deferred from Phase 5 — skills tuned to a particular
  language/framework ("house stack"), beyond today's methodology and meta skills.
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
