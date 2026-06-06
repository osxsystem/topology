# Design: Security scanning (the deterministic safety floor)

- **Date:** 2026-06-06
- **Feature slug:** security-scanning
- **Status:** approved (2026-06-06)
- **Phase:** Roadmap Phase 1 (front-loaded). See `docs/ROADMAP.md:39`.
- **Provenance:** The maintainers (we) own this design. We took **advisory input** — a Codex overview
  and two Codex review passes — and **evaluated each point ourselves**, adopting what we judged
  correct, scoping or declining the rest. We independently **verified** the one load-bearing external
  fact (the Claude Code hook contract) against the official docs rather than trusting the advice. See
  *Verified facts*, *Advisory input*, and *Advisory reviews — our dispositions*.

## Problem

Topology enforces methodology gates, but it has **no enforcement against the two failure modes that
are catastrophic and irreversible**: an agent committing a **secret** (cloud key, private key) or
running a **destructive command** (`rm -rf /`, `curl … | sh`, a history rewrite). Every other gate
protects *process quality*; nothing protects *safety*. The research report calls this the system's
biggest true gap, which is why the roadmap front-loads it (`docs/ROADMAP.md:20`).

A secret that reaches git history is compromised the moment it is pushed — rotation, not deletion,
is the only remedy. A destructive command cannot be un-run. So the control must be a **veto that
fires before the act**, not advice the agent can rationalize past — the deterministic end of
Topology's enforcement spectrum (`docs/adr/0002`).

**Who it's for.** Anyone running Topology on real work, and the project itself: a coding agent
operating semi-autonomously needs a floor that does not depend on the agent's own judgement.

## Goal — scoped honestly to what the controls deliver

A deterministic, offline, fast safety floor on the **cooperative path**. Stated precisely, because an
over-broad guarantee is a lie:

- **History (the strong guarantee).** The **pre-commit** hook scans the *staged blob content* of
  every added/modified file and **blocks the commit** on a secret/dangerous match — **regardless of
  how the content got there** (a `Write`, an `Edit`, or a shell command like `cp ~/.ssh/id_rsa .`).
  A blob we **cannot** scan (over the size cap, or binary/undecodable) **blocks by default** unless
  it is allowlisted by path **and** content hash. This is the floor's backbone.
- **Working tree (a partial, pre-execution veto).** The **`PreToolUse`** hook blocks, *before the act*:
  (a) dangerous **commands** by pattern, and (b) secrets visible in a **command string** or in a
  **tool-driven file write** (`Write`/`Edit`/`MultiEdit`, scanning the **full reconstructed
  post-edit file**). It **does not** intercept a secret that a **`Bash` command writes to disk** —
  that class is caught at the **pre-commit** boundary above, not pre-execution. *(A `PostToolUse`
  working-tree sweep would close this pre-exec gap; it is a noted future enhancement, deliberately
  not Phase 1.)*
- **Integrity of the safety files.** Edits to the scanner's own rules/hooks/wiring are blocked early
  by `PreToolUse` (cooperative) and — the robust control — by a **pre-commit integrity check** that
  blocks *any* staged modification to a protected path however it was made, unless explicitly
  overridden.

It must not depend on a model, a network call, or the agent's cooperation at decision time.
**Phase 1 is self-hosted** (run inside the Topology repo, where the rules path and the git repo root
coincide); **vendoring into a target project is Phase 6**.

## Constraints

- **Lives in the `gatekeeper` crate** as a `scan` subcommand (`docs/adr/0002`), reusing
  `framework_root()` (`main.rs:66`), the git-shelling pattern of the review/finish gates
  (`git -C <root>`), and the exit convention (`0` pass / `1` veto / `2` usage error —
  `main.rs:46,187`).
- **Dependencies — a deliberate change.** Std-only today (`main.rs:12`). Phase 1 adopts `regex`
  (ReDoS-safe; `RegexSet` one-pass; **`regex::bytes`** for non-UTF8/NUL blobs), `serde` (derive),
  `toml` (rules file), and **`serde_json`** (the `PreToolUse` event — see *No `jq`*). Recorded in
  **ADR-0007**, which *refines* (not reverses) ADR-0002's dependency-free clause; the core scanner
  stays ours/offline; off-the-shelf-scanner-as-core stays rejected (gitleaks/trufflehog become
  **comparison fixtures**). The hand-rolled `json.rs` is **retired** — all JSON parsing moves to
  `serde_json` (rationale under *No `jq`*).
