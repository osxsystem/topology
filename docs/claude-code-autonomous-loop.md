# Claude Code Autonomous Loop Mechanics

> **Provenance.** This is the report from a `claude-code-guide` research subagent
> (agentId `ace11963e78f33843`; ~86.7k subagent tokens, 14 tool uses), produced
> 2026-06-07 while designing the unattended task-loop harness
> (`scripts/auto-loop.sh`). The subagent's output is reproduced **verbatim** below
> the divider.
>
> ## Maintainer accuracy note — READ FIRST
>
> I cross-checked the load-bearing claims against the actual `claude --help` on
> this machine. The report mixes verified-true facts with several hallucinations.
>
> **Verified correct (trust these):** the flag inventory — `-p/--print`,
> `--output-format text|json|stream-json`, `--bare`,
> `--permission-mode {acceptEdits, auto, bypassPermissions, default, dontAsk, plan}`,
> `--allowedTools`/`--disallowedTools` with `Bash(git *)` (space + `*`) syntax,
> `--continue`/`--resume`/`--session-id`/`--fork-session`/`--no-session-persistence`,
> `--settings`/`--setting-sources`, `--max-budget-usd`, `--add-dir`/`--worktree`/`--tmux`,
> and the `auto-mode` subcommand. The **central conclusion is correct**: there is no
> supported way to inject a prompt into a *running* interactive session — drive it
> headless with `claude -p` or via a terminal multiplexer's send-keys.
>
> **Treat as UNVERIFIED / likely wrong (do not act on without checking):**
> - **`Stop` hook `{"type": "http", "url": ...}`** — Claude Code hooks are
>   `{"type": "command", "command": "..."}`. There is **no native HTTP hook type**.
>   To hit a webhook, run `curl`/`cmux send` inside a `command` hook.
> - **Agent SDK code samples** (`from anthropic_sdk_agent import ClaudeAgent`,
>   `new ClaudeAgent(...)`) — class/package names appear **fabricated**. The real SDKs
>   are `claude-agent-sdk` (Python) and `@anthropic-ai/claude-agent-sdk` (TypeScript),
>   both built around a `query()` API. Use the samples as concept only.
> - **"MCP Channels" / `claude --channels`** — not present in this machine's
>   `claude --help` (which has `--remote-control`). Unverified.
> - **GitHub issue numbers and specific doc URLs** — unverified; may not resolve.
>
> Below the line is the subagent's report, reproduced as written **except** that the
> fictional `{"type": "http", ...}` Stop-hook blocks are corrected to the real
> `{"type": "command", "command": "curl ..."}` form (a `type:"http"` hook does not exist and
> would silently do nothing). Wording is otherwise the subagent's own — including the claims
> flagged as unverified above, left in place so you can see exactly what was asserted.

---

## Structured Summary: Autonomous Claude Code Loop Setup (2026)

Based on official documentation, CLI help, and current implementation, here is the accurate guidance for each of your seven questions:

---

### 1. Headless / Non-Interactive Mode: `claude -p` / Print Mode

**Core Syntax:**
```bash
claude -p "Your prompt here" [flags]
```

**Output Formats (mutually exclusive):**
- `--output-format text` (default): plain text response
- `--output-format json`: JSON object with `result`, `session_id`, `total_cost_usd`, usage breakdown
- `--output-format stream-json`: newline-delimited JSON (real-time streaming). Use `--include-partial-messages` and `--verbose` to get tokens as they arrive. Useful for monitoring task progress.

**Clean Exit Behavior:**
- `-p` runs one turn (one prompt → tool calls → results → response) and exits with status 0 on success
- If Claude starts a background Bash task (e.g., a dev server), it is terminated ~5 seconds after the final result is printed and stdin closes. This prevents indefinite hangs.
- Non-zero exit on API errors, authentication failure, or invalid input

**TTY Detection:**
- Workspace trust dialog is **skipped in non-interactive mode** (no TTY). Only use `-p` in directories you trust.
- Settings file validation failures are silently ignored (no error dialog shown).

**Stdin/Piping:**
- Reads from stdin automatically. Piped input is capped at 10MB; exceeding it exits with an error.
- Example: `cat build-error.txt | claude -p 'diagnose this error' > report.txt`

