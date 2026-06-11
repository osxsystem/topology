# Changelog

Earlier releases (≤ v0.4.0) predate this file; see the GitHub releases page for their artifacts.

## v0.4.1 — 2026-06-11

Payload-only patch: scan-rule additions and their regression harness. No gatekeeper code changes
([spec](docs/specs/2026-06-11-day-zero-containment.md), ROADMAP Phase 13).

### Security rules (`security/rules.toml`)

- New `jwt-structural` rule (**block**): three dot-joined base64url segments with the `eyJ`
  sigil — catches bearer JWTs regardless of labeling.
- Broadened `openai-key` (**block**): now tolerates hyphenated segment prefixes and
  underscore-bearing tails (`sk-proj-…`, `sk-ant-…`); rule id unchanged so existing allowlists
  stay valid.
- New `labeled-secret-assignment` rule (**warn**): credential-labeled assignments
  (api key / auth token / password / bearer followed by a 16+-char value). Warn posture is
  deliberate — promotion to block is a Phase 2 decision made on bench data.

### Tests

- Secrets benchmark (`gatekeeper/tests/cli_scan_bench.rs` + `tests/fixtures/secrets-bench/`):
  eleven positive classes assembled at runtime (no secret-shaped literals in the tree), six
  literal negative fixtures. Asserts the 9/11 in-scope detection floor with rule-id attribution
  and zero false positives on the negatives — the standing regression wall for every future rule.

### Ops & docs

- GitHub push protection enabled on the repository (host-side backstop); bypass flow documented
  in the user guide's Security scanning section.
- Process-weight baseline recorded in `docs/research/2026-06-11-process-weight-baseline.md`
  via the new `scripts/metrics.sh` — the FM1 denominator for the remediation track.
- Ships the `check` usage-text fix that landed on `main` just after the v0.4.0 tag.
