# 0014 — One dispatch table over clap for the CLI surface

- **Status:** ✅ Accepted (spec approved `a9928a1`; ships with `feat/hollow-pass-kills`)
- **Date:** 2026-06-11

## Context

The v0.4.0 release demonstrated FM3 (doc/binary drift): the same command surface exists in four
hand-maintained copies — the dispatch match in `main.rs`, nine `USAGE_*` constants, the separate
`print_help()` literal, and the README/USER-GUIDE command tables. Commit `39710a0` fixed a usage
string but missed the tag; nothing diffs the copies and nothing runs at the tag. The
[hollow-pass-kills spec](../specs/2026-06-11-hollow-pass-kills.md) (§2, §6) eliminates the class:
one source of truth for the binary's surface, plus a sync test that diffs the docs against it in
CI and in the release version-guard.

Two candidate mechanisms for the single source of truth:

1. **clap (derive or builder)** — the ecosystem default; help, dispatch, and flag validation
   generated from one definition.
2. **A hand-rolled static table** — `static SUBCOMMANDS: &[SubcommandSpec]` with
   `name`/`usage`/`synopsis`/`known_flags`/`handler`, iterated by both `main()` dispatch and
   `print_help()`.

## Decision

**The table.** clap would solve the duplication too, but it drags a dependency tree into a binary
whose security posture is partly *being auditable*: ADR-0007 fixed the dependency budget at four
crates (regex, serde, serde_json, toml) precisely so the scanner that gates other code is itself
reviewable. The table is ~100 LOC of std with no new failure modes, preserves the
`check_help_or_unknown` contract exactly — `Some(0)` on help, `Some(2)` on unknown flag, `None`
to proceed (characterization-pinned by `cli_help_flags.rs`, which must stay green unmodified;
output diffs are confined to the spec §2 enumerated sanctioned list), and gives
`cli_doc_sync.rs` a trivially well-formed structure to diff the docs against.

What clap would buy us — typed flag parsing, shell completions, derived validation — is not worth
a dependency-budget exception for a CLI with nine subcommands and a handful of flags. If the
surface ever grows past what a flat table holds cleanly (deep nesting, repeated flag groups), that
is the signal to revisit this ADR rather than stretch the table.

## Consequences

- `grep -c 'const USAGE' gatekeeper/src/main.rs` → 0; help and dispatch cannot disagree because
  both iterate the same data.
- Adding a subcommand is one table row plus a handler; forgetting the docs is a CI failure
  (`cli_doc_sync.rs`), not a silent drift.
- Flag *values* remain hand-parsed inside handlers (as today, e.g. `feature_arg`) — the table
  governs the surface, not argument semantics. That residual duplication is accepted; it has
  never been the drift vector.
- ADR-0007's four-dependency constraint stands unmodified.