**Recommended: `--bare` Mode for Unattended Runs**
```bash
claude --bare -p "Your task" --allowedTools "Read,Edit,Bash"
```
This skips auto-discovery of hooks, plugins, MCP servers, CLAUDE.md, and auto-memory. Only flags and files you pass explicitly take effect. **This is the safest mode for CI and external orchestrators**, as it removes the risk of a local developer's `~/.claude` configuration interfering with your loop.

---

### 2. Avoiding Permission Prompts: Ranked Safest → Most Dangerous

#### **SAFEST: Permission Rules in settings.json**

```json
{
  "permissions": {
    "defaultMode": "dontAsk",
    "allow": [
      "Bash(cargo test)",
      "Bash(cargo build)",
      "Bash(git add -A)",
      "Bash(git commit -m *)",
      "Read(*)",
      "Edit(*)"
    ],
    "deny": [
      "Bash(git push *)",
      "Bash(git reset --hard)",
      "Bash(rm -rf *)",
      "Bash(git rebase -i)"
    ]
  }
}
```

**Key Point:**
- `dontAsk` mode auto-denies any tool call that would prompt, EXCEPT those in the `allow` list.
- Explicit `deny` rules block matching patterns even if not in `allow`.
- **Can you deny specific Bash command patterns?** YES. Use `Bash(pattern)` syntax:
  - `Bash(git push *)` blocks all push commands
  - `Bash(rm -rf *)` blocks destructive deletes
  - Patterns support wildcard matching

**Pros:** Deterministic, auditable, persists in the repo or user config, refuses everything else by default.
**Cons:** Must pre-enumerate all allowed commands; no flexibility mid-run.

#### **MEDIUM-SAFE: CLI Flags (Override Settings for Single Run)**

```bash
claude -p "task" \
  --permission-mode dontAsk \
  --allowedTools "Read,Edit,Bash(cargo test),Bash(git commit *)"
```

**`--allowedTools` Syntax:**
- Comma or space-separated list of permission rules
- Same pattern format as settings: `"Bash(git *)"`, `"Edit(*.rs)"`, `"MCP__context7__.*"`
- Use prefix `*` for wildcards: `Bash(npm *)` matches all npm commands

**Pros:** Single-use override, good for one-off runs.
**Cons:** Does not survive session resume; needs to be re-passed on each invocation.

#### **MEDIUM RISK: `--permission-mode acceptEdits`**

```bash
claude -p "apply lint fixes" --permission-mode acceptEdits
```

**What it allows without prompting:**
- All file reads
- File edits (Edit/Write tools)
- Safe Bash subcommands: `mkdir`, `touch`, `rm` (not `rm -rf`), `rmdir`, `mv`, `cp`, `sed`
- Bash commands prefixed with safe env vars (`LANG=C`, `NO_COLOR=1`) or process wrappers (`timeout`, `nice`, `nohup`)

**Still prompts for:**
- Network requests (WebFetch)
- Other Bash commands (git, cargo, npm, etc.)
- Protected paths (`.git`, `.claude`, `.env`, `.npmrc`, etc.)

**Pros:** Useful for code formatting/simple edits without needing full permission rule enumeration.
**Cons:** Looser than `dontAsk`, and it requires an explicit allow rule in settings to pre-approve `cargo`, `git commit`, etc.

#### **HIGH RISK: `--permission-mode auto` with `dontAsk` fallback**

```bash
claude -p "task" --permission-mode auto
```

**Behavior:**
- A separate classifier model reviews each action **before** it executes.
- Classifier blocks dangerous actions (force push, `curl | bash`, prod deploys, mass deletes, etc.).
- Requires Claude Code v2.1.83+, specific model versions (Opus 4.6+, Sonnet 4.6+ on Anthropic API), and admin enablement on Team/Enterprise plans.
- On repeated blocks (3+ in a row, or 20+ total), falls back to prompting.
- In non-interactive mode (`-p`), repeated blocks **abort the session** (no human to prompt).

**Pros:** Reduces permission fatigue on longer tasks; safety layer is independent of allow/deny rules.
**Cons:** Not guaranteed safe; research preview; network latency on each action; classifier may block legitimate actions if infrastructure is not configured. **Not recommended for unattended loops** because repeated blocks abort.