- **No `jq`; a *vetted* parser on the security boundary.** The hook does **not** shell out to `jq`.
  `gatekeeper scan --hook` parses the `PreToolUse` event **in-process with `serde_json`** (into a
  typed `HookEvent { tool_name, tool_input }`) and emits the decision itself, so the bash hook is a
  thin pipe with no parsing dependency and the deny JSON is the *only* stdout. We **retire the
  hand-rolled `json.rs`**: it does not decode `\uXXXX` escapes (it would scan the *wrong bytes* — an
  evasion vector) and recurses without a depth cap (a crafted nested event crashes it). Those gaps
  were harmless for the trusted, ASCII `skill-rules.json` but disqualifying once the parser sits on
  an adversarial, security-critical path. `serde_json` decodes escapes correctly, bounds recursion,
  and shares the `serde` core we already pull in; `skill-rules.json` parsing migrates to it too, so
  one audited parser is used everywhere.
- **No secret through argv or diagnostics.** All scan payloads arrive on **stdin** (never an argv,
  observable via `ps`/`/proc`). The scanner **never emits the matched value** — diagnostics carry
  rule-id, location, and a **redacted** token only (and so does the hook's `permissionDecisionReason`,
  since that is shown to the model + user and saved to the transcript).
- **Fail closed.** When the binary or rules are unavailable, the hooks **deny / abort** with an
  actionable message — never fail open. (Diverges from advisory `skill-activation.sh`, which is
  routing and fails open.) Operational mitigation: `install.sh` builds the binary; Phase 6 CI checks it.
- **Tests stay in the existing std style** (`std::process::Command` + `CARGO_BIN_EXE_gatekeeper`, as
  in `tests/cli_review.rs`). No `assert_cmd`/`predicates`.
- **Non-goals (explicit):**
  - **Not evasion-resistant.** Threat model = the careless/mistaken agent, not an adversary
    obfuscating a command (`r''m -rf /`, `$IFS`) or a novel secret format. Per-command Bash rules to
    catch protected-file mutation are **declined** for this reason — infinite shell variants; the
    pre-commit integrity check is the robust catch instead.
  - **Not a guarantee against hostile bypass.** `git commit --no-verify` skips pre-commit (git-scm) —
    defense-in-depth, not a wall; the `PreToolUse` veto is stronger (the agent can't `--no-verify`
    it). CI / server-side `pre-receive` = Phase 6.
  - **Claude Code built-in tools only** (`Bash`, `Write`, `Edit`, `MultiEdit`). MCP tools and other
    harnesses (Codex `apply_patch`, …) = Phase 4. An MCP tool that writes a file is **not** vetoed
    pre-execution in Phase 1 (but its output is still caught at pre-commit).
  - **Not** an auto-fixer, a history scrubber, an entropy/ML detector, a worktree sweeper
    (`PostToolUse`, future), or a dependency-vuln auditor.

## Approaches considered

1. **Bash + grep regexes in the hooks** — rejected by ADR-0002: non-portable, untestable, no single
   contract, no command-veto home. **Rejected.**
2. **Off-the-shelf scanner (gitleaks) as the core** — heavy dep, no first-class command-veto, less
   control over the exit contract. Kept as a comparison fixture. **Rejected as core.**
3. **`gatekeeper scan` over a versioned `rules.toml`, regex matching, content + command rules, staged
   + hook entry points** *(chosen)* — one fast static path; rules editable as data; testable in Rust.
   Matches ADR-0002 and "few high-signal tools with clear contracts." **Chosen.**

## Decision

**A `gatekeeper scan` subcommand matching a versioned `security/rules.toml` against stdin-delivered
inputs, with `gatekeeper` itself parsing the `PreToolUse` event (no `jq`) and emitting the Claude
deny decision; pre-commit scans full staged blobs and enforces safety-file integrity. Exit
`0`/`1`/`2`. Hooks fail closed.**

### `gatekeeper` surface (payloads on **stdin**)

