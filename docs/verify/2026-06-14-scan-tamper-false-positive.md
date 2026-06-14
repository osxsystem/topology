# Verify: Scan tamper false-positive fix — Approach 3 (tokenized detection)

- **Date:** 2026-06-14
- **Feature slug:** scan-tamper-false-positive
- **Design:** `docs/specs/2026-06-14-scan-tamper-false-positive.md` (Approach 3, approved) · **Plan:** `docs/plans/2026-06-14-scan-tamper-false-positive.md`

The two regex `tamper-*` command rules were replaced by a quote-aware shell tokenizer
(`detect_path_mutation` in `gatekeeper/src/scan.rs`) driven by `kind = "path-mutation"` rules that carry
only the protected-token lists. Re-run any command below.

## AC1 — reads allowed (the original symptom, resolved)

The original false-positive (a read-only inspection of the wiring, blocked because a verb appeared as a
search string) is gone. Against the live binary:

```
$ printf '%s' 'grep -n "tee" security/rules.toml'           | gatekeeper scan --cmd ; echo $?  → 0
$ printf '%s' 'grep -rn install gatekeeper/src/main.rs'      | gatekeeper scan --cmd ; echo $?  → 0
$ printf '%s' 'grep -rn "fn scan" gatekeeper/src/scan.rs 2>/dev/null' | gatekeeper scan --cmd ; echo $?  → 0
$ printf '%s' 'cat docs/memory/h.handoff.md'                 | gatekeeper scan --cmd ; echo $?  → 0
```

## AC2 — writes block, no false-negative regression (mistake-class)

Every mistake-class write to a protected path blocks. Proven by the integration suite (drives the
shipped `rules.toml` via the `real_ruleset_*` harness) and unit tests:

```
$ env -u TOPOLOGY_ROOT cargo test --manifest-path gatekeeper/Cargo.toml --test cli_scan
test result: ok. 46 passed; 0 failed
$ env -u TOPOLOGY_ROOT cargo test --manifest-path gatekeeper/Cargo.toml --bin gatekeeper path_mutation_tests
test result: ok. 13 passed; 0 failed
```

Covered block-cases (all exit 1; both the wiring and memory token sets): bare verbs; the eight
Approach-1 guards (`>|`, `command tee`, `\tee`, `env FOO=1 tee`, `sudo -- tee`, `nohup cp`); keyword/`)`/`!`
forms (`if true; then cp … fi`, `for … do rm …`, `case …) cp …`, `! tee …`); process substitution incl.
nested writes and a quoted `)`; interior `/./` and `/../`; `sed` short/bundled/long in-place
(`-i`, `-ri`, `--in-place`); flag-carried targets (`cp -tDIR`, `--target-directory=`); fd-redirects
(`2>`, `&>`); path-qualified verbs (`/bin/cp`, `./rm`). The two pre-existing floor tests
(`real_ruleset_blocks_bash_tampering_with_wiring`, `…writes_into_memory_artifacts`) stay green — no
regression.

The seven hardening rounds were each driven by a fresh-context review that found a real bypass the
test suite had missed; see the spec's "Threat-model boundary" + review-round audit trail and
`docs/reviews/2026-06-14-scan-tamper-false-positive.md`.

## AC3 — suite & lint clean

```
$ env -u TOPOLOGY_ROOT cargo test --manifest-path gatekeeper/Cargo.toml      → 579 passed; 0 failed
$ env -u TOPOLOGY_ROOT cargo fmt --manifest-path gatekeeper/Cargo.toml -- --check    → clean
$ env -u TOPOLOGY_ROOT cargo clippy --manifest-path gatekeeper/Cargo.toml --all-targets -- -D warnings → clean
```

## How you know

Re-run any line. The symptom (over-block on read-only inspection) is fixed; every mistake-class write to
the security wiring or the memory artifacts blocks. Out-of-scope per the floor's "mistakes, not a
determined evader" threat model (documented residual, not defects): deliberate shell obfuscation,
runtime-resolved paths (variable/command-sub/glob/brace/tilde/eval/interpreter), arg-taking wrappers, the
conservative `cp <protected-source> dest` over-block, and verbs outside the A3 set (`touch`/`mkdir`/
`git checkout --` — a possible future widening, no regression vs. the old regex).