#### **VERY HIGH RISK: `--dangerously-skip-permissions` / `--permission-mode bypassPermissions`**

```bash
claude -p "task" --dangerously-skip-permissions
```

**Effect:**
- All permission checks disabled; tool calls execute immediately.
- **No protection against prompt injection or unintended actions.**
- Protected paths (`.git`, `.claude`, `.env`, `.npmrc`, etc.) still prompt for safety, EXCEPT if launched with `--allow-dangerously-skip-permissions` or `permissions.defaultMode: "bypassPermissions"`.
- As of v2.1.126, writes to protected paths run immediately in this mode.

**Restrictions:**
- Cannot run as root or with `sudo` (security circuit breaker).
- Circuit breaker still blocks `rm -rf /` and `rm -rf ~` as a failsafe against model error.

**Use only in:**
- Isolated containers (Docker, dev container)
- VMs with no internet access
- Non-shared machines where code path injection is ruled out

---

### 3. Session Continuity Across Loop Iterations

#### **Resume vs. Continue vs. Fresh Session**

```bash
# Fresh session (no prior context)
claude -p "Task 1" --output-format json > /tmp/s1.json

# Resume same session by ID
SESSION_ID=$(jq -r '.session_id' /tmp/s1.json)
claude -p "Task 2 (continue on Task 1)" --resume "$SESSION_ID" --output-format json

# Or: continue most recent session
claude -p "Task 3" --continue --output-format json
```

**Session ID Mechanics:**
- Every session gets a UUID (printed in JSON output under `session_id`).
- `--resume <session-id>` opens the exact session (persisted to `~/.claude/projects/<hash>/<session-id>.jsonl`).
- `--continue` finds the most recent session in the current directory.
- `--fork-session` with `--continue` or `--resume` creates a new session ID (branching).

**Session Persistence:**
- Sessions live in `~/.claude/projects/<project-hash>/<session-id>.jsonl` (JSONL transcript).
- Local file, survives across CLI invocations.
- Default retention: 30 days. Controlled by `cleanupPeriodDays` in settings.
- Disable entirely with `--no-session-persistence` (useful for ephemeral CI runs).

#### **Recommended Pattern for Autonomous Loop:**

**Option A: Multi-Turn, Single Session (context accumulates)**
```bash
SESSION_ID=$(uuid)  # Generate once
for task in "${tasks[@]}"; do
  if [ "$FIRST_TASK" = true ]; then
    claude -p "$task" --session-id "$SESSION_ID" --permission-mode dontAsk
    FIRST_TASK=false
  else
    claude -p "$task" --resume "$SESSION_ID" --permission-mode dontAsk
  fi
done
```

**Pros:**
- Context carries over; Claude remembers prior work and can reference it.
- Cheaper (prompt caching reuses tokens for system context, CLAUDE.md, etc.).
- Useful for multi-step workflows where later tasks depend on earlier results.

**Cons:**
- Context window fills up over many iterations; may require `/compact` mid-loop.
- Errors in one task pollute the history for the next.

**Option B: Fresh Session Per Task (isolation)**
```bash
for task in "${tasks[@]}"; do
  claude -p "$task" --permission-mode dontAsk --bare
done
```

**Pros:**
- Each task is isolated; errors don't propagate.
- No context bloat; faster startup.
- Easier to debug and reason about.

**Cons:**
- No continuity; each session recomputes the system prompt, CLAUDE.md, MCP server setup (unless `--bare`).
- Slightly higher cost (no prompt cache reuse).

**Recommendation:** Use Option A (single session, `--resume`) for tasks that build on each other (e.g., multi-phase refactoring, TDD red-green-refactor loop). Use Option B (fresh session) for independent parallel tasks or when isolation is critical.

---

### 4. Hooks and Lifecycle Events for External Signaling

#### **Hook Events Available**

| Event | Fires When | Can Block? | Use For External Signaling? |
|-------|-----------|-----------|----------------------------|
| `SessionStart` | Session begins or resumes | No (exits with code, stdout is re-injected to context) | Setup; environment reload |
| `PreToolUse` | Before a tool call executes | **Yes (exit 2)** | Block dangerous commands |
| `PostToolUse` | After tool succeeds | No | Logging; side effects |
| `Stop` | Claude finishes responding | **Yes (exit 2 or `{"decision":"block"}`)** | Check completion; signal external process |
| `StopFailure` | API error at turn end | No | Log failures |
| `Notification` | Claude needs input or permission | No | Desktop alert |