```
gatekeeper scan --hook                 # stdin = a PreToolUse event JSON; the PreToolUse front door
gatekeeper scan --cmd                  # stdin = one shell command string (content + command rules)
gatekeeper scan --content              # stdin = raw bytes of a file image (content rules)
gatekeeper scan --staged               # pre-commit: scan staged blobs + integrity (two enumerations)
gatekeeper scan --check-path <path>    # exit 1 iff <path> is a protected safety file
```

- **`--hook`** parses the event with `serde_json` (typed `HookEvent`), then by `tool_name`:
  - `Bash` → run `--cmd` logic on `tool_input.command`.
  - `Write` → run `--content` logic on the new file text.
  - `Edit`/`MultiEdit` → **reconstruct the full post-edit file** (read `file_path`, apply the
    replacement(s)) and run `--content` on the result — catching a secret completed across unchanged
    surrounding text. Also runs `--check-path` on `file_path` first (integrity).
  - On a **content/command veto** → print **only** the `deny` JSON (exit 0). On a **protected-path
    edit** → print the `ask` JSON (human-approval dialog — see *Self-protection*). Otherwise no
    stdout, exit 0 (normal permission flow).
- **`--staged`** runs **two separate git enumerations** — their needs differ:
  - *Scan:* `git -C <root> diff --cached --name-only -z --diff-filter=ACMR` → scan each **staged
    blob** (`git show :<path>`), not the textual diff. Deletions are excluded (no content enters).
    Full-blob scanning means a **pre-existing** secret in a touched file also blocks (intended;
    allowlist is the escape).
  - *Integrity:* a **broader** pass — `git -C <root> diff --cached --name-status -z -M
    --diff-filter=ACDMRT` — checking **both sides** of a rename against `protected_paths`. This
    catches a **deletion or rename-away** of a protected file, which the `ACMR` scan filter
    deliberately drops — closing a self-weakening bypass (`rm hooks/pre-commit.sh`).
- Exit `0` clean / `1` ≥1 `block` match / `2` usage or load error. Dispatched from `main.rs:34`;
  listed in `print_help()` and the `//!` header.

### `rules.toml` schema (versioned, validated)

```toml
schema_version = 1

[[rule]]
id = "aws-access-key-id"
kind = "content"          # content | command (validated)
severity = "block"        # block | warn      (validated)
description = "AWS access key ID"
pattern = '\bAKIA[0-9A-Z]{16}\b'

# Allow is RULE-SCOPED and matches the MATCHED SPAN, never the whole line:
[[allow]]
rule = "aws-access-key-id"            # specific id, or "*" — but "*" still REQUIRES a value/pattern
value = "AKIAIOSFODNN7EXAMPLE"        # exact span exempted
reason = "AWS documentation example key"

# Large/binary blobs we cannot scan are blocked unless path + hash allowlisted:
[[allow_blob]]
path = "assets/model.bin"
sha256 = "…"
reason = "known-safe large binary asset"

[integrity]
protected_paths = [
  "security/rules.toml",
  "hooks/security-scan.sh", "hooks/pre-commit.sh",
  "gatekeeper/src/scan.rs", "gatekeeper/src/main.rs",
  "gatekeeper/Cargo.toml", "gatekeeper/Cargo.lock",
  "scripts/install.sh",
  ".claude/settings.json", ".claude/settings.local.json",  # can inject env / disable hooks
]
```

- **Validation → exit `2`, fail loud:** unknown fields (`#[serde(deny_unknown_fields)]`), invalid
  `kind`/`severity`, **duplicate id**, unsupported `schema_version`, uncompilable `pattern`, an
  `allow` (incl. `rule="*"`) without a concrete `value`/`pattern` (so `*` can't become a blanket
  suppressor).
- `block` → exit `1`; `warn` → stderr, exit `0`.
- **Two-pass matching:** a per-kind `RegexSet` says *which* rules hit (one pass); each hit's
  `Regex::find` recovers the **span** (for redaction, location, and span-scoped allow).

### Seed rules (v1)

