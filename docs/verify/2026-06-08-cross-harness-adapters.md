# Verify: Cross-harness adapters (Phase 4)

- **Date:** 2026-06-08
- **Feature slug:** cross-harness-adapters
- **Design:** docs/specs/2026-06-08-cross-harness-adapters.md · **Plan:** docs/plans/2026-06-08-cross-harness-adapters.md
- **Branch:** `feat/cross-harness-adapters` (from `origin/main` `5aa172a`)

All commands below are re-runnable from a clean checkout.

## 1. Quality gates

```
$ cd gatekeeper && cargo test
test result: ok. 81 passed; 0 failed; 2 ignored   # bin unittests (incl. 7 adapt::tests)
test result: ok.  8 passed; 0 failed; 0 ignored   # tests/cli_adapt.rs   (new)
test result: ok.  8 passed; 0 failed; 0 ignored   # tests/cli_instinct.rs
test result: ok.  1 passed; 0 failed; 0 ignored   # tests/cli_review.rs
test result: ok. 43 passed; 0 failed; 0 ignored   # tests/cli_scan.rs
# => 141 passed, 2 ignored, across 5 suites

$ cargo clippy --all-targets -- -D warnings   # 0 warnings/errors
$ cargo fmt --check                            # clean (no diff)
```

No new dependencies: `gatekeeper/Cargo.toml` and `Cargo.lock` are unchanged on this branch
(`git diff origin/main -- gatekeeper/Cargo.toml gatekeeper/Cargo.lock` is empty).

## 2. Harness generation + validation against real tools

Generate every harness from the real repo content (9 skills, 6 instincts) into a scratch root, then
validate each output with the harness's own tooling where available:

```
$ SR=$(mktemp -d); cp AGENTS.md "$SR/"; cp -R skills instincts hooks "$SR/"
$ (cd "$SR" && for h in codex cursor opencode claude; do gatekeeper adapt --harness "$h"; done)
generated codex / cursor / opencode / claude
```

| Criterion (from the spec) | Command | Result |
|---|---|---|
| **Codex** config loads under strict-config | `CODEX_HOME=<copy> codex exec --strict-config --skip-git-repo-check "noop"` | **PASS** — no schema rejection; `codex 0.137.0` parses the generated `.codex/config.toml` (advances to the auth stage). |
| Codex sets no denylisted key | inspect `.codex/config.toml` | only assignment is `project_doc_max_bytes = 1048576`; `profile`/`model_provider`/`notify` appear (if at all) only in the explanatory comment. |
| **OpenCode** `opencode.json` valid JSON | `python3 -c "import json; json.load(open('opencode.json'))"` | **PASS** — keys `['$schema','instructions']`; `instructions = ["AGENTS.md", ".opencode/instincts.md"]`. |
| OpenCode skills copied verbatim | `cmp skills/_getting-started/SKILL.md .opencode/skills/getting-started/SKILL.md` | **byte-equal** (and the dir is keyed by frontmatter `name`, not `_getting-started`). |
| **Cursor** rules generated | `ls .cursor/rules/` | **11** files: `agents-contract.mdc` + `instincts.mdc` (both `alwaysApply: true`) + `skill-<name>.mdc` ×9 (`alwaysApply: false`, a `description`, no `globs`). |
| **Claude** `.claude/settings.json` valid JSON | `python3 -c "import json; json.load(...)"` | **PASS** — hook events `['PreToolUse','UserPromptSubmit']` in the loadable array-of-matcher-groups schema. |

> Note on scope of "loads": Codex is validated by its own binary. Cursor and OpenCode binaries are not
> installed in this environment, so their outputs are validated as **well-formed in the documented
> schema** (valid JSON; the `.mdc` rule-mode fields per cursor.com/docs/rules; the `opencode.json`
> `$schema` + `instructions` per opencode.ai/docs) rather than by launching the live editor. The schemas
> were captured in docs/research/2026-06-08-cross-harness-adapters.md.

## 3. Idempotency (the `--check` gate)

```
$ (cd "$SR" && gatekeeper adapt --harness opencode --check); echo $?
up to date (11 file(s))
0                                   # clean re-render → exit 0

$ printf 'tampered\n' >> "$SR/opencode.json"
$ (cd "$SR" && gatekeeper adapt --harness opencode --check); echo $?
DRIFT opencode.json
1                                   # a mutated artifact is flagged → exit 1
```

Re-generating is idempotent, and drift is detectable (the CI hook for Phase 6).

## 4. Error paths (exit 2)

Covered by `tests/cli_adapt.rs` and `adapt::tests`: an unknown `--harness`, a missing `--harness`, and a
missing `AGENTS.md` each exit `2` with a clear message.

## Verdict

Every acceptance criterion in the spec is met. The four `gatekeeper adapt --harness <h>` targets each
generate their native config from the one Markdown source; the Codex config is validated by the real
`codex` binary; JSON outputs parse; copied skills are byte-equal; `--check` is idempotent and flags
drift. Suite green (141 passed), clippy/fmt clean, zero new dependencies. Phase 4 is delivered.