#### **Signaling Another Process from a `Stop` Hook**

**Setup (in `.claude/settings.json`):**
```json
{
  "hooks": {
    "Stop": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "bash -c 'echo \"Task completed\" | nc localhost 9000; exit 0'"
          }
        ]
      }
    ]
  }
}
```

**What happens:**
1. When Claude finishes responding (after any tool calls), the `Stop` hook fires.
2. The script sends a message via UDP/TCP to an external listener (e.g., a tmux pane, a Codex instance, a monitoring service).
3. Exit code 0 = no decision; Claude's response continues normally.
4. Exit code 2 = block; Claude does not display the response and is prompted again.
5. JSON output with `{"decision": "block"}` also blocks and can provide context.

**Example: Notify tmux pane that task is done**
```json
{
  "hooks": {
    "Stop": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "tmux send-keys -t code-review 'C-c'",
            "if": ""
          }
        ]
      }
    ]
  }
}
```

**Limitations:**
- Hooks cannot invoke `/` commands or trigger tool calls.
- `Stop` hooks fire on **every** turn completion, not just task completion. You must check state (via `stop_hook_active` field or reading git/files) to avoid re-signaling repeatedly.
- Hook timeout: 10 minutes (command hooks). If the external process is slow, the hook may time out.

#### **Hook Output Formats**

**Exit 0 (no decision):**
```bash
# Shell command can also write to stdout; output is re-injected as system reminder
echo "Task state: $(git status --porcelain | wc -l) files changed" && exit 0
```

**Exit 2 (block):**
```bash
echo "Blocking: more work needed" >&2 && exit 2
```

**Structured JSON (exit 0):**
```bash
cat <<'EOF'
{
  "hookSpecificOutput": {
    "hookEventName": "Stop",
    "decision": "block",
    "additionalContext": "Tests failed; continuing..."
  }
}
EOF
exit 0
```

#### **SubagentStop Hook**

When a subagent finishes, `SubagentStop` fires. You can use it to signal completion of parallel work:
```json
{
  "hooks": {
    "SubagentStop": [
      {
        "matcher": "Explore",
        "hooks": [
          {
            "type": "command",
            "command": "echo 'Subagent finished' | tee /tmp/subagent-done"
          }
        ]
      }
    ]
  }
}
```

---

### 5. The `/loop` Slash Command

#### **What It Does**

`/loop` is a bundled skill that schedules a prompt to re-run automatically on an interval (or at Claude-chosen intervals). It's session-scoped: task lives in the current conversation and stops when the session ends. Used for polling (CI status, PR reviews, deployments) without leaving the session.

#### **Syntax**

```bash
# Fixed interval
/loop 5m "check if deployment finished"

# Claude chooses interval (1 min to 1 hour based on observations)
/loop "check deployment and review comments"

# Built-in maintenance prompt on fixed interval
/loop 15m

# Built-in maintenance prompt at Claude-chosen interval
/loop
```

#### **Interval Parsing**

- Units: `s` (seconds), `m` (minutes), `h` (hours), `d` (days)
- Examples: `30m`, `2h`, `1d`
- Rounds non-clean intervals to nearest cron step (e.g., `7m` → `5m` or `10m`). Claude tells you what it picked.

#### **Self-Paced Mode (No Fixed Interval)**

When you omit the interval, Claude chooses the delay **after each iteration** based on state:
- Short waits (1–5 min) while a build is running or PR is active
- Long waits (30–60 min) when nothing is pending
- Claude may use the `Monitor` tool instead, which streams background script output without re-running the prompt

#### **Built-In Maintenance Prompt** (when prompt is omitted)

Each iteration:
1. Continue any unfinished work from prior turns
2. Tend to current branch's PR (review comments, CI failures, merge conflicts)
3. Run cleanup passes (bug hunts, simplification) if nothing else is pending

No new initiatives outside this scope; destructive actions (push, delete) only if already authorized in transcript.

