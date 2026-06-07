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
