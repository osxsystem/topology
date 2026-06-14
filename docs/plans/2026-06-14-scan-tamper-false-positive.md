# Plan: Scan `tamper-security-wiring` false-positive fix

- **Date:** 2026-06-14
- **Feature slug:** scan-tamper-false-positive
- **Design:** `docs/specs/2026-06-14-scan-tamper-false-positive.md` (approved)

## Baseline

`env -u TOPOLOGY_ROOT cargo test --quiet --manifest-path gatekeeper/Cargo.toml --test cli_scan`
→ `test result: ok. 44 passed` (recorded 2026-06-14). Clean.

## Files to touch

| File | Responsibility | Protected? |
|------|----------------|------------|
| `gatekeeper/tests/cli_scan.rs` | New test distinguishing reads from writes to the wiring | No |
| `security/rules.toml` | Tighten the two `tamper-*` command-rule patterns | **Yes** (`[integrity].protected_paths`) |

## The exact patterns (GREEN target)

Shared shape: `(?:>>?\s*<P>)|(?:<CMDPOS>(?:<VERB>)[^\n]*<P>)` where
`CMDPOS = (?:^|[\n;&|(\x60])\s*(?:(?:sudo|env|xargs)\s+(?:-[A-Za-z]*\s+)*)?` and
`VERB = (?:tee|cp|mv|ln|chmod|rm|dd|install|truncate)\b|sed\s+-[A-Za-z]*i`.

**`tamper-security-wiring`** (`security/rules.toml:169`) `pattern` becomes (single-quoted TOML literal):

```
(?:>>?\s*(\.git/hooks/|hooks/(pre-commit|security-scan)\.sh|security/rules\.toml|gatekeeper/(src/|Cargo\.)|\.claude/settings))|(?:(?:^|[\n;&|(\x60])\s*(?:(?:sudo|env|xargs)\s+(?:-[A-Za-z]*\s+)*)?(?:(?:tee|cp|mv|ln|chmod|rm|dd|install|truncate)\b|sed\s+-[A-Za-z]*i)[^\n]*(\.git/hooks/|hooks/(pre-commit|security-scan)\.sh|security/rules\.toml|gatekeeper/(src/|Cargo\.)|\.claude/settings))
```

**`tamper-memory-artifacts`** (`security/rules.toml:183`) `pattern` becomes:

```
(?:>>?\s*(docs/memory/|\.claude/topology/memory/))|(?:(?:^|[\n;&|(\x60])\s*(?:(?:sudo|env|xargs)\s+(?:-[A-Za-z]*\s+)*)?(?:(?:tee|cp|mv|ln|chmod|rm|dd|install|truncate)\b|sed\s+-[A-Za-z]*i)[^\n]*(docs/memory/|\.claude/topology/memory/))
```

## Tasks

### Task 1 — RED: add the discriminating test

Append to `gatekeeper/tests/cli_scan.rs` (after `real_ruleset_blocks_bash_writes_into_memory_artifacts`, before `real_ruleset_protects_active_git_hook`):

```rust
#[test]
fn real_ruleset_distinguishes_reads_from_writes_to_wiring() {
    // Read-only inspection of the wiring must be allowed; only commands that actually WRITE a
    // protected path block. Regression guard for the tamper-* false-positive fix
    // (docs/specs/2026-06-14-scan-tamper-false-positive.md).
    let root = scratch_root("real_tamper_rw");
    fs::copy(real_rules_toml(), root.join("security").join("rules.toml")).unwrap();
    let cmd = |s: &str| run(&root, &["scan", "--cmd"], s.as_bytes()).0;

    // Allowed: a mutating verb word appearing as a grep PATTERN or argument, not a command.
    assert_eq!(cmd("grep -n \"tee\" security/rules.toml"), 0, "verb as search string");
    assert_eq!(cmd("grep -rn install gatekeeper/src/main.rs"), 0, "install as search arg");
    // Allowed: a redirect whose TARGET is not a protected path, co-occurring with a protected token.
    assert_eq!(cmd("echo done > /tmp/out; grep -rn foo gatekeeper/src/main.rs"), 0, "redirect to unprotected");
    assert_eq!(cmd("grep -rn \"fn scan\" gatekeeper/src/scan.rs 2>/dev/null"), 0, "stderr to /dev/null");

    // Blocked: a redirect whose TARGET is a protected path (including an fd redirect).
    assert_eq!(cmd("echo x 2> security/rules.toml"), 1, "fd-redirect overwriting rules");
    // Blocked: a command-position verb writing a protected path (piped / privileged).
    assert_eq!(cmd("cat x | tee gatekeeper/src/scan.rs"), 1, "piped tee write");
    assert_eq!(cmd("sudo tee security/rules.toml < /tmp/x"), 1, "privileged tee write");

    let _ = fs::remove_dir_all(&root);
}
```

