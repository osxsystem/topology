# Design: Scan `tamper-security-wiring` false-positive on read-only inspection

- **Date:** 2026-06-14
- **Feature slug:** scan-tamper-false-positive
- **Status:** approved
- **History:** Approach 1 (regex) approved 2026-06-14; superseded after two fresh-context reviews found
  systematic command-position false-negatives (see "Update — post-review" and "Approach 3" below).
  **Approach 3** (tokenized detection in `scan.rs`) detailed design approved by maintainer
  (Do Viet Hung), 2026-06-14.

## Problem

The deterministic security floor over-blocks. The two command rules in `security/rules.toml`
that guard the security wiring —

- `tamper-security-wiring` (`security/rules.toml:169`)
- `tamper-memory-artifacts` (`security/rules.toml:183`)

— both share the prefix:

```
(>>?|\btee\b|\bcp\b|\bmv\b|\bln\b|\bchmod\b|\brm\b|\bdd\b|\binstall\b|\btruncate\b|sed\s+-[A-Za-z]*i)[^\n]*(<protected-path-token>)
```

This fires `block` whenever a mutating verb **or a bare `>`/`>>` redirect** appears *anywhere*
before a protected-path token — regardless of whether the command actually writes the protected
path. Two benign read-only patterns are therefore vetoed:

1. **Verb-as-search-string.** `grep -n "tee" security/rules.toml` matches `\btee\b … security/rules\.toml`
   — the word `tee` is grep's *pattern*, not a command. Likewise `grep -rn install gatekeeper/src/main.rs`
   (the word `install`).
2. **Co-occurring redirect.** A `>`/`2>`/`>/dev/null` redirect on the same line as a protected-path
   token matches `>>? … <protected>` even though the redirect target is not the protected file.

**Evidence (reproduced live).** During the design study, three read-only `grep` commands auditing
this very subsystem were vetoed as `tamper-security-wiring`. The failure mode is self-defeating:
you cannot `grep` the security scanner's own rules or source without the scanner blocking you.
This is the same coarse-boundary disease class as the open issue **F-001** (quoted-token fail-open),
applied here to fail *closed* (over-block).

Success: a read-only command that merely *names* or *reads* a protected path is allowed, while every
command that actually *writes* a protected path stays blocked. No false-negative is introduced on the
security floor.

## Constraints

- **Rust `regex` crate: no lookaround, no backreferences.** The fix must be expressible as a plain
  regular expression. Negative-lookbehind approaches ("verb not preceded by a quote") are unavailable.
- **Three-language lanes.** Prefer a data-only change in `security/rules.toml` (Markdown/data is the
  source of truth); do not move enforcement logic into a new lane unless required.
- **Security floor — false-negatives are the expensive kind.** Tightening the rule must not let any
  real write to a protected path through. Every existing true-positive assertion in
  `real_ruleset_blocks_bash_tampering_with_wiring` and
  `real_ruleset_blocks_bash_writes_into_memory_artifacts` must stay `block`.
- **Heuristic posture is unchanged.** Both rules are already documented as "raises, does not close"
  the residual (variable-built paths, `eval`, interpreter writes still evade). This change keeps that
  posture; it does not claim to close it.
- **Non-goals.** Not rewriting the scanner; not touching `scan.rs`; not addressing F-001 itself (a
  separate slice); not changing which paths are protected.

## Approaches considered

