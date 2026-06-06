# 0007 — The security scanner adopts vetted crates and retires the hand-rolled JSON parser

- **Status:** Accepted
- **Date:** 2026-06-06

ADR-0002 put security scanning in the `gatekeeper` crate and called it "dependency-free (std only)".
Building Phase 1 (docs/specs/2026-06-06-security-scanning.md) showed that clause is wrong for an
adversarial, security-critical path. This ADR **refines** ADR-0002: it keeps the core scanner ours
and offline, and rejects an off-the-shelf scanner as the core, but adopts four vetted crates and
retires the hand-rolled parser.

## Decision

- **Adopt `regex`, `serde` (derive), `serde_json`, `toml`.** `regex` gives a ReDoS-safe, one-pass
  `RegexSet` and a `bytes` API for non-UTF8/NUL blobs; `serde`/`toml` parse and *validate* the
  versioned rules file; `serde_json` parses the `PreToolUse` event in-process (no `jq`).
- **No hashing dependency.** `[[allow_blob]]` pins an unscannable blob by its **git object id**
  (`git rev-parse :<path>`), reusing the git we already shell to. Redaction uses prefix + length.
- **Retire `json.rs`.** The hand-rolled parser does not decode `\uXXXX` (it would scan the wrong
  bytes — an evasion vector) and recurses without a depth cap (a crafted event crashes it). Harmless
  for the trusted, ASCII `skill-rules.json`; disqualifying on the adversarial hook boundary.
  `serde_json` decodes escapes, bounds recursion, and shares the `serde` core — so one audited parser
  is used everywhere (`skill-rules.json` routing migrates to it too).
- **Off-the-shelf-scanner-as-core stays rejected** (ADR-0002). gitleaks/trufflehog/semgrep are
  comparison fixtures, not runtime deps.

## Consequences

- The binary gains four well-known, offline-buildable crates; `Cargo.lock` is committed.
- For a security tool the calculus inverts: a hand-rolled parse bug is a worse risk than serde_json's
  well-audited supply-chain surface, so the vetted parser is the safer choice on adversarial input.
- `json.rs` and its two unit tests are deleted; routing behavior is unchanged and still tested.
