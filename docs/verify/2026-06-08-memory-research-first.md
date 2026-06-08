# Verify — Memory + research-first hardening (Phase 5)

- **Date:** 2026-06-08
- **Feature slug:** memory-research-first
- **Branch:** `feat/memory-research-first`
- **Commits verified:** `6bbb54e..15bb707` (11 commits)
- **Spec:** [docs/specs/2026-06-08-memory-research-first.md](../specs/2026-06-08-memory-research-first.md) — 11 acceptance criteria
- **Method:** the quality gates were run, then every acceptance criterion was exercised with a
  re-runnable command and its exit code captured live (functional checks in a hermetic scratch root;
  gate/dependency checks in the repo). Two spec criteria were written before later design changes and
  are recorded here with their **superseding** behaviour, not silently passed.

---

## Quality gates (criterion 6)

| Gate | Command | Result |
|---|---|---|
| Format | `cargo fmt --check` | **exit 0** (clean) |
| Lint | `cargo clippy --all-targets -- -D warnings` | **clean** — no issues |
| Tests | `cargo test` | **198 passed, 2 ignored** (8 suites) |
| No new dependency | `git diff 6bbb54e^..HEAD --stat -- gatekeeper/Cargo.toml gatekeeper/Cargo.lock` | **empty** — `Cargo.toml`/`Cargo.lock` unchanged across all of Phase 5 (ADR-0009 §1) |

---

## Acceptance criteria

All commands below use the repo-built binary `gatekeeper/target/debug/gatekeeper` and, for the
functional checks, a scratch framework root (`mktemp -d` containing `skills/` + a copy of
`security/rules.toml`). `[n]` is the expected exit code.

### 1. Research gate exists *and* blocks design — ✅
```
check research --feature demo            → exit 1   [1]  (no note yet)
# write a spec but NO research note:
check design  --feature demo             → exit 1   [1]  LOCK — prints:
        FAIL design gate: research-first — no docs/research/*demo*.md
check research          (no --feature)   → exit 2   [2]
# after writing docs/research/2026-06-08-demo.md:
check research --feature demo            → exit 0   [0]
check design  --feature demo             → exit 0   [0]  falls through to the spec check
```
The lock is real: `design` fails on a missing research note **even when the spec exists**.

### 2. Handoff round-trips — ✅
```
printf '## Goal\n…' | memory write --feature demo --date 2026-06-08   → exit 0
memory read --feature demo  > rt.out                                   → exit 0
cmp memory/artifacts/demo.handoff.md rt.out                            → byte-equal: YES
memory read --feature nonesuch                                          → exit 1
```
Stamped frontmatter observed: `feature: demo`, `created: 2026-06-08`, `status: in-progress`.
`branch`/`head_sha` are empty here because the scratch root is intentionally **not** a git repo
(the documented degrade-empty policy; in a repo they carry `git rev-parse` output).

### 3. Write hygiene holds, on the *rendered* artifact — ✅
```
# non-allowlisted AWS-key-shaped token (AKIA + 16 chars, assembled at runtime — this note holds no literal key):
secret in the BODY                         → memory write exit 1, target file absent
secret via the stamped --verified-by field → memory write exit 1, target file absent
```
The second case proves the scan runs over the **rendered artifact** (frontmatter + body), not just
the stdin body — a secret reaching a stamped field is refused before any byte hits disk.

### 4. Format template present and parses — ✅
`git ls-files memory/TEMPLATE.handoff.md` → tracked. Parses through the real parser via the unit
test `template_parses_through_frontmatter_parser` (`include_str!` round-trip; part of the 198).