1. **Precision-tighten both rules in-place (regex-only) — RECOMMENDED.**
   Two changes to the shared prefix, applied identically to both rules:
   - **Redirect → target-bound.** Replace the bare `>>?` branch with `>>?\s*<protected>`: a redirect
     counts only when its *target* is a protected path. This drops the entire `2>/dev/null` /
     `>/dev/null` co-occurrence class, and is *more* precise on real writes (e.g. `cmd 2> security/rules.toml`,
     a stderr redirect that overwrites the rules file, is still caught).
   - **Verbs → command-position.** Require the mutating verb to sit at a command boundary —
     start-of-string or immediately after a separator (`;`, `|`, `&`, `&&`, `||`, `(`, `` ` ``, `$(`),
     with optional `sudo`/`env`/`xargs` wrappers — so a verb word appearing as a *search pattern* or a
     bare *argument* (`grep -n tee file`, `grep "tee" file`) no longer fires. A verb preceded only by a
     space (an argument) is not command position.

   Trade-offs: pure data change, lowest blast radius, stays in the Markdown/data lane. Command-position
   anchoring is itself a heuristic — obscure evasions (`eval "tee …"`, built paths) remain, but those
   are the *already-documented, unchanged* residual.

2. **Redirect-only fix (conservative minimum).** Apply only the target-bound redirect change; leave the
   verb branch matching anywhere. Trade-offs: smaller diff, but **under-fixes** the reported problem —
   `grep "tee" security/rules.toml` (verb-as-search-string) would still block. Rejected: it doesn't
   resolve the live symptom.

3. **Move detection into `scan.rs` (argument-aware parsing).** Tokenize the command, identify the verb
   and its operands / redirect targets, test those against the protected set. Trade-offs: robust and
   closes more of the residual, but it is Rust logic in a *protected* file (`scan.rs`), a much larger
   blast radius and review surface — over-engineered for this slice, and it violates
   "weakest-enforcement-that-works." Deferred; noted as the likely long-term answer to the broader
   F-001 token-boundary class, not this fix.

## Decision

**Approach 1.** It resolves both false-positive classes, keeps every existing true-positive blocking,
adds precision on real writes, stays a data-only change in the correct lane, and preserves the
documented heuristic posture. Both `tamper-security-wiring` and `tamper-memory-artifacts` receive the
identical treatment so they cannot drift.

## Risks & open questions

- **Risk: a tightening opens a hole.** A wrong anchor could let a real write slip (false-negative on the
  security floor). Mitigation: TDD with paired cases — every existing block-case stays `block`, plus new
  block-cases for the precise forms (`cmd 2> security/rules.toml`, `… | tee gatekeeper/src/scan.rs`,
  `sudo tee …`) — watched red→green, and a fresh-context review specifically tasked to find a bypass.
- **Open question (for approval): how many command-position wrappers?** `sudo`/`env`/`xargs` cover the
  realistic privileged/batched writes; `eval`/backtick-built paths stay in the residual. Is that the
  right line, or should the wrapper set be wider/narrower?
- **Residual (expanded after review — see Update below):** indirectly-built paths, `eval`, interpreter
  writes (`python -c "open(...)"`), **arg-taking or exotic command wrappers** beyond the recognized set
  (e.g. `timeout 5 tee security/rules.toml`, where `5` is a positional arg), and **multi-slash path
  tricks** (`security//rules.toml`, the pre-existing F-001 coarse-token-boundary class) still evade both
  rules. This slice narrows the gap but does not claim to close it; closing the class is Approach 3
  (tokenized detection in `scan.rs`), tracked separately.

## Acceptance criteria

Driven against the **shipped** `security/rules.toml` via the `cli_scan.rs` `real_ruleset_*` harness
(`gatekeeper scan --cmd <string>` → exit 0 = allow, exit 1 = block):

- **AC1 — read-only inspection is allowed (the bug, red→green):**
  - `grep -n "tee" security/rules.toml` → exit 0
  - `grep -rn install gatekeeper/src/main.rs` → exit 0
  - `grep -rn "fn scan" gatekeeper/src/scan.rs 2>/dev/null` → exit 0
- **AC2 — real writes still block (no regression):** every assertion in
  `real_ruleset_blocks_bash_tampering_with_wiring` and
  `real_ruleset_blocks_bash_writes_into_memory_artifacts` remains exit 1, plus new precise block-cases:
  - `echo x 2> security/rules.toml` → exit 1 (fd-redirect overwriting the rules file)
  - `cat x | tee gatekeeper/src/scan.rs` → exit 1 (piped `tee` write)
  - `sudo tee security/rules.toml < /tmp/x` → exit 1 (privileged write)
- **AC3 — suite & lint clean:** `cargo test` (full suite) green; `cargo fmt --check` and
  `cargo clippy -- -D warnings` clean.

## Update — post-review (2026-06-14)

The first fresh-context review **failed** the change: it found real false-negatives the narrow regex
introduced, outside the documented residual. Recorded here as the audit trail.

**Found (regressions — blocked by the OLD rule, allowed by the first fix):**
- Force-clobber redirects: `echo x >| security/rules.toml` (and fd-dup `>&`). The redirect branch
  `>>?\s*<P>` could not bridge the `|`/`&` between the operator and the target.
- Wrapped / prefixed verbs: `command tee …`, `\tee …`, `env FOO=1 tee …`, `env env tee …`,
  `sudo -- tee …`, `nohup cp …`. The command-position anchor recognized only `sudo|env|xargs` with
  `-flags`.

**Resolved (maintainer chose "widen regex + document residual"):**
- Redirect branch widened to `>>?[&|]?\s*<P>` — catches `>`, `>>`, `>|`, `>&`, and fd-prefixed forms
  (`2>`, `&>`), while a redirect to a non-protected target (`2>/dev/null`) stays allowed.
- Verb command-position anchor widened: optional leading `\`, a bounded wrapper set
  (`sudo|doas|env|command|builtin|exec|nohup|setsid|time|timeout|nice|ionice|stdbuf|xargs`) with flags
  and optional `--`, `VAR=val` assignments, all repeatable.
- All eight review cases added as red→green regression guards in
  `real_ruleset_distinguishes_reads_from_writes_to_wiring` and
  `real_ruleset_blocks_bash_writes_into_memory_artifacts`.

**Accepted residual** (see the amended Risks bullet): arg-taking/exotic wrappers (`timeout 5 tee …`),
multi-slash path tricks (F-001 class), `eval`, and interpreter writes. The proper closure of this whole
coarse-token-boundary class is Approach 3 (argument-aware tokenization in `scan.rs`), explicitly out of
this slice's scope and tracked as a follow-up.

## Approach 3 — adopted after review (2026-06-14), pending re-approval

A **second** fresh-context review then found a further in-scope regression: shell control-flow keywords
defeat the regex's command-position anchor — `if true; then cp /tmp/x security/rules.toml; fi` and
`for f in a; do rm gatekeeper/src/scan.rs; done` are allowed (confirmed live, exit 0; the old rule
blocked both). The same class includes `case … )` patterns and `!` negation. The regex is trying to do
a parser's job (deciding command-vs-argument across shell grammar); each review finds another
command-position form, and the rule is already ~430 chars. The maintainer chose to **escalate to
Approach 3**: argument-aware detection in Rust.

### Decision
Replace the two regex command-rules with a built-in, quote-aware command detector in `scan.rs`.
Detection logic lives in Rust (enforcement lane); the protected-path token sets stay declarative in
`security/rules.toml` (data/source-of-truth lane); the shell grammar (verbs, wrappers, keywords) lives
in Rust because it is universal, not per-project config.

### Algorithm (in `scan.rs`, run on `--cmd` / `--hook Bash` inputs)
1. **Lex** the command string into words with quote/escape awareness (single-quote, double-quote,
   backslash), and record redirect operators and their target words.
2. **Split** into simple-commands on unquoted separators (`;`, `&`, `&&`, `||`, `|`, newline, `(`, `)`,
   `{`, `}`, backtick, `$(`).
3. For each simple-command, **skip prefix tokens**: shell keywords
   (`if then elif else fi for while until do done case esac select in function time ! coproc`),
   wrapper commands (`sudo doas env command builtin exec nohup setsid time timeout nice ionice stdbuf
   xargs`), `VAR=val` assignments, and flags. The first remaining token is the **verb**.
4. **Block iff**: the verb is a mutating verb (`tee cp mv ln chmod rm dd install truncate`, or `sed`
   with an in-place `-i` flag) AND any of its operand words (path-normalized: collapse `//`, resolve
   `./`) matches a protected token; **OR** any redirect target word (path-normalized) matches a
   protected token.
5. Protected token sets come from the two rule definitions in `rules.toml` (see below).

### What changes in `rules.toml`
The two `kind = "command"` regex rules (`tamper-security-wiring`, `tamper-memory-artifacts`) become a
new `kind = "path-mutation"` carrying a `protected = [ … ]` list of path substrings (the same tokens as
today), interpreted by the Rust detector. No regex `pattern` for these two rules. All other rules
(secrets, `rm -rf /`, git rules) are untouched.

### Why this converges where regex did not
It parses structure instead of matching position, so command-position is computed, not enumerated:
wrappers, keywords, `)`, `!`, and every redirect form fall out of the same logic. Quote-awareness fixes
the original false-positive at the root (`grep "tee" rules.toml` → verb is `grep`, `"tee"` is a quoted
argument → allowed). Path-normalization closes the F-001 multi-slash class.

### Residual (Approach 3)
Only **runtime-resolved** writes evade static lexing and remain the documented floor residual (mistakes,
not a determined evader — matching the contract's threat boundary):
- variable- or command-substitution-built paths (`d=security/rules.toml; cp x $d`, `cp x $(…)`, backticks);
- shell-expansion forms whose result is known only at runtime — brace `{a,b}`, glob `?`/`*`, tilde `~`;
- `eval`, and interpreter writes (`python -c "open(...)"`, `perl -e`, `ed`/`ex`).

Everything **statically visible** is detected: wrappers, control-flow keywords, `case`/`!`, all redirect
forms (`>`, `>>`, `>|`, `>&`, `2>`, `&>`), process substitution (including a write nested inside `>(…)`),
flag-carried write targets (`cp -tDIR`, `--target-directory=DIR`), `.`/`..`/`//` path forms, and
path-qualified verbs (`/bin/cp`, `./rm`, basenamed before matching). Input redirects (`<`) are reads,
not writes.

**Deliberate conservative over-block:** a mutating verb with a protected path in *any* operand position
blocks — exactly right for `mv`/`rm`/`sed -i`/`chmod` (which mutate the path), and a fail-closed
over-block for `cp <protected> dest` (which only reads it). Distinguishing read-source from
write-destination needs per-verb argument semantics (cp's last arg, dd's `of=`, tee's args, …) — added
floor complexity and bug surface not worth one uncommon false-positive. To *read* a protected file, use
a non-mutating verb (`cat`/`grep`/`less`), which is allowed.

**Threat-model boundary (the line we stop at).** The floor targets *mistakes, not a determined evader*
(CONTRACT). Every **mistake-class** write above is detected — a normal command that happens to write a
protected path (`/bin/cp … rules.toml`, `if true; then cp … fi`, `cp -tDIR`, `sed -ri … rules.toml`).
**Deliberate adversarial shell obfuscation** (e.g. burying a write behind a quoted `)` in a process
substitution, or any of the runtime forms above) is explicitly **out of scope**: an agent with a shell
can disable the floor outright (`git commit --no-verify`, rewrite the hook), so chasing every obfuscation
is not the floor's job and would only add bug surface. Shipping here is a deliberate decision, not an
omission.

Seven fresh-context reviews drove this out — the audit trail of how the tokenizer was hardened:
(1) wrapper/keyword/`>|`; (2) keyword-led writes; (3) process-sub + interior `/./`+`/../` + `sed --in-place`;
(4) flag-carried paths + input-redirect false-positive; (5) `sed` bundled in-place flags; (6) path-qualified
verbs + the conservative-over-block decision above; (7) a quoted `)` desyncing the process-sub paren scan.

### Acceptance criteria (Approach 3 — supersede AC1–AC3 above; the prior tests remain and must stay green)
- **A3-1 (reads allowed):** the AC1 cases plus quoted-verb reads (`grep "rm -rf x" security/rules.toml`,
  `cat docs/memory/h.md`, `time cat gatekeeper/src/main.rs`, `command grep tee security/rules.toml`) → allow.
- **A3-2 (writes block — no FN regression):** every existing `real_ruleset_blocks_*` assertion, the eight
  Approach-1 regression guards, **plus** the keyword/`)`/`!` forms:
  `if true; then cp /tmp/x security/rules.toml; fi`, `for f in a; do rm gatekeeper/src/scan.rs; done`,
  `case $x in y) cp /tmp/z security/rules.toml;; esac`, `! tee security/rules.toml` → block. Both the
  wiring and memory token sets.
- **A3-3 (quote-awareness):** a mutating verb inside quotes is treated as an argument, not a command.
- **A3-4 (path-normalization):** `cp /tmp/x security//rules.toml` and `./security/rules.toml` → block.
- **A3-5 (residual documented, not silently closed):** `d=security/rules.toml; cp x $d` and
  `python -c "..."` remain allowed (residual), asserted as such with a comment so the boundary is explicit.
- **A3-6 (suite & lint clean):** full `cargo test` green; `cargo fmt --check` + `cargo clippy -- -D warnings` clean.

### Scope & cost (honest)
This is no longer a one-line slice: new Rust module/function in the **protected** `scan.rs`, a small
shell lexer, a new rule `kind`, and config-parsing for it. Implementation will be delegated to the
`feature-implementer` / `test-engineer-tdd` agents under TDD, with the main loop owning this design and
the fresh-context review. The commit touching `scan.rs`/`rules.toml` is protected (authorized
`--no-verify`, maintainer-run).
