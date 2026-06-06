---
name: security-scanning
description: The deterministic safety floor — a gatekeeper scan that vetoes secrets and dangerous commands before they run or get committed. Use when wiring secret/command scanning, when a PreToolUse or pre-commit veto fires, or when asked about the security rules, allowlist, or protected files.
---

# Security scanning (the safety floor)

A `gatekeeper scan` over `security/rules.toml` blocks two catastrophic, irreversible mistakes: a
**secret** reaching git history and a **destructive command** running. It is deterministic, offline,
and fires *before* the act — not advice you can rationalize past.

## When the veto fires

- **PreToolUse `deny`** — a `Bash` command or a `Write`/`Edit` introduced a secret or a dangerous
  command. **Do not** rephrase to slip past the matcher (the threat model is mistakes, not evasion;
  obfuscating is acting in bad faith). Remove the secret/command, or justify it to the human.
- **PreToolUse `ask`** — you tried to edit a **protected safety file** (the rules, the hooks, the
  scanner, the manifests, `.claude/settings.json`). Only a human can approve it. State *why* the
  change is needed and let them decide.
- **Pre-commit abort** — a staged blob carries a secret, is unscannable (too large / binary), or
  changes a protected file. Fix the staged content. A human — not the agent — may override a
  legitimate change with `git commit --no-verify` at their own terminal.

## Responding to a finding

1. Read the redacted `BLOCK <rule-id>` line: it names the rule and location, never the value.
2. If it is a real secret, **remove it and rotate it** — a pushed secret is compromised.
3. If it is a false positive, add a **span-scoped** `[[allow]]` (rule id + exact value), with a
   reason — never a blanket suppressor.
4. For a known-safe large/binary asset, pin it in `[[allow_blob]]` by path + `blob_oid`
   (`git hash-object <file>`).

## The bar

The scanner is the floor that does not depend on your judgement. Weakening it (editing the rules,
hooks, or binary) is gated behind a human. Honest scope: **history is the strong net** (every staged
blob is scanned at commit); the **working-tree veto is partial** (commands + tool-writes, not
content a `Bash` command writes to disk — that is caught at commit).
