VERDICT: pass
HEAD: 631da21c717ca0d28f81fa00b1c3625e67e54d7d
BASE: 8ea06d825957afb081e1dccfc53510474e5a101d

# Review: memory-research-first (2026-06-08)

Reviewed by an independent fresh-context critic (a different model — Sonnet — per the code-review
skill), auditing `git diff 8ea06d8...HEAD` against the spec, plan, ADR-0009, and repo standards.
Tooling-enforced checks (fmt/clippy/tests) were skipped by design — the finish gate covers them.

The first pass returned **fail** with one blocking finding: the `design` gate's `cmd_check` arm exited
`1` (stdout, research-first message) instead of the usage error `2` (stderr) on a missing `--feature`,
violating AC1 ("Missing `--feature` exits `2`"). It was fixed in `631da21` (an `f.is_empty()` guard
before `find_doc`, mirroring `gate_doc_exists`, plus a `design_gate_missing_feature_exits_2` test) and
re-verified by a focused fresh-context re-review — exit `2` on stderr, the sequence-lock unchanged, 199
tests green. This artifact is bound to the corrected HEAD.

## Blocking findings
None.

## Non-blocking notes
- `gatekeeper/src/memory.rs:239-257` — the `status: done` check requires `--verified-by` to be non-empty but resolves the note via `find_doc("verify", &slug)` using the **feature** slug, not the `--verified-by` value. Code matches the plan (Task 2: `find_doc("verify", feature)`); the spec's "`verified_by` resolves to …" wording is looser. Effect: any non-empty `--verified-by` passes once the feature's verify note exists. Latent inconsistency, not a correctness bug.
- `gatekeeper/src/scan.rs` — `scan_bytes_for_secrets` is `pub` rather than `pub(crate)`; harmless in a binary crate (no external consumers), could be narrowed for clarity.
- `gatekeeper/tests/cli_scan.rs` — the `--check-path` protection for `memory/artifacts/*` and the `artifacts-evil` collision are covered by the `is_protected` unit tests in `scan.rs` but not by a CLI-level integration case; the end-to-end `scan_check_path → is_protected` path for the new entry is exercised only in the verify note, not the suite.

## Criteria checked
### Spec/plan
- AC1 (research gate exists *and* blocks design; missing `--feature` exits 2) — MET after `631da21`. `main.rs:227` research arm (exit 1/0/2 correctly); `main.rs:228-243` design arm now guards `f.is_empty()`→2 before the `find_doc("research")` lock (→1) and the fall-through to `gate_doc_exists("specs")`. Tests: `cli_check.rs` `design_gate_fails_without_research_note_even_with_spec` (lock), `..._passes_after_both_...`, `design_gate_missing_feature_exits_2` (the AC1 fix), `research_gate_missing_feature_exits_2`.
- AC2 (handoff round-trips) — MET. `cmd_write` (`memory.rs:147-275`) → `memory/artifacts/<slug>.handoff.md`; `cmd_read` (`memory.rs:277-300`). `cli_memory.rs:66` byte-equality + stamped fields; `cli_memory.rs:122` unknown slug → 1.
- AC3 (write hygiene on the *rendered* artifact) — MET. `memory.rs:223-236` scans `rendered.as_bytes()` before any write. `cli_memory.rs:133` body secret → 1, file absent; `cli_memory.rs:163` secret via the stamped `--verified-by` field → 1, file absent.
- AC4 (format template present and parses) — MET. `memory/TEMPLATE.handoff.md` tracked; `memory.rs` `template_parses_through_frontmatter_parser` round-trips it via `include_str!`.
- AC5 (`list` read-only and accurate) — MET, with recorded deviation: the `kind` axis was cut (single artifact kind), so `list` shows `slug · created · status`. `cmd_list` (`memory.rs:302-327`); `cli_memory.rs:347`.
- AC6 (no new dependency; suite green) — MET. `gatekeeper/Cargo.toml`/`Cargo.lock` unchanged across the branch; suite 199 passed / 2 ignored.
- AC7 (`code-review` subagent returns findings) — MET, re-asserted. The `review` gate (`cli_review.rs`) is green; this very artifact is the per-feature review, and its first pass returned a real blocking finding (AC1) — the gate demonstrably works.
- AC8 ("done" tied to verify evidence) — MET (see non-blocking note on the `verified_by` value). `memory.rs:239-257`; `cli_memory.rs:201-271` (no note → 1; note present → 0); default status `in-progress`.
- AC9 (protection guards the tree, not siblings, resists aliases) — MET. `is_protected` (`scan.rs:492-500`) uses component-wise `Path::starts_with` on lexically-resolved paths; unit tests (`scan.rs:1312-1396`) cover file-inside, absolute, `..`-alias, trailing-slash, exact-match, and the `artifacts-evil` collision (not protected). Symlink/case residual documented.
- AC10 (input validated) — MET. `validate_id` (`memory.rs:157`), `is_iso_date` (`memory.rs:168`), double-frontmatter reject (`memory.rs:200-209`); `cli_memory.rs:276-343` all three, nothing written.
- AC11 (Bash residual documented, not silently passed) — MET, stronger than specified. `11cb956` broadened the tamper rule to `severity=block` (`rules.toml`); the PreToolUse Bash path emits `deny`. `cli_scan.rs` `real_ruleset_blocks_bash_writes_into_memory_artifacts` proves redirect/`cp`/`tee`/`mv` denied; indirect/interpreter/symlink residuals documented in the verify note.

### Standards
- ADR-0009 §1 (no new Cargo deps) — `Cargo.toml`/`Cargo.lock` unchanged on the branch.
- ADR-0009 §3 (markdown not an engine; gatekeeper-owned writes) — `memory.rs` stamps structured fields from flags (never the body), scans the rendered artifact, and owns the only write path; protection via `is_protected`/`scan_hook` + the tamper rule, with documented residuals.
- ADR-0009 §4 (gitignored artifacts) — `/memory/artifacts/` in `.gitignore`; `README.md`/`TEMPLATE.handoff.md` committed, artifacts runtime-only.
- AGENTS.md three-language lanes — Rust confined to `gatekeeper/src/`; Bash untouched; new Markdown in `docs/`/`memory/`/`skills/`. No lane crossings.
- AGENTS.md surgical-changes-only — additions are focused (new files + targeted edits to `scan.rs`/`main.rs`); no orphan refactors in the diff.
- AGENTS.md `mod` ordering — `main.rs`: `adapt, instinct, learn, memory, review, scan` — alphabetical.
- AGENTS.md test style — integration tests use `env!("CARGO_BIN_EXE_gatekeeper")` + `std::process::Command` (`cli_check.rs`, `cli_memory.rs`, `cli_scan.rs`); no `assert_cmd`/`predicates`; pure-function unit tests in-module.
- AGENTS.md skill description format — `research-first` and `resume` both follow `<verb phrase>. Use when <triggers>.`
- Honest residuals / no overclaiming — the verify note records the two deviations (AC5 `kind` cut, AC11 broadened to deny), the `/tmp` symlink demonstration, and the three open residuals; no protection guarantee beyond what the code enforces is claimed.