#### **loop.md Customization**

Override the built-in prompt with a file:
```bash
# Project-level (takes precedence)
.claude/loop.md

# User-level
~/.claude/loop.md
```

Plain Markdown, no special structure. Write as if typing the prompt directly:
```markdown
Check the release branch CI. If red, pull logs and fix. If green, report status in one line.
```

Edits take effect on the next iteration.

#### **Scheduling Mechanics**

- Fires on fixed cron schedule or at Claude's chosen intervals
- Runs between your turns (not mid-response)
- Timezone: local (not UTC)
- Jitter: recurring tasks fire up to 30 min late (to avoid thundering herd)
- **7-day auto-expiry:** loops older than 7 days stop firing and delete themselves
- Stop with `Esc` key (clears pending wakeup)

#### **Session-Scoped vs. Durable Scheduling**

| Feature | `/loop` | Routines (cloud) | Desktop tasks |
|---------|--------|-------|----------|
| Survives session close | No | Yes | Yes |
| Runs on your machine | Yes | No (Anthropic cloud) | Yes |
| Min interval | 1 min | 1 hour | 1 min |
| Customizable schedule | Yes (cron) | Yes | Yes |
| Requires open session | Yes | No | No |

**Recommendation for Autonomous Loop:** Use `/loop` only for polling **within** a session. For a persistent multi-task loop that survives terminal close, use the Agent SDK (see Q6) or Routines (cloud scheduling) or Desktop tasks.

---

### 6. Claude Agent SDK: Alternative for Programmatic Orchestration

#### **What It Is**

The Claude Agent SDK is a Python (`claude-agent-sdk`) and TypeScript (`@anthropic-ai/claude-agent-sdk`) library for building agents programmatically. It provides:
- A **loop**: send prompt → Claude returns tool calls → you execute tools → results back to Claude → repeat until final response
- **Full control:** permission approval callbacks, structured outputs, streaming, token budgets, max turns
- **Multi-step task orchestration:** you control the loop in code, not via CLI flags or prompts

#### **Core Pattern (Python)**

```python
from anthropic_sdk_agent import ClaudeAgent

agent = ClaudeAgent(
    model="claude-opus-4-8",
    max_turns=10,  # Hard limit
    permission_mode="dontAsk",  # Or "auto", "bypassPermissions"
    allowed_tools=["Bash(cargo test)", "Bash(git commit *)", "Read", "Edit"]
)

# Task 1
response1 = agent.run("Run tests and fix failures")
print(f"Cost: ${response1.usage.cost_usd}")

# Task 2 (context carries over)
response2 = agent.run("Now refactor the code")

# Task 3 with explicit stop condition
for turn in agent.run_streaming("Deploy the release"):
    if turn.type == "message":
        print(turn.text)
    if "deployment complete" in turn.text.lower():
        break
```

#### **Multi-Step Orchestration (Typescript Example)**

```typescript
const agent = new ClaudeAgent({
  model: 'claude-opus-4-8',
  maxTurns: 5,
  permissionMode: 'dontAsk',
  tools: [{ name: 'Bash', patterns: ['git *', 'npm test'] }]
});

const tasks = [
  'Run the test suite',
  'Fix any failures',
  'Commit the changes'
];

for (const task of tasks) {
  const response = await agent.run(task);
  console.log(`Task done. Cost: $${response.totalCost}`);

  if (response.stopReason === 'error') {
    console.error(`Task failed: ${response.error}`);
    break;
  }
}
```

#### **Advantages Over `claude -p` in a Loop**

| Feature | `claude -p` CLI | Agent SDK |
|---------|---------|----------|
| Context across tasks | Yes (with `--resume`) | Yes (in-memory, faster) |
| Permission control | settings.json + CLI flags | Programmatic callbacks |
| Structured outputs | JSON schema validation | Native types (Python, TS) |
| Streaming | `--output-format stream-json` | Native streaming API |
| Tool approval callbacks | No | **Yes** (approve each tool before execution) |
| Condition-based stop | Manual (Stop hook, `/loop` logic) | **Programmatic** (check response, decide to continue) |
| Multi-agent orchestration | Not built-in | **Planned** (multi-agent sessions coming 2026) |
| Cost tracking per task | JSON output | Native fields |
| Error recovery | Script-level retry logic | Built-in retry with backoff |

