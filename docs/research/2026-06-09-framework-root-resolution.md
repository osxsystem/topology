# Research — framework-root resolution hijacked by a stray `skills/`

## Problem

`gatekeeper`'s `framework_root()` (`gatekeeper/src/main.rs`) walks **up** from the current
directory and returns the first ancestor that contains a `skills/` directory, falling back to the
current directory if none is found. Every framework-relative command depends on it: `scan`,
`instinct`, `adapt`, `learn`, `memory`, `doctor`, `list`, `activate`, and all `check` gates.

A generic `skills/` directory name is not a reliable marker of a Topology root. If any ancestor of
the working directory happens to contain a `skills/` folder, resolution stops there.

## Evidence (reproduced 2026-06-09)

Testing Topology against an external project `/Users/hugues_mini/Codes/AgentTools/react-weather-app`:

- The machine has an **unrelated** `~/skills` directory (from a different agent-skills tool:
  `find-skills`, `javascript-sdk`, `react-components`).
- Running `gatekeeper check design --feature forecast-widget` from the project reported the
  project's docs missing — it looked under `$HOME/docs/`, never `react-weather-app/docs/`.
- `gatekeeper list` run from the project printed the three `~/skills` entries instead of Topology's
  twelve skills. This confirms `framework_root()` resolved to `$HOME`.

The stray `~/skills` silently defeats the intended "fall back to cwd" behaviour: a non-fork external
project that *should* read its own `docs/` instead resolves to `$HOME`.

## What is unaffected

- **Hooks** (`hooks/skill-activation.sh`, `hooks/security-scan.sh`) `cd "$ROOT"` — the Topology repo,
  derived from the script's own path — before invoking the binary, so skill routing and the
  PreToolUse veto resolve correctly cross-project regardless of cwd.
- The **`finish` gate** runs the test command in the process cwd, independent of `framework_root()`.

Only manually-invoked framework-relative commands are affected.

## Candidate fixes considered

1. **Env override only** (`$TOPOLOGY_ROOT`): explicit, but the default `skills/`-only walk-up still
   hijacks unless the variable is set. Insufficient alone.
2. **More specific sentinel**: require `skills/` plus a marker unique to a Topology root. The repo
   root carries `AGENTS.md`, `gatekeeper/`, and `.claude-plugin/`; none sit beside `~/skills`.
   Accepting *any one* of these alongside `skills/` is robust across fork/thin-install modes.
3. **Marker + env override** (chosen): the specific sentinel fixes the default hijack; the
   `$TOPOLOGY_ROOT` override mirrors the existing `$GATEKEEPER_BIN` precedent and gives CI / external
   callers an explicit pin.

## Decision

Adopt option 3. See the spec for the precedence and the testability refactor (the function is
currently untested because it reads process cwd/env directly).
