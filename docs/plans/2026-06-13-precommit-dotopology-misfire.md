# Plan: fix pre-commit hook blocking commits on a stray `.topology/` (issue #60)

- **Date:** 2026-06-13
- **Feature slug:** precommit-dotopology-misfire
- **Design:** docs/specs/2026-06-13-precommit-dotopology-misfire.md
- **Baseline:** tests green at commit `6a90466` (gatekeeper: 552 passed, 0 failed, 5 ignored — only
  docs added on this branch since the `8e19256` merge).

## Commit mechanics (read first)

Both files edited here are **protected paths**, so the pre-commit scan vetoes the staged diff. The
maintainer **authorized `--no-verify` for this #60 fix** (design doc, Landing mechanics). Every commit
below uses `git commit --no-verify` and records the override in its message. `--no-verify` skips the
pre-commit hook entirely, so these commits need **no** `TOPOLOGY_ROOT` workaround. The `Edit` calls to
the protected files will trip the `PreToolUse` **ask** gate (human approval) — expected.

## Files

- `gatekeeper/src/main.rs` — add the negative-gate `resolve_root` unit test (the one uncovered branch).
- `hooks/pre-commit.sh` — delete the `TOPOLOGY_ROOT` export block; replace with a plain-prose why-comment.

## Tasks

### Task 1: Negative-gate `resolve_root` unit test

- **File(s):** `gatekeeper/src/main.rs` (protected → `Edit` triggers the ask gate).
- **Change:** add this test immediately after `resolve_root_self_governed_beats_project_vendored_topology`
  (ends `main.rs:2343`):
  ```rust
  #[test]
  fn resolve_root_rejects_non_marked_vendored_topology() {
      // Negative gate for #60: an UNMARKED project root with a NON-marked .topology/ child must
      // NOT resolve as ProjectVendored — is_marked_root (the gate at the ProjectVendored step)
      // rejects the stub, so resolution falls through to Fallback. This is the branch the
      // stub-.topology safety relies on, and the one no prior test exercises (the SelfGoverned
      // tests short-circuit at step 2 before reaching it).
      let base = env::temp_dir().join("topology_nonmarked_vendored");
      let _ = fs::remove_dir_all(&base);
      fs::create_dir_all(&base).unwrap();
      // base is NOT a marked root (no skills/). Its .topology child is also non-marked: a lone
      // CONTRACT.md, mirroring the framework repo's deliberate contract-split stub.
      let vendored = base.join(".topology");
      fs::create_dir_all(&vendored).unwrap();
      fs::write(vendored.join("CONTRACT.md"), "stub\n").unwrap();
      // Empty home (no <home>/.topology) so the GlobalHome step can't fire — isolate the gate.
      let fake_home = base.parent().unwrap().join("home_nonmarked");
      let _ = fs::remove_dir_all(&fake_home);
      fs::create_dir_all(&fake_home).unwrap();

      // exe_path = None so the BinaryAdjacent step cannot pre-empt the ProjectVendored gate.
      let result = resolve_root(&base, None, None, Some(&fake_home));
      assert_ne!(
          result.source,
          RootSource::ProjectVendored,
          "a non-marked .topology/ stub must NOT be selected as ProjectVendored (#60)"
      );
      assert_eq!(
          result.source,
          RootSource::Fallback,
          "with no marked root anywhere, resolution must fall through to Fallback"
      );
      let _ = fs::remove_dir_all(&base);
      let _ = fs::remove_dir_all(&fake_home);
  }
  ```
- **TDD honesty:** characterization — the `is_marked_root` gate already returns correctly for this
  false branch, so the test passes immediately. It pins a *previously-uncovered* branch (the property
  the Bash deletion relies on), not new behavior. Not red→green theater.
- **Test:** `cargo test --manifest-path gatekeeper/Cargo.toml resolve_root_rejects_non_marked_vendored_topology`
  → expect `test result: ok. 1 passed`.
- **Commit:** `git commit --no-verify -m "test(root): pin negative gate — non-marked .topology not ProjectVendored (#60)"`
  with a body recording the authorized protected-path override.

### Task 2: Delete the `TOPOLOGY_ROOT` export from the hook

- **File(s):** `hooks/pre-commit.sh` (protected → `Edit` triggers the ask gate).
- **Change:** replace lines 28-32 (the 2-line comment + the `if` block):
  ```bash
  # Governed project: the framework root is the vendored .topology, passed via env — never via cd;
  # the scan must run from the COMMITTED repo so the staged lane targets this repo's index.
  if [[ -z "${TOPOLOGY_ROOT:-}" && -d "$ROOT/.topology" ]]; then
    export TOPOLOGY_ROOT="$ROOT/.topology"
  fi
  ```
  with this plain-prose comment (no command-like tokens, so it does not trip a scanner rule):
  ```bash
  # Do not pin TOPOLOGY_ROOT here. The binary resolves the framework root itself via its
  # is_marked_root ladder: a self-governed clone resolves to the repo, and a governed project
  # resolves to its marked .topology payload (reached binary-adjacent from .topology/bin). Pinning
  # it on the bare existence of a .topology directory misfires when that directory is a deliberate
  # non-marked stub, blocking every commit (issue #60).
  ```
  Leave the binary-finder ladder (`:13-26`) and `cd "$ROOT" && "$GK" scan --staged` (`:34`) untouched.
- **Test:** `bash -n hooks/pre-commit.sh` (syntax OK, exit 0) and `just shell` (shellcheck clean). Then
  a direct resolution check: `env -u TOPOLOGY_ROOT ./gatekeeper/target/release/gatekeeper scan --staged`
  from the repo root → exit 0 (resolves self-governed, loads the repo's rules). End-to-end through the
  *deployed* hook is the verify gate.
- **Commit:** `git commit --no-verify -m "fix(hooks): drop redundant TOPOLOGY_ROOT pin so a non-marked .topology does not block commits (#60)"`
  with a body recording the authorized protected-path override.

### Task 3: Finish-style regression — `just check`

- **File(s):** none (verification only).
- **Test:** `just check` (fmt-check + clippy + test + shell + typos + docs — the full CI gate, per the
  finish-gate convention, not just `cargo test`). Run with `TOPOLOGY_ROOT` unset for the test step.
  Expect: all clean; `cargo test` adds 1 test to the 552 baseline → `553 passed, 0 failed`.
- **Commit:** none.

## Done criteria (maps to spec acceptance criteria)

- AC (deployed-hook commit succeeds): verify gate — `just setup` to redeploy, stage a trivial
  non-protected change, `git commit` succeeds where it previously failed.
- AC (governed no-regression, vendored-binary layout / Step 3): verify gate fixture.
- AC (negative-gate unit test): Task 1.
- AC (hook no longer pins TOPOLOGY_ROOT, why-comment present): Task 2.
- AC (no change to install.sh / resolve_root logic; binary-finder ladder + cd+scan untouched): Tasks 1-2 diff scope.
- AC (commit records the authorized --no-verify override): Task 1 + Task 2 commit messages.