**Content:** AWS `\bAKIA[0-9A-Z]{16}\b` (+`ASIA`); private-key header `-----BEGIN (RSA |EC |DSA
|OPENSSH |PGP )?PRIVATE KEY-----`; GCP service-account marker; conservative prefix-anchored provider
tokens (`gh[pousr]_`, `xox[baprs]-`, `sk-`). **Command:** `rm -rf /` (+flag-order, targeting `/` not
`/tmp`); `(curl|wget) … | (sh|bash|zsh)`; `git reset --hard`; `git clean -fdx`; `git filter-branch`;
**`git commit --no-verify`/`-n`** (an agent attempt to bypass the pre-commit floor — see
*Self-protection*); a small **`git push` matcher** distinguishing `--force`/`-f` from
`--force-with-lease`. *(Documented limits: `git push +refspec` and rebase-onto-published aren't
reliably regex-expressible — future work, not faked.)*

### Diagnostics & redaction

Per finding: `BLOCK <rule-id>: <description> [<file>:<line> | offset <n>] (redacted: AKIA…<len=20>)`
— rule, location, and a non-reversible hint (prefix+length or truncated SHA-256), **never the value**.
Clean runs are silent. Same redaction governs the deny reason.

### `PreToolUse` hook — concrete contract (verified against official docs)

`hooks/security-scan.sh`, `set -euo pipefail`, resolves the binary as `skill-activation.sh` does,
then pipes stdin to `gatekeeper scan --hook` and passes its stdout/exit through. The load-bearing
facts (verified, `code.claude.com/docs/en/hooks.md`, 2026-06-05):

- A hook **exit code 1 is *non-blocking*** (the tool proceeds); only **exit 2** or a JSON decision
  blocks. So we **never** propagate the scanner's exit 1. On a veto, `gatekeeper` emits, on **stdout,
  exit 0**:
  ```json
  {"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny",
    "permissionDecisionReason":"Topology security veto: <redacted rule-id + location>"}}
  ```
- **Protected-file edit → `ask`, not allow:** a `Write`/`Edit`/`MultiEdit` to a protected path emits
  `permissionDecision: "ask"` (the human-approval dialog). The approval is the proof of intent —
  there is **no env-var override** (an env var can't prove human authorship; it's injectable via
  `settings.json` — see *Self-protection*).
- **Fail closed:** if the binary is missing/errors, the bash wrapper emits a `deny` with reason
  "security scanner unavailable — run ./scripts/install.sh." A broken scanner is **not** overridable
  in-band — you fix the binary.
- **Registration** (`settings.json`, matcher array — replaces the legacy string form at
  `install.sh:31`):
  ```json
  { "hooks": { "PreToolUse": [ { "matcher": "Bash|Write|Edit|MultiEdit",
      "hooks": [ { "type": "command", "command": "<root>/hooks/security-scan.sh", "timeout": 30 } ] } ] } }
  ```
  Matcher is a case-sensitive regex on `tool_name`; per-hook `timeout` (default 600s) set to 30s.

### Pre-commit hook

`hooks/pre-commit.sh`, `set -euo pipefail`: `gatekeeper scan --staged`; non-zero → abort. **Fails
closed** if the binary is missing. A human who must commit a legitimate protected-file change uses
`git commit --no-verify` **at their own terminal** — a human act off the agent's tool path (the agent
is blocked from `--no-verify` by a command rule). Documented as the deliberate, visible escape.

- **Size/binary policy:** the per-blob cap is generous for pre-commit (latency doesn't matter at
  commit; byte-regex over tens of MiB is cheap). A blob over the cap or binary/undecodable is
  **blocked by default** unless `[[allow_blob]]` lists its path + sha256.
- **Symlinks:** not followed (scan the link target *path string*, not the pointee). **Submodules:**
  scan the gitlink entry as a pointer; do not recurse.

### Self-protection (integrity)

The agent edits through the same path the scanner guards, so it could weaken its own guardrails.
`[integrity].protected_paths` covers the rules, both hooks, the scanner source **and** its dispatch
(`main.rs`), the dep manifests (`Cargo.toml`/`Cargo.lock`), and `install.sh`. Enforcement:

- **Early (in-session human gate):** `PreToolUse` returns `permissionDecision: "ask"` for a
  `Write`/`Edit`/`MultiEdit` to a protected path — surfacing the human-approval dialog. The agent
  cannot click it; the human approval **is** the proof of intent. **No env-var override** — an env
  var is ambient, not authored, and is injectable by the agent via `.claude/settings.json` (now
  itself protected), so it cannot prove human authorship.