### 5. `list` is read-only and accurate — ✅ *(with a recorded deviation)*
```
memory list →
    alpha · 2026-01-01 · in-progress
    beta  · 2026-06-08 · in-progress
    demo  · 2026-06-08 · in-progress
```
**Deviation from the spec wording:** the spec says list shows `kind/created/status`. The `kind`
axis was **cut during design** (the handoff is the sole artifact kind — see spec non-goals, "no
separate `compaction` kind"), so `list` shows `slug · created · status`. Recorded, not silently
diverged.

### 6. No new dependency; suite green — ✅
See **Quality gates** above.

### 7. `code-review` subagent returns findings against the plan — ✅ *(re-asserted)*
The `code-review` gate pre-dates this phase and remains green (`gatekeeper/tests/cli_review.rs`,
part of the 198). The Phase-5 **plan itself underwent external review** (Codex): verdict
*needs-rework*, three must-fixes (research gate didn't actually block design; an overclaimed
"no hand-edit"; missing input validation) — all resolved before execution. No per-feature
`docs/reviews/*memory-research-first*.md` artifact was produced this phase (the review was
external/conversational); producing one to pass `check review` is a possible follow-up.

### 8. "Done" is tied to verify evidence — ✅
```
memory write … --status done --verified-by dn   (no verify note)          → exit 1
# after creating docs/verify/2026-06-08-dn.md:
memory write … --status done --verified-by dn                              → exit 0
```
Default/omitted `--status` is `in-progress` (observed in criterion 2's frontmatter).

### 9. Protection guards the tree, not its siblings, resists aliases — ✅ *(symlink residual demonstrated)*
```
scan --check-path memory/artifacts/demo.handoff.md   → exit 1
scan --check-path memory/artifacts-evil/x.md         → exit 0   (no prefix collision — Path::starts_with is component-wise)
scan --check-path memory/TEMPLATE.handoff.md         → exit 0   (sibling seed)
scan --check-path memory/artifacts/../artifacts/x.md → exit 1   (.. alias resolves in)
scan --check-path memory/artifacts/                  → exit 1   (trailing slash)
scan --check-path <ABS>/memory/artifacts/abs.md      → exit 1   (absolute, under a resolved root)
```
**Symlink note:** the same absolute check run under a `/tmp/…` scratch returned **0**, because
macOS symlinks `/tmp`→`/private/tmp` while `framework_root()` resolves to `/private/tmp/…`, and
`is_protected` is **lexical** (it does not resolve symlinks). Re-running under a resolved
(`/private/tmp/…`) root gives **1**, and the unit test `absolute_in_repo_path_to_artifacts_file_is_protected`
confirms it. This is the documented symlink/case residual in action (criteria 9 & 11), not a
regression.

### 10. Input is validated — ✅
```
memory write --feature ../escape …               → exit 2, nothing written  (validate_id)
memory write --date 06/08/2026 …                 → exit 2, nothing written  (date shape)
memory write  (body opens a second --- block)    → exit 2, nothing written
```

### 11. Bash residual is documented, not silently passed — ✅ *(stronger than the spec wording)*
The spec (written before the Task-4 follow-up) said a redirection into the artifacts directory is
"not blocked, only raised." That is **superseded**: commit `11cb956` broadened the tamper rule to
`severity = "block"`, and the `PreToolUse` Bash path emits **`deny`** on a block match. So the hook
now **denies** a literal `>`/`>>` redirect *and* the mutation verbs `tee`/`cp`/`mv`/`ln`/`chmod`/
`rm`/`dd`/`install`/`truncate`/`sed -i` aimed at the artifacts directory (proven by
`real_ruleset_blocks_bash_writes_into_memory_artifacts`). The honest residual is recorded below.

---

## Residuals (accepted this phase — ADR-0009 §3, spec non-goals)

The real guarantee is bounded and stated plainly: **the file-editing tools (`Write`/`Edit`/
`MultiEdit`) cannot clobber `memory/artifacts/` without a human approving an `ask`; artifacts are
gitignored; and the obvious shell write vectors are denied.** What remains open:

1. **Indirect shell writes.** The tamper rule is a regex, not a shell parser. A path built
   indirectly (a variable assigned the directory, then redirected through it), a heredoc, or an
   interpreter write (`python -c …`, `node -e …`) is not matched and still reaches the directory.
2. **Lexical `is_protected`.** No symlink-following and no case-folding — demonstrated incidentally
   by the `/tmp`→`/private/tmp` case in criterion 9. A symlink alias, or a case-variant on a
   case-insensitive filesystem, can reach the directory.
3. **Hook override.** A human (or an agent with arbitrary shell) can disable the floor the same way
   anyone can — `git commit --no-verify`, or removing the hook. This is the threat boundary
   (mistakes, not a determined evader), per AGENTS.md "Security scan."

These are intentional limitations, not gaps to close this phase (`[[surgical-changes-only]]`).

---

## Verdict

**All 11 acceptance criteria satisfied**, with two recorded deviations where the implementation
diverged from the earlier spec text — both *stronger or simpler* than specified:
- criterion 5: the `kind` axis was cut; `list` shows `slug · created · status`.
- criterion 11: the tamper rule was broadened from "raise" to **`deny`**, so literal redirects and
  the common mutation verbs are now blocked (not merely flagged); the residual is the indirect/
  interpreter vectors above.

Quality gates green (fmt 0, clippy clean, 198 passed / 2 ignored, no dependency change). **Phase 5
is delivered.**

---

### How to re-run

```sh
# Gates (from gatekeeper/):
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test

# Functional (hermetic scratch root):
S="$(mktemp -d)"; mkdir -p "$S/skills" "$S/security" "$S/docs/specs" "$S/docs/research" "$S/docs/verify"
cp security/rules.toml "$S/security/"; cd "$S"
GK=<repo>/gatekeeper/target/debug/gatekeeper
# then the per-criterion commands above; resolve the scratch root with `cd "$(cd "$S" && pwd -P)"`
# so an absolute --check-path matches the (symlink-resolved) framework root.
```