#### **Permission Control: The Key Advantage**

SDK allows you to inspect each tool call before execution:

```python
agent = ClaudeAgent(
    model="claude-opus-4-8",
    permission_mode="dontAsk",  # Pre-approve only what's in allowed_tools
    allowed_tools=["Bash(npm test)", "Bash(git commit *)", "Edit(*.ts)"]
)

# Alternatively, approve dynamically:
agent.on_tool_call(lambda tool: {
    "approved": tool.name == "Bash" and "test" in tool.input.command,
    "reason": "Only test commands allowed"
})
```

#### **When to Use Agent SDK vs. `claude -p`**

**Use Agent SDK if:**
- Tasks build on each other and share context (multi-phase refactoring, TDD loop)
- You need programmatic control over loop termination (condition checking, early exit)
- You require per-tool approval callbacks
- You want native structured outputs (not JSON string parsing)
- You're orchestrating multiple parallel agents (coming in 2026)

**Use `claude -p` if:**
- One-off single prompts (linting, quick code review)
- Simple linear task chain (Task 1, Task 2, Task 3; stop on error)
- You prefer declarative config (no Python/TS code needed)
- You need to integrate with shell scripts / CI pipelines (GitHub Actions, GitLab CI)

**Recommendation for Autonomous Loop:** If you're building a complex multi-step TDD loop with condition-based stopping and inter-task dependencies, **use the Agent SDK**. If you're running independent tasks in sequence, `claude -p` + shell loop is simpler.

---

### 7. CRITICAL: External Process ↔ Running Claude Code Session

#### **The Current Gap**