- **Robust (catch-all at commit):** the **pre-commit integrity enumeration** (the broader
  `--diff-filter=ACDMRT` pass above) blocks *any* staged change to a protected path however made —
  including a **delete or rename-away** the scan filter misses. The human's escape is
  `git commit --no-verify` at their own terminal (off the agent's tool path); the agent is blocked
  from `--no-verify` by a command rule.

A `Bash` command mutating a protected file is **not** reliably caught pre-execution (evasion, out of
scope) but **is** caught by the commit-time integrity enumeration. Every override is a genuine human
act (a dialog approval, or `--no-verify` typed by the human); this is cooperative-path protection,
not hostile-proof. *Caveat:* in fully-autonomous mode (human has globally disabled permission
prompts), `"ask"` may auto-resolve — but there the human has opted out of all gating.

### Dependency & ADR record

`gatekeeper/Cargo.toml` gains pinned `regex = "1"`, `serde = { version = "1", features = ["derive"] }`,
`toml = "0.8"`, `serde_json = "1"`; `Cargo.lock` committed; the hand-rolled `json.rs` is **deleted**.
**ADR-0007** records adopting the four vetted serde-ecosystem / regex crates **and** retiring the
hand-rolled parser (a vetted JSON parser belongs on the adversarial hook boundary); add the row to
`docs/adr/README.md`.

## Performance budget (with acceptance tests, not just targets)

- PreToolUse per-input cap **5 MiB** (latency-sensitive); pre-commit per-blob cap generous (e.g.
  **50 MiB**), over-cap → block-unless-allowlisted.
- `RegexSet` compiled **once** per process; rule count in the low hundreds is fine.
- Targets **and tested**: a typical diff/command **p95 < 150 ms, p99 < 250 ms**; a 5 MiB input
  within budget; a run over a repo with N staged blobs scales linearly. Hook `timeout: 30s` is the
  ceiling.

## Threat model & limitations

- **Cooperative path only**; not a boundary against an actor defeating it.
- **History is the strong net** (pre-commit, content-source-agnostic, unscannable-blobs-blocked);
  **working-tree pre-exec veto is partial** (dangerous commands + visible tool writes; *not*
  Bash-written file content — caught at commit). `PostToolUse` sweep = future.
- **Fail closed**, **stdin-only**, **redacted diagnostics**, **human-gated protected edits** (`ask`
  dialog, no spoofable env override), **self-protection** (in-session + commit-time, catching
  deletes/renames), **no `jq`** — all above.
- **Human `--no-verify`, command obfuscation/evasion, MCP / other harnesses** out of Phase 1 scope
  (Phases 4/6). *(The agent is blocked from `--no-verify`; a human at their own terminal is not.)*
- **Self-scan/dogfooding:** `rules.toml` holds patterns, not literal secrets; **test fixtures build
  planted secrets by concatenation** so the scanner never flags its own source.

## Risks & open questions

- **Hook contract pinned**; residual: confirm `Write`/`Edit`/`MultiEdit` `tool_input` field names at
  implementation time across tool versions. *(Low.)*
- **Edit post-image reconstruction** must mirror Claude's own replacement semantics (first-match vs.
  all, `replace_all`); get it exactly right or a near-miss scans the wrong text. *(Medium — covered
  by tests.)*
- **Full-blob `--staged`** can block a commit over a *pre-existing* secret in a touched file
  (intended; allowlist is the escape). Watch for friction; `warn`-first a noisy rule.
- **`warn` tuning** for generic token rules.
- **`--no-verify` coarseness:** a human committing a legitimate protected-file change with
  `--no-verify` also skips *content* scanning for that one commit; mitigate by running
  `gatekeeper scan --staged` manually first. Accepted for Phase 1.
- **Open (future, not Phase 1):** `PostToolUse` worktree sweep (the Bash-write pre-exec gap); entropy
  detection; `git push +refspec` / rebase-onto-published command coverage.

## Acceptance criteria

- [ ] `gatekeeper scan --hook|--cmd|--content|--staged|--check-path` exist; in `print_help()`, the
      `//!` list, dispatched from `main.rs`. **All payloads via stdin; no `jq`.**
