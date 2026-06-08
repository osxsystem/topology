# Verify: Phase 3 — Continuous learning (2026-06-08)

All commands run from the repository root unless noted. All exit 0 / green.

---

## 1. Format check

```
cd gatekeeper && cargo fmt --check
```

Exit 0. No diff.

---

## 2. Clippy (no warnings)

```
cd gatekeeper && cargo clippy --all-targets -- -D warnings
```

Exit 0. No issues found. (`build_promotion`'s knobs are grouped in a `PromoteOpts` struct to stay within
the argument-count lint.)

---

## 3. Full test suite

```
cd gatekeeper && cargo test
```

**Result:** `150 passed, 2 ignored (5 suites, ~1.7s)`

Breakdown:
- `learn` unit tests (ledger parse, slugify, scaffold/validate): **14 passed**
  (`learn::parse_tests` 6, `learn::slug_tests` 4, `learn::scaffold_tests` 4).
- CLI integration (`tests/cli_learn.rs`): **10 passed**.
- Baseline (gatekeeper bin unittests, `cli_instinct`, `cli_review`, `cli_scan`): **126 passed, 2 ignored**
  (the 2 ignored are the `scan::perf_report` wall-clock evidence tests).

---

## 4. No new dependencies

```
git diff origin/main -- gatekeeper/Cargo.toml gatekeeper/Cargo.lock
```

Empty. The ledger parser is hand-rolled on `std`; promotion is add-only (no diff crate); dates arrive via
`--date` (no date crate). Rule validation reuses the existing `pub scan::load_rules`; instinct validation
reuses the existing `parse_instinct` via the new `pub instinct::validate_instinct_str`.

---

## 5. Acceptance criteria (from spec)

The integration suite is the re-runnable proof for each criterion:

```
cd gatekeeper && cargo test --test cli_learn
```

| Criterion | Test |
|---|---|
| capture appends a parseable `## <id>` entry, creating the ledger | `capture_creates_ledger_file_with_entry` |
| recurrence is the same id captured again; `list` counts it | `capture_appends_then_list_counts_recurrence` (→ `again\t2\tskill`) |
| a missing ledger is empty + exit 0 | `list_missing_ledger_is_empty_exit_0` |
| a malformed ledger fails loud (exit 2), naming the offender | `list_on_malformed_ledger_exits_2` |
| `promote --kind instinct` → `instincts/<id>.md` (with `source: ledger:<id>`) **passes `instinct list`** | `promote_instinct_passes_instinct_list` |
| `promote --kind skill` → `skills/<id>/SKILL.md` **appears in `gatekeeper list`** | `promote_skill_appears_in_gatekeeper_list` |
| `promote --kind rule --pattern <re>` appends a `[[rule]]` that **`scan` loads** + vetoes | `promote_rule_loads_under_scan` |
| a declined promotion (no `y`) writes nothing, exit 0 | `promote_requires_confirmation` |
| unknown id / rule without `--pattern` fail loud (exit 2) | `promote_unknown_id_exits_2`, `promote_rule_without_pattern_exits_2` |

---

## 6. Gated-promotion demo (the human-confirmation evidence)

Run against a scratch framework root (`skills/`, `instincts/`, `security/rules.toml`):

```
# 1. capture a gate failure as a proposed instinct, then capture the same id again (a recurrence)
gatekeeper learn capture --summary "Unit tests passing is not the verify gate; record a re-runnable command and its output" \
  --id verify-needs-rerunnable-evidence --trigger gate-failure --gate verify --kind instinct --date 2026-06-08
gatekeeper learn capture --summary "verify gate skipped again on a green unit run" \
  --id verify-needs-rerunnable-evidence --kind instinct

# 2. list shows the recurrence count
$ gatekeeper learn list
verify-needs-rerunnable-evidence	2	instinct

# 3. promote, answer "n" → ABORTS, nothing written
$ printf 'n\n' | gatekeeper learn promote --id verify-needs-rerunnable-evidence
# promotion preview: instinct 'verify-needs-rerunnable-evidence'
#   from ledger entry 'verify-needs-rerunnable-evidence' — trigger manual, 2 occurrence(s)
--- /dev/null
+++ instincts/verify-needs-rerunnable-evidence.md
+---
+id: verify-needs-rerunnable-evidence
+priority: medium
+source: ledger:verify-needs-rerunnable-evidence
+---
+verify gate skipped again on a green unit run
Write this operator? Type 'y' to confirm: promotion aborted: no confirmation, nothing written
  -> exit=0  file_exists=NO

# 4. promote with --yes → writes a valid instinct (priority from --priority high)
$ gatekeeper learn promote --id verify-needs-rerunnable-evidence --priority high --yes
... (same diff) ...
promoted: wrote instincts/verify-needs-rerunnable-evidence.md

# 5. the promoted instinct loads under its own surface
$ gatekeeper instinct list
verify-needs-rerunnable-evidence	high

# 6. a RULE promotion is live under scan
$ gatekeeper learn capture --summary "FIXME-SECRET markers keep leaking into commits" --id leaky-fixme-secret --kind rule
$ gatekeeper learn promote --id leaky-fixme-secret --pattern '\bFIXME-SECRET\b' --severity block --yes
--- security/rules.toml
+++ security/rules.toml
+[[rule]]
+id = "leaky-fixme-secret"
+kind = "content"
+severity = "block"
+description = "FIXME-SECRET markers keep leaking into commits"
+pattern = '\bFIXME-SECRET\b'
promoted: wrote security/rules.toml
$ printf 'oops FIXME-SECRET here\n' | gatekeeper scan --content ; echo $?
1        # vetoed by the promoted rule
```

**Verdict.** Promotion is explicit and previewed: the diff prints first, and the operator is written
**only** on `y`/`--yes` (decline ⇒ nothing written, exit 0). Each promotion produces an operator valid
under that operator's own loader (`instinct list` / `gatekeeper list` / `scan`), validated *before* the
write. This is ADR-0005's "explicit, reviewed action, never silent," enforced by construction.

---

## 7. Phase 3 complete

All acceptance criteria from `docs/specs/2026-06-08-continuous-learning.md` confirmed. Phase 3 status
updated to ✅ in `docs/ROADMAP.md`. The capture→promote loop closes ADR-0005: a failure becomes a
permanent operator exactly where the system got burned, with a human approving every promotion.