- **Run:** `env -u TOPOLOGY_ROOT cargo test --manifest-path gatekeeper/Cargo.toml --test cli_scan real_ruleset_distinguishes_reads_from_writes_to_wiring`
- **Expected (RED):** test FAILS; the three "Allowed" assertions on lines `verb as search string`,
  `install as search arg`, and `redirect to unprotected` get exit `1` (currently blocked) where `0` is
  expected. The block-cases pass (they already block under the old rule — they are forward regression
  guards).

### Task 2 — GREEN: tighten the two rule patterns

In `security/rules.toml`, replace the `pattern = '...'` line of `tamper-security-wiring` (`:169`) with
the first pattern above, and the `pattern = '...'` line of `tamper-memory-artifacts` (`:183`) with the
second. No other lines change.

- **Run:** `env -u TOPOLOGY_ROOT cargo test --manifest-path gatekeeper/Cargo.toml --test cli_scan`
- **Expected (GREEN):** `test result: ok. 45 passed` (44 prior + the new test), `0 failed`. In
  particular `real_ruleset_blocks_bash_tampering_with_wiring` and
  `real_ruleset_blocks_bash_writes_into_memory_artifacts` stay green (no regression).

### Task 3 — REFACTOR/lint

- **Run:** `env -u TOPOLOGY_ROOT cargo fmt --manifest-path gatekeeper/Cargo.toml -- --check` and
  `env -u TOPOLOGY_ROOT cargo clippy --manifest-path gatekeeper/Cargo.toml --tests -- -D warnings`
- **Expected:** both exit `0` (clean). No production Rust changed; only the test file and the data file.

### Commit (one cycle)

Test + rule change land together (red→green→refactor = one commit). `security/rules.toml` is a
**protected path**, so the commit requires an authorized `--no-verify` — and `git commit --no-verify`
is itself blocked by the `git-commit-no-verify` scan rule. **The maintainer runs the commit** (e.g.
`! git commit --no-verify ...`), keeping the human as the authority over the floor bypass (the
documented threat boundary). Proposed message:

```
fix(scan): allow read-only inspection of protected wiring (slice 1)

The tamper-security-wiring and tamper-memory-artifacts rules matched a mutating
verb or bare redirect anywhere before a protected-path token, vetoing read-only
commands (grep "tee" security/rules.toml; grep install gatekeeper/src/...) and
any line with a co-occurring redirect. Tighten both: a redirect counts only when
its target is a protected path, and a verb counts only at command position
(start or after a separator, optional sudo/env/xargs). Real writes still block.

Closes the live false-positive; same token-boundary class as F-001 (separate).

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
```

## Verify / review / finish (after the cycle)

- **Verify:** re-run the three live-blocked `grep` forms through the built binary, show exit `0`; show a
  real write still exit `1`; full `cli_scan` suite green. Record at
  `docs/verify/2026-06-14-scan-tamper-false-positive.md`.
- **Review:** fresh-context critic, tasked to find a bypass (a real protected-path write that now
  passes). Artifact at `docs/reviews/2026-06-14-scan-tamper-false-positive.md`.
- **Finish:** full repo test suite + fmt + clippy (`just check`).