- [ ] **Planted AWS key blocked** (diff/blob/content); stderr shows `BLOCK aws-access-key-id` with a
      **redacted** token — the raw key never appears in any output, incl. the deny reason.
- [ ] **`curl … | sh` blocked**; **`rm -rf /` blocked**, `rm -rf /tmp/build` → `0`.
- [ ] **`git push --force` blocked**, `git push --force-with-lease` → `0`.
- [ ] **Clean passes** → `0`, no stdout/stderr.
- [ ] **Edit completes a secret across unchanged text:** the reconstructed post-edit file is scanned
      and **blocked** (scanning `new_string` alone would miss it).
- [ ] **`--hook` emits exactly one JSON object** (`permissionDecision: deny`) on a veto and **nothing
      else** on stdout; allows are silent, exit 0.
- [ ] **Parser robustness (security boundary):** a dangerous payload whose characters are `\uXXXX`-escaped
      in `tool_input` is **decoded and still blocked** (proves we don't scan the wrong bytes); a
      deeply-nested event JSON is **rejected gracefully** (no crash / stack overflow) and fails closed.
- [ ] **`json.rs` retired:** the file is deleted; `skill-rules.json` now parses via `serde_json` and
      the existing routing tests still pass.
- [ ] **Unscannable blob blocks:** a >cap or binary staged blob → **block**, unless `[[allow_blob]]`
      lists path + sha256 (then `0`).
- [ ] **Allowlist span-scoped:** `AKIAIOSFODNN7EXAMPLE` alone → `0`; a real key on the **same line**
      → `1`. **`allow rule="*"` without a value/pattern → exit `2`** (validation).
- [ ] **Integrity — protected edit human-gated, not env-bypassed:** an `Edit`/`Write` to a protected
      path makes `--hook` emit `permissionDecision: "ask"` (not silent allow; no env override).
      `--check-path security/rules.toml` → `1`; a normal path → `0`.
- [ ] **Integrity — broader enumeration catches delete/rename:** staging a **deletion** or
      **rename-away** of a protected file (`hooks/pre-commit.sh`, `.claude/settings.json`, …) is
      **blocked** by the integrity pass, though the `ACMR` scan filter would skip it.
- [ ] **Bypass commands flagged:** `git commit --no-verify` and `-n` are **blocked** as command rules.
- [ ] **Scanner-unavailable fails closed** and is not overridable in-band (no env var).
- [ ] **Schema validation → `2`:** uncompilable pattern (names id), unknown field, bad
      `kind`/`severity`, duplicate id, bad `schema_version`.
- [ ] **Hardening tests:** large diff (cap), binary, **CRLF**, **NUL**, **symlink** (not followed),
      **submodule** (gitlink, not recursed) behave per the stated policy.
- [ ] **Perf tests:** typical-input p95<150ms/p99<250ms; 5 MiB within budget; linear over N staged
      blobs.
- [ ] `cargo test` covers **each seed rule kind** (≥1 blocked + ≥1 clean), existing std style,
      **fixtures build secrets by concatenation**.
- [ ] `security/rules.toml` exists: `schema_version`, seed rules, span-scoped `[[allow]]`,
      `[[allow_blob]]`, full `[integrity].protected_paths`; `serde` + `deny_unknown_fields`.
- [ ] **`hooks/security-scan.sh`** pipes to `scan --hook`, blocks a real `Bash` `curl … | sh`
      end-to-end via JSON `deny` (redacted reason), **fails closed** with no `jq`; `set -euo pipefail`.
- [ ] **`hooks/pre-commit.sh`** runs `scan --staged`, aborts on veto, enforces integrity, **fails
      closed**, documents `--no-verify`.
- [ ] `skills/security-scanning/SKILL.md` (house format): when the scan fires, how to respond to a
      veto, **not to obfuscate past it**; `hooks/skill-rules.json` light routing keywords.
- [ ] `Cargo.toml` pins `regex`/`serde`/`toml`/`serde_json`; `Cargo.lock` committed; `json.rs`
      deleted; `cargo fmt` + `clippy -- -D warnings` clean.
- [ ] **ADR-0007** + `docs/adr/README.md` row.
- [ ] `install.sh` prints the matcher-array `PreToolUse` config; `ROADMAP.md` Phase 1 → delivered;
      `AGENTS.md`/`README.md` note the floor (and its honest scope: history strong, worktree partial).

## Verified facts (we checked these ourselves)

The **Claude Code `PreToolUse` hook contract**, verified against the official reference
(`code.claude.com/docs/en/hooks.md`, updated 2026-06-05) — not taken on advice: **exit 0** → parse
stdout JSON / else normal flow; **exit 2** → blocking error (stderr shown); **exit 1 / other →
non-blocking** (tool proceeds); deny via `hookSpecificOutput.permissionDecision: "deny"` +
`permissionDecisionReason` (shown to model + user); matcher = case-sensitive regex on `tool_name`
(`Bash|Write|Edit|MultiEdit`, `mcp__server__tool`); per-hook `timeout` (default 600s); stdin carries
`tool_name` + `tool_input` (`.command` for Bash; `.file_path`/content for Write/Edit).

## Advisory input (weighed, not authoritative)

A **Codex** overview (2026-06-06): endorsed the ADR-0002 architecture; supplied the `regex` (RegexSet,
ReDoS-safe) / `toml` + `serde` shortlist; raised the `git commit --no-verify` caveat (git-scm) that
anchors the threat model. **gitleaks / trufflehog / semgrep** referenced — adopted as **comparison
fixtures**, not runtime deps (ADR-0002's no-off-the-shelf-core decision stands).

## Advisory reviews — our dispositions

Two Codex review passes. **We evaluated each point and decided** — adopting what we judged correct,
**scoping or declining** the rest. Codex is an advisor; approval is ours.

**Pass 1 (7 issues, all adopted):** hook block contract (we then *verified* it ourselves); stdin-only
(no argv leak); redacted diagnostics; staged-blob scanning; fail-closed; self-protection; span-scoped
allowlist.

**Pass 2 (6 issues — its valid core: "the guarantee was broader than the controls"):**
1. Large blobs skipped → **adopted**: unscannable blobs block by default + path/hash allowlist.
2. Edit misses cross-text secrets → **adopted**: scan the full reconstructed post-edit file.
3. Bash writes unseen content → **adopted the critique, our fix was to *narrow the guarantee*** (history
   net at pre-commit; worktree veto is explicitly partial) rather than add broad worktree scanning now.
4. Self-protection too narrow → **adopted**: expanded protected paths + a pre-commit integrity check;
   **declined** brittle per-command Bash rules (evasion-prone).
5. Ambiguous override → **adopted, then strengthened in our own review**: instead of env-var controls,
   protected edits use the `ask` human-approval dialog (no spoofable env var); scanner-unavailable
   hard-fails closed.
6. `jq` undeclared dep → **adopted and improved**: parse the event in-process (`scan --hook`),
   eliminating `jq` and centralizing the contract in testable Rust. *(Follow-on maintainer decision,
   prompted by our own review — not Codex: we first reached for the hand-rolled `json.rs`, then
   recognized that parsing adversarial, security-critical input demands a **vetted** parser —
   `json.rs` mishandles `\uXXXX` (evasion) and recurses unbounded (crash). We adopt `serde_json` and
   **retire `json.rs`** entirely.)*

Minors folded: symlink/submodule policy; `--staged` blocks pre-existing secrets (documented);
perf *acceptance tests*; deny-JSON is sole stdout; `allow "*"` validation. Author questions resolved:
block large blobs by default; reconstruct+scan post-edit files; Bash-written content is **out** of
pre-exec scope (caught at commit) — goal narrowed accordingly; **vendored = Phase 6, Phase 1 is
self-hosted**.

**Maintainer review (ours, not Codex).** Three of the sharpest hardenings came from our own
questioning of the draft, not from an advisor: (a) **`json.rs` → `serde_json`** — a hand-rolled
parser can't sit on an adversarial security boundary; (b) **env-var override → `ask` dialog** — an
env var can't *prove* human authorship (it's injectable via `settings.json`), an approval click can;
(c) **one `--staged` enumeration → two** — integrity must catch the deletes/renames the `ACMR` scan
filter drops. The dogfooding is working: the best catches were ours.

We consider the remaining items *convergent* (claim-vs-control wording, now reconciled). Further
review passes hit diminishing returns; **we** will call it plan-ready, accepting the documented
residuals — as the code-review-gate spec did.