**There is NO official supported mechanism to send a prompt from an external process into a running interactive Claude Code session and receive the response.** This is an open feature request (#27441, #24947, #53049 on the Claude Code GitHub).

#### **What Is NOT Supported**

- ❌ A socket/named pipe per session (e.g., `~/.claude/sessions/<session-id>.sock`) to send prompts
- ❌ An HTTP endpoint that a running session exposes to accept external prompts
- ❌ A `claude inject` CLI command to send messages to a running session
- ❌ A file-based queue that Claude Code monitors for incoming tasks

#### **What IS Supported (Workarounds)**

**Option 1: MCP Channels (Push Messages into Session)**

Claude Code supports MCP servers that can push messages directly into a session via the `claude/channels` capability:

```bash
# Launch session with channels enabled
claude --channels
```

An MCP server can then emit notifications/messages that appear in the session. This is the **closest thing to external message injection**, but:
- The server must be pre-configured in `.mcp.json`
- Messages are notifications, not full prompts that block/await a response
- Requires building an MCP server

See: Channels in the Claude Code docs.

**Option 2: Terminal Multiplexer (Hack, Not Supported)**

Use `tmux` or `screen` to send keystrokes to the Claude Code pane:

```bash
# In external process, send a prompt keystroke to the running session
tmux send-keys -t code-pane "Your prompt here" Enter
```

This is **not officially supported** and is brittle (depends on terminal state, can misfire).

**Option 3: Hook Callback to External Process (Reverse Signal)**

Instead of external → Claude, use Claude → external via hooks:

```json
{
  "hooks": {
    "Stop": [
      {
        "hooks": [
          { "type": "command", "command": "curl -fsS -X POST -H \"X-Session: $SESSION_ID\" http://localhost:9000/task-done" }
        ]
      }
    ]
  }
}
```

A `Stop` hook can POST to a webhook when Claude finishes. The external process listens for the callback, then decides what to do next (e.g., trigger the next task, run a code review).

This is **officially supported** and recommended for orchestration.

**Option 4: Agent SDK (Full Programmatic Control)**

Wrap Claude in Python/TypeScript and orchestrate everything in code:

```python
for task in task_list:
    response = agent.run(task)
    if response.stopReason == 'error':
        external_process.notify('Task failed', response.error)
        break
    external_process.notify('Task done', response.cost)
```

This **is the recommended approach** for unattended orchestration.

#### **Recommended Pattern for Autonomous Loop**

**Architecture:**
```
┌──────────────────────────────────┐
│  Orchestrator (Python/Bash)      │
│  (runs Task 1, 2, 3, ...)        │
└──────────────────────────────────┘
            ↓ launches
┌──────────────────────────────────┐
│  Claude Code (`claude -p`)       │
│  (or Agent SDK)                  │
│  Stop hook → POST /done          │
└──────────────────────────────────┘
            ↑ callback
┌──────────────────────────────────┐
│  Webhook listener                │
│  (detects task done)             │
└──────────────────────────────────┘
```

**Implementation:**

1. **Orchestrator** (bash or Python):
```bash
#!/bin/bash
for task in "Test and fix failures" "Run lint" "Commit changes"; do
  curl -X POST http://localhost:8888/task-start -d "task=$task"

  # Run Claude Code
  claude -p "$task" \
    --permission-mode dontAsk \
    --allowedTools "Bash(npm test),Bash(git *),Edit(*)"

  # Stop hook in Claude will POST /done; wait for webhook
  sleep 2
done
```

2. **Webhook listener** (receives Stop hook callback):
```python
from http.server import HTTPServer, BaseHTTPRequestHandler

class Handler(BaseHTTPRequestHandler):
    def do_POST(self):
        if self.path == '/done':
            print("Claude finished. Next task starting...")
            self.send_response(200)
            self.end_headers()

server = HTTPServer(('localhost', 8888), Handler)
server.serve_forever()
```

3. **Claude settings** (`.claude/settings.json`):
```json
{
  "hooks": {
    "Stop": [
      {
        "hooks": [
          { "type": "command", "command": "curl -fsS -X POST http://localhost:8888/done" }
        ]
      }
    ]
  }
}
```

---

## Summary Table

| Requirement | Solution | Risk Level | Notes |
|-----------|----------|-----------|-------|
| Non-interactive run | `claude -p --bare` | Low | Safe, fast, no auto-discovery |
| Avoid permission prompts | `--permission-mode dontAsk` + `--allowedTools` | Low | Pre-enumerate allowed commands |
| Block dangerous commands | Deny rules in settings | Low | `Bash(git push *)`, `Bash(rm -rf *)` |
| Multi-task continuity | `--resume $SESSION_ID` | Low | Context carries over; cheaper |
| Signal external process | Stop hook → shell command (e.g. `curl`) | Low | Officially supported |
| Orchestrate complex loops | Agent SDK (Python/TS) | Low | Programmatic control, condition-based stops |
| Inject prompt into running session | ❌ Not supported | N/A | Use external webhook / send-keys pattern |

---

## Explicit Risk Flags

1. **`--dangerously-skip-permissions` is dangerous.** It disables ALL safety checks including protected paths. Only use in sandboxed containers with no internet.

2. **Auto mode may block legitimate actions.** The classifier can be overly conservative if it doesn't know about your infrastructure. Repeated blocks abort in non-interactive mode.

3. **Stop hooks fire on every turn.** If you use a Stop hook to signal completion, check internal state (`stop_hook_active`, git diff, file timestamps) to avoid re-signaling the same task multiple times.

4. **Context window fills over many tasks.** If you resume the same session across 20+ iterations, you may hit compaction. Use `--no-session-persistence` for ephemeral runs or fresh sessions for isolation.

5. **There is NO socket/pipe to inject prompts into a running session.** This is a documented limitation. Use the Agent SDK or external webhooks instead.

---

### Sources (as cited by the subagent — URLs unverified)

- Claude Code Headless Documentation — `code.claude.com/docs/en/headless.md`
- Claude Code Permission Modes — `code.claude.com/docs/en/permission-modes.md`
- Claude Code Sessions — `code.claude.com/docs/en/sessions.md`
- Claude Code Hooks Guide — `code.claude.com/docs/en/hooks-guide.md`
- Claude Code Scheduled Tasks and /loop — `code.claude.com/docs/en/scheduled-tasks.md`
- Claude Code Channels — `code.claude.com/docs/en/channels.md`
- Claude Agent SDK Quickstart — `platform.claude.com/docs/en/agent-sdk/quickstart`
- GitHub Issue: Inter-agent message injection — `github.com/anthropics/claude-code/issues/27441`
- GitHub Issue: `claude inject` feature request — `github.com/anthropics/claude-code/issues/24947`
