# Verify — end-to-end re-verification (Phase 12)

- **Date:** 2026-06-13
- **Spec:** `docs/specs/2026-06-13-e2e-reverification.md` · **Plan:** `docs/plans/2026-06-13-e2e-reverification.md`
- **Harness:** `scripts/test-e2e-reference.sh` (`just test-e2e-reference`) · **Binary:** `gatekeeper 0.9.0`

## Symptom (before)

No artifact proved the five consumer-visible outcomes end-to-end on a real install; `just test-e2e`
covered installer *mechanics* only (payload layout, pre-commit installed, .gitignore), never the
*outcomes* with `--harness claude`. The named reference project `react-weather-app` is not on disk.

## Resolution (after)

A new harness builds a genuine `react-weather-app`-shaped fixture (package.json/src/README + git),
runs the **real** `install.sh` offline (`--build-from-source`), and asserts the five outcomes — after
first proving, on a fresh fixture (the **red baseline**), that they are ABSENT, so the green
assertions cannot be tautological. Run independently by the main loop:

```
$ bash scripts/test-e2e-reference.sh
… (real --project and --global installs) …
test-e2e-reference: 25 passed, 0 failed      # exit 0, offline
```

### Red baseline (AC-1) — outcomes absent pre-install
`no CLAUDE.md @.topology/CONTRACT.md import` · `no .claude/settings.json` · `no .claude/topology/` ·
`no .topology/ payload` · `planted-secret commit SUCCEEDS (no pre-commit hook installed)`. Five PASS.

### `--project --harness claude` — the five outcomes (AC-2..AC-6)
- **O1 contract in context** — `CLAUDE.md imports @.topology/CONTRACT.md`; `.topology/CONTRACT.md`
  exists and renders the governed path `.claude/topology`.
- **O2 bare `gatekeeper` via `GATEKEEPER_BIN`** — binary at `.topology/bin/gatekeeper`;
  `settings.json env.GATEKEEPER_BIN` points at it; `"$GATEKEEPER_BIN" --version` →
  `gatekeeper 0.9.0 (rules schema v1)` **with PATH scrubbed** (`env PATH=/usr/bin:/bin`); `check
  design` also runs PATH-scrubbed (no PATH/sudo step).
- **O3 hooks fire** — `settings.json` wires `UserPromptSubmit`→`skill-activation.sh` and
  `PreToolUse`→`security-scan.sh`; invoking each directly: skill-activation emits an advisory block /
  exit 0; security-scan emits `"permissionDecision":"deny"` on a planted secret.
- **O4 project pre-commit blocks a planted secret** — `git commit` exits non-zero, output carries a
  scanner BLOCK line, HEAD unchanged; the documented skip-hooks bypass then lands the commit.
- **O5 design artifact under the project** — `doctor` resolves artifacts root to
  `<fixture>/.claude/topology`; a planted approved spec (+ research note) under
  `.claude/topology/specs/` makes `check design --feature x` PASS, reading from the project root.

### `--global` scope (AC-7)
payload + `VERSION` at `$TOPOLOGY_HOME/.topology`; `bin/gatekeeper` present; `doctor` from a separate
neutral project resolves via `GlobalHome` (`resolved by: global ~/.topology`); binary `--version`
`0.9.0` == payload `VERSION` `0.9.0` (no skew).

## Replayable evidence

```evidence
$ just test-e2e-reference
# expect: 25 passed, 0 failed
```

## No binary change (AC-9)

The Phase 12 diff (`d088830..HEAD`) touches only `scripts/test-e2e-reference.sh`, `justfile`,
`.github/workflows/ci.yml`, and the `docs/` artifacts — **no `gatekeeper/src/**`, `Cargo.toml`, or
`Cargo.lock`**. No version bump, no release tag. `shellcheck` clean on the new script.

## Known fidelity notes (non-blocking, for review)

- O2/O5 pass `TOPOLOGY_ROOT` explicitly to the project payload; a real Claude Code session resolves
  it via vendored/binary-adjacent precedence instead. The outcome (artifacts/contract anchored to the
  project) is still genuinely proven; the env pin only removes test flakiness.
- O3 proves the hooks fire by invoking the wired scripts directly (deterministic, CI-able) rather
  than driving a live Claude Code session — the documented spec non-goal; a live-session smoke test
  is a possible follow-up.

## Gate status

research ✓ · design ✓ (PASS) · plan ✓ (PASS, baseline 501) · tdd ✓ (harness red baseline → green;
proves itself) · finish ✓ (`just test-e2e-reference` 25/0, exit 0; full `cargo test` unaffected,
501/6) · shellcheck clean · `check docs` ok.
