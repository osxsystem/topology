# Architecture Decision Records

Short, durable records of the load-bearing decisions behind the Topology operator system. An ADR earns
its place when a decision is **hard to reverse**, **surprising without context**, and **the result of
a real trade-off**.

| # | Decision | Status |
|---|---|---|
| [0001](0001-extend-topology-in-place.md) | Extend Topology in place rather than rebrand or rewrite | Accepted |
| [0002](0002-security-scanning-in-rust.md) | Security scanning lives in the Rust `gatekeeper` crate | Accepted |
| [0003](0003-one-markdown-source-per-harness-adapters.md) | One Markdown source; generate per-harness configs | Accepted |
| [0004](0004-instincts-vs-gates.md) | Instincts are a distinct operator class from gates | Accepted |
| [0005](0005-continuous-learning-capture-gotcha.md) | Continuous learning via capture-gotcha + approved promotion | Accepted |
| [0006](0006-code-review-gate.md) | The code-review gate is a commit-bound, fail-closed critic artifact | Accepted |
| [0007](0007-security-scanner-dependencies.md) | The security scanner adopts vetted crates (regex/serde/serde_json/toml) and retires the hand-rolled JSON parser | Accepted |
| [0008](0008-cross-harness-adapter-mappings.md) | 0008 — Cross-harness adapter mappings | Accepted |
| [0009](0009-memory-research-first-hardening.md) | 0009 — Memory artifacts as markdown; research as a gated stage | Accepted |
| [0010](0010-packaging-distribution.md) | 0010 — Packaging & distribution: system-PATH binary, CI mirrors the justfile, hand-authored plugin | Accepted |
| [0011](0011-prebuilt-binary-distribution.md) | 0011 — Prebuilt-first binary distribution: release matrix, installer download, plugin self-provisioning | Accepted |
| [0012](0012-project-root-vs-framework-root.md) | 0012 — Project root vs framework root: artifacts move to `.claude/topology/` in governed projects | Accepted |
| [0013](0013-payload-read-only-artifacts-root-state.md) | 0013 — The payload is read-only at runtime; mutable state anchors to the artifacts root | Accepted |
| [0014](0014-dispatch-table-over-clap.md) | 0014 — One dispatch table over clap for the CLI surface | Accepted |
