---
id: three-language-lanes
priority: high
source: doc:ARCHITECTURE
---
Put each change in its lane — Markdown is the source of truth, Rust enforces, Bash only glues. Never
bridge a behavior across lanes (no logic in Bash, no enforcement in Markdown).
