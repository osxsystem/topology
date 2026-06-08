# RTK — the default shell proxy

RTK ("Rust Token Killer") is a token-optimizing shell proxy for Claude Code. It intercepts noisy
command output — long `git diff` blocks, verbose `cargo` traces, wide `ls` listings — and condenses
or filters it before Claude sees it. In practice this yields **60–90 % token savings** on routine
dev/CLI operations, keeping the context window lean for the work that actually matters.

RTK is an **optional productivity layer**. It is not a `gatekeeper` surface and plays no role in
the enforced gate/scan machinery. Topology works without it; with it, long-running agent loops stay
leaner longer.

## How it is wired

A Claude Code **command-rewrite hook** (`UserPromptSubmit`) transparently prepends `rtk` to bare
shell commands before they execute:

```
git status          →   rtk git status
cargo test          →   rtk cargo test
cat some/long/file  →   rtk cat some/long/file
```

The rewrite is transparent — zero token overhead, no change to exit codes or file side-effects. The
hook lives in Claude Code's `settings.json` (or `settings.local.json`) and runs client-side; it is
never part of the Topology gate sequence.

## Meta commands (run directly)

A handful of RTK commands are introspective and bypass the rewrite:

```bash
rtk gain              # token-savings analytics for the current session
rtk gain --history    # per-command savings history
rtk discover          # scan Claude Code history for missed optimisation opportunities
rtk proxy <cmd>       # execute a raw command without RTK filtering (debug escape hatch)
```

Run these directly — they are not wrapped by the hook.

## Install and opt-in

RTK is opt-in. Install it from the user's own toolchain; Topology does not bundle or require it.
After installing the binary, enable the rewrite hook by adding the `UserPromptSubmit` hook entry to
your Claude Code settings. The `rtk --version` / `which rtk` / `rtk gain` triplet confirms a
working install (see the note below about name collisions).

Topology's `auto-loop.settings.json` already carries `rtk`-prefixed variants in its `deny` list
alongside the bare variants, so the guard rails apply regardless of whether the hook is active.

## Name-collision caveat

Two unrelated tools share the `rtk` binary name:

| Binary | Project | Purpose |
|--------|---------|---------|
| `rtk` (this one) | token-killer proxy | condenses shell output for LLM contexts |
| `rtk` | `reachingforthejack/rtk` ("Rust Type Kit") | unrelated Rust utility |

If `rtk gain` fails with "command not found" or an unrecognised subcommand, verify you have the
correct binary: `rtk --version` should report a version consistent with the token-killer, and
`rtk gain` should succeed.

## Relationship to Topology gates

RTK is a **productivity tool**, not a gate. The `gatekeeper` binary does not call `rtk`, does not
require it, and does not check for it. Enabling or disabling RTK has no effect on gate outcomes.
The only connection is that `auto-loop.settings.json` mirrors its `deny` patterns so that the same
destructive-command guardrails hold whether a command runs bare or through `rtk`.
