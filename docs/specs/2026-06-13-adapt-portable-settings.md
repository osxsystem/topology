# Design: portable adapt-generated settings.json (#50 + #51)

- **Date:** 2026-06-13
- **Feature slug:** adapt-portable-settings
- **Status:** approved
- **Research:** [docs/research/2026-06-13-adapt-portable-settings.md](../research/2026-06-13-adapt-portable-settings.md)
- **Issues:** [#50](https://github.com/osxsystem/topology/issues/50) (portable hook paths), [#51](https://github.com/osxsystem/topology/issues/51) (stop pinning GATEKEEPER_BIN)

## Problem

`gatekeeper adapt --harness claude` bakes the **generating clone's absolute paths** into
`.claude/settings.json`: hook `command` strings rooted at `read_root` and `env.GATEKEEPER_BIN =
read_root/bin/gatekeeper`. When that clone/worktree is deleted (routine phase cleanup), the paths go
stale → `PreToolUse:Bash hook error … No such file or directory` in every other clone, and a stale
`GATEKEEPER_BIN` makes `gatekeeper doctor` exit 1. The dogfood clone never even has `bin/gatekeeper`,
so the pin is dead-on-arrival and the hook already falls back to the in-repo build (see research).

Success: a dogfood-generated `settings.json` survives deletion of any sibling worktree —
because it carries no clone-specific absolute path. Governed downstream projects (framework installed
outside the project) are unchanged.

## Constraints

- **Preserve governed behavior (non-negotiable).** When the payload lives outside the project
  (`read_root ≠ write_root`), the absolute hook path and pinned `GATEKEEPER_BIN` are the *documented*
  resolution mechanism (`CONTRACT.md`); `${CLAUDE_PROJECT_DIR}` would point at the wrong tree. The fix
  touches **only** the in-framework branch.
- **`${CLAUDE_PROJECT_DIR}` is location-dependent** (documented asymmetry — the Claude Code hooks
  reference lists the variable for hook `command` strings; the `env` block documents only literal
  values with no substitution mechanism). We **avoid depending on env-block interpolation** by
  *dropping* the `GATEKEEPER_BIN` key in the in-framework case rather than trying to make it relative
  (the hook's own 6-tier fallback resolves the binary), so the fix is robust even if that asymmetry is
  softer than "documented to be literal."
- **Surgical.** Reuse the predicate the code already computes (`roots_differ`, `adapt.rs:817`). No new
  deps, pure-Rust, three-language-lanes preserved.
- **Generated output must pass `gatekeeper scan`** (it writes the protected `.claude/settings.json`).
- **Non-goals:** the doctor stale-path warning (#52); committing the dogfood settings.json (#53);
  **cross-tree dogfood generation** (#54) — when `framework_root` and `project_root` are different
  worktrees of the *same* repo, `roots_differ == true` and adapt treats it as governed; this fix
  deliberately leaves that branch unchanged (see Scope boundary below); quoting hook paths against
  spaces-in-path (pre-existing residual, unchanged today).

## Approaches considered

1. **Key on the existing `roots_differ` predicate (chosen).** In the in-framework case
   (`!roots_differ`, i.e. dogfood): emit hook `command = "${CLAUDE_PROJECT_DIR}/hooks/<name>.sh"` and
   **omit** `env.GATEKEEPER_BIN` (managing it *absent* — `--check` treats its presence as drift, write
   removes it). In the governed case (`roots_differ`): absolute hook paths + pinned `GATEKEEPER_BIN`,
   exactly as today. Trade-off: smallest diff, reuses an existing distinction, governed path provably
   untouched. Cost: two code branches keyed on one bool.

2. **General "framework at-or-under project" via `strip_prefix`.** Also covers a hypothetical
   payload-installed-inside-a-governed-project layout. **Rejected:** no test or installer path
   demonstrates that layout exists today; it widens the blast radius for zero proven benefit
   (YAGNI / surgical-changes).

3. **Always drop `GATEKEEPER_BIN` + always relative.** **Rejected:** breaks governed-external
   (`ac5_gatekeeper_bin_value` + the CONTRACT.md mechanism).

## Design (chosen)

**Predicate.** Reuse `roots_differ` (already computed at `adapt.rs:817`). Define
`in_framework = !roots_differ`.

**#50 — hook paths.** `build_claude_hooks` takes a precomputed `in_framework` flag (Q3 — **`bool`, not
a `&Path`**: in the portable branch no on-disk path is interpolated, so threading a `project_root`
would imply a dependency that does not exist):

```
fn build_claude_hooks(framework_root: &Path, in_framework: bool) -> Result<Value, String>
```

- `in_framework` → `command = format!("${{CLAUDE_PROJECT_DIR}}/hooks/{name}")` for both
  `skill-activation.sh` and `security-scan.sh` (the literal variable is emitted verbatim).
- else → `framework_root.join("hooks/<name>")` absolute (unchanged).

**Upstream-caller fix (G2).** `build_claude_hooks` has a *second* caller at `adapt.rs:536` inside
`build_claude`, reached from the harness match at `adapt.rs:806` — **upstream** of where
`roots_differ` is computed (`817`), and its result is discarded (it exists only to trigger the
AGENTS.md check). Replace that discarded call with `require_agents_md(framework_root)?` directly, so
`build_claude_hooks`'s new signature is only ever reached from the claude branch (`830+`) where
`in_framework` is known. (`build_claude` still returns `Ok(Vec::new())`.)

**#51 — GATEKEEPER_BIN.** `merge_claude_settings` takes `bin: Option<&str>`:

- `Some(b)` → set `env.GATEKEEPER_BIN = b` (governed; current behavior).
- `None` → **remove** `env.GATEKEEPER_BIN` if present; preserve all other `env` keys; if `env` becomes
  `{}` **leave it** (G4 — simplest; the drift check below treats `{}` as steady state so `--check`
  does not flap). Removal — not merely skipping — is what makes a re-`adapt` clear an already-stale
  pin, which the verify gate requires.
- **Intentional contract narrowing (Q2):** in the in-framework case adapt now *deletes* an
  adapt-owned key, slightly narrowing the doc-stated "preserve all other env keys" contract (which
  `ac4` and `merge_settings_preserves_other_env_keys` enshrine for *non-GATEKEEPER_BIN* keys). This is
  deliberate and scoped to the adapt-owned `GATEKEEPER_BIN` only; all user keys are still preserved.

Caller computes `let bin_opt = if roots_differ { Some(bin.as_str()) } else { None };`.

**Drift check (G3).** The real code is a single `disk_ok` closure at `adapt.rs:870–881`. Rewrite it to:
`disk_ok = obj["hooks"] == expected_hooks` (already mode-correct, since `build_claude_hooks` encodes
the mode) **and** the GATEKEEPER_BIN arm: `match bin_opt { Some(b) => env.GATEKEEPER_BIN == Some(b),
None => env.GATEKEEPER_BIN is absent }`. The `None`-branch inversion (a *present* `GATEKEEPER_BIN` is
now drift) is what lets `--check` flag, and the write path clear, an already-stale pin — getting this
backwards either loops on drift forever or never cleans the pin.

## Decisions (resolved in spec review)

- **Q1 — reproduction scope:** roots-equal fix + follow-up #54 for cross-tree (see Scope boundary).
- **Q2 — `GATEKEEPER_BIN` in dogfood:** **remove** (manage-absent). Only removal lets a re-`adapt`
  clean an already-stale pin (the verify symptom). Chosen over "remove-only-if-adapt-shaped" for
  simplicity; the contract narrowing is called out in the #51 design note above.
- **Q3 — signature:** `build_claude_hooks(framework_root, in_framework: bool)`.
- **G4 — empty `env`:** leave `{}` in place.

## Test strategy (TDD targets, red first)

New unit tests (`adapt.rs`):
- `merge_settings_none_bin_removes_gatekeeper_bin` — existing `{env:{GATEKEEPER_BIN:old, MY_VAR:x}}` +
  `None` → `GATEKEEPER_BIN` absent, `MY_VAR` preserved.
- `build_claude_hooks_in_framework_uses_project_dir_var` — equal roots → command starts
  `${CLAUDE_PROJECT_DIR}/hooks/`.
- `build_claude_hooks_governed_uses_absolute` — differing roots → absolute framework path.

New e2e test (`cli_adapt.rs`, roots-equal via `run`):
- `dogfood_settings_are_portable` — generated `settings.json` hook command ==
  `${CLAUDE_PROJECT_DIR}/hooks/security-scan.sh`, contains **no** scratch absolute path, and
  `env.GATEKEEPER_BIN` is **absent**.
- `readapt_removes_stale_gatekeeper_bin` (verify-gate reproduce→resolve) — seed settings with a stale
  absolute `GATEKEEPER_BIN`, run adapt (roots-equal), assert it's gone.

**Update existing tests (the full list):**
- `merge_settings_*` unit tests (`adapt.rs:1422–1465`) → `Option<&str>` signature
  (`Some("/fw/bin/gatekeeper")`).
- **`claude_wires_both_hooks` (`adapt.rs:1087–1100`) — G1, the compile-breaker.** It calls both
  changed functions (one-arg `build_claude_hooks`, `&str` bin) and asserts `s.contains(root)`
  ("Framework root is referenced in hook paths"), which is the *inverse* of the in-framework form.
  Split into two: a governed case (`in_framework=false` → absolute root referenced + `Some` bin) and
  an in-framework case (`in_framework=true` → `${CLAUDE_PROJECT_DIR}/hooks/…`, root **not** referenced,
  `None` bin → no `GATEKEEPER_BIN`).
- **Doc comment (G7):** update `merge_claude_settings`'s doc (`adapt.rs:148–154`), which currently
  describes the old always-set behavior.

Governed e2e (`ac4`, `ac5`, `adapt_writes_to_project_not_framework`) stay **unchanged** and serve as
the no-regression guard.

## Scope boundary & migration

**Scope (Q1).** The logged incident (`dogfood-settings-pinned-to-worktree`) was the **main clone's**
settings pinned to the **`topology-phase12` sibling worktree** — cross-tree generation
(`roots_differ == true`), the governed branch this fix leaves unchanged. So this PR makes the
*intended* dogfood flow portable and is necessary, but **does not by itself close the worktree bug** —
the cross-tree path is tracked separately as **#54** and is an explicit non-goal here. Do not declare
the worktree-portability bug closed on this PR alone.

**Migration (G6).** On the first post-fix `adapt --check`, every existing dogfood clone reports
`DRIFT .claude/settings.json` (hooks now expected in the var form; `GATEKEEPER_BIN` now expected
absent), then self-heals on the next `adapt` write. Blast radius is small and confirmed: no CI
workflow runs `adapt` (zero hits in `.github/workflows`), and the file is untracked
(`git ls-files` empty), so the cost is "devs re-run `adapt` once."

## Verify-gate symptom

Reproduce: in a scratch roots-equal project, generate `settings.json`; show today's output embeds the
absolute scratch path (would dangle if the dir were renamed). Resolve: post-fix output embeds
`${CLAUDE_PROJECT_DIR}/…` and no absolute clone path, and carries no `GATEKEEPER_BIN` to go stale.
