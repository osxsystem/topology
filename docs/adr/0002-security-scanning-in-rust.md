# 0002 — Security scanning lives in the Rust `gatekeeper` crate

- **Status:** Accepted
- **Date:** 2026-06-04

The system needs a deterministic safety floor — detect secrets, dangerous shell commands, and known
vulnerable patterns, and **veto** them before execution and before they enter git history. We will
implement this in Rust as a `gatekeeper scan` subcommand in the existing crate, driven by a versioned
`security/rules.toml`, rather than as a separate service or a pile of Bash regexes.

## Why

Scanning runs on *every* qualifying tool call and on pre-commit, so it must be fast and deterministic,
and it must ship as one static binary. Consolidating into the existing tool matches Anthropic's
"few high-signal tools with clear contracts" guidance and reuses `framework_root()` and the
dependency-free `json.rs`. Exit `1` = veto makes wiring into a `PreToolUse`/pre-commit hook trivial.

## Consequences

- One binary grows a `scan.rs` module; rules live in human/agent-editable, versioned TOML.
- The scan is a veto, not advice — it sits at the deterministic end of the enforcement spectrum.
- Considered and rejected: a separate `sentinel` sibling binary (more to distribute, no benefit at
  this size) and adopting an off-the-shelf scanner (e.g. gitleaks) as the core (heavier dependency,
  less control over the command-veto path — may be *wrapped* later, not made the core).
