# Spec — one-command install

**Status:** approved

## Goal

A brand-new user gets a working Topology — gatekeeper binary included — from **one command**, with
no Rust toolchain required when a prebuilt binary exists for their platform:

```bash
curl -fsSL https://raw.githubusercontent.com/osxsystem/topology/main/scripts/install.sh | bash
```

And a plugin-only user (`/plugin marketplace add osxsystem/topology` → `/plugin install
topology@topology`) gets the binary **self-provisioned** on first session — no separate build step.
Both paths end with a printed manifest of every file the install created or modified, plus the
binary's resolved path.

Grounding: [research](../research/2026-06-10-one-command-install.md),
[ADR-0010](../adr/0010-packaging-distribution.md) (which names this exact upgrade as reversible),
[ADR-0011](../adr/0011-prebuilt-binary-distribution.md) (the decision record for this spec).

## Non-goals

- No `"skills"` field in `plugin.json` — plugin skill auto-discovery already registers `skills/`
  natively (research §plugin-facts #1); adding the field risks double registration. Documented in
  ADR-0011 instead.
- No change to the gate engine, scanner rules, or any Rust behavior beyond the version bump.
- No binary blobs committed to git: "bundled" means *placed into `bin/` at install time* (ignored
  by git) or *provisioned into `${CLAUDE_PLUGIN_DATA}/bin` at session start*.
- No Windows target this pass (no hook support story there yet); recorded as a scope choice.

## Design

### 1. Release matrix (`.github/workflows/release.yml`)

On `v*` tags, build four targets and attach each as `gatekeeper-<target-triple>`:

| Target | Runner |
|---|---|
| `aarch64-apple-darwin` | `macos-14` |
| `x86_64-apple-darwin` | `macos-14` + `--target` cross-compile |
| `x86_64-unknown-linux-gnu` | `ubuntu-latest` |
| `aarch64-unknown-linux-gnu` | `ubuntu-24.04-arm` |

A final job aggregates a `SHA256SUMS` file over all four artifacts and attaches it. The
version-match guard extends to assert tag == `gatekeeper/Cargo.toml` == `.claude-plugin/plugin.json`
== `.claude-plugin/marketplace.json` (plugins entry), failing the release on any divergence.

### 2. Binary resolution (both hooks)

Insert two prebuilt locations into each hook's existing chain, directly after `$GATEKEEPER_BIN`:

- `security-scan.sh`: `$GATEKEEPER_BIN` → **`$ROOT/bin/gatekeeper`** →
  **`$CLAUDE_PLUGIN_DATA/bin/gatekeeper`** → repo release build → repo debug build → PATH → deny.
- `skill-activation.sh`: `$GATEKEEPER_BIN` → **`$ROOT/bin/gatekeeper`** →
  **`$CLAUDE_PLUGIN_DATA/bin/gatekeeper`** → PATH → repo release → repo debug → advisory + exit 0.

The fail-policy split (scan fail-closed, activate fail-open) is untouched. `$ROOT/bin` outranks
`$CLAUDE_PLUGIN_DATA/bin` because an installer-placed binary is an explicit local choice; the
provisioned one is the automatic fallback. `.gitignore` gains `/bin/`.

### 3. Shared fetch helper (`scripts/fetch-gatekeeper.sh`)

One script owns download + verification, used by both the installer and the SessionStart hook.
Contract: `fetch-gatekeeper.sh <dest-dir>` downloads the asset for the current platform into
`<dest-dir>/gatekeeper`, prints the installed path on success, non-zero on any failure.

- Platform map from `uname -s`/`uname -m`: Darwin/arm64 → `aarch64-apple-darwin`, Darwin/x86_64 →
  `x86_64-apple-darwin`, Linux/x86_64 → `x86_64-unknown-linux-gnu`, Linux/aarch64 →
  `aarch64-unknown-linux-gnu`; anything else fails with a "build from source" message.
- Version is **pinned**: read from `.claude-plugin/plugin.json` next to the script's repo root (no
  jq — a `grep`/`sed` extraction, line-anchored). Download URL:
  `https://github.com/osxsystem/topology/releases/download/v<version>/gatekeeper-<triple>`.
- Verification: fetch `SHA256SUMS` from the same release; check with `shasum -a 256 -c` /
  `sha256sum -c` (whichever exists); then run `./gatekeeper --version` as a smoke test; only then
  move into `<dest-dir>` (download happens in a `mktemp -d`, atomic `mv` at the end).
- Test seams: `$TOPOLOGY_RELEASE_BASE_URL` overrides the URL prefix (a `file://` fixture makes the
  whole path testable offline), `$TOPOLOGY_VERSION` overrides the pinned version. `curl
  --fail --silent --show-error --location --max-time 60`.

### 4. Plugin self-provisioning (`hooks/ensure-gatekeeper.sh` + SessionStart)

`hooks/hooks.json` gains a `SessionStart` entry running `ensure-gatekeeper.sh` (timeout 90).

- Fast path: if any resolver location (the §2 chain) yields an executable, exit 0 silently — no
  network, one round of `-x` tests per session.
- Provision path: call `fetch-gatekeeper.sh "${CLAUDE_PLUGIN_DATA:-$ROOT}/bin"`, then print one
  line telling the user what was installed and where.
- Fail-open *for this hook only*: on download failure print an advisory naming the manual fix
  (`scripts/install.sh` or `cargo build`) and exit 0 — a session must still start offline. The
  security floor is unaffected: `security-scan.sh` keeps denying while the binary is absent.

### 5. Installer rework (`scripts/install.sh`)

Pipe-able and checkout-aware:

- **Locate or create the tree.** If `BASH_SOURCE` resolves to a real file inside a checkout, use
  that root (current behavior). Otherwise (piped via curl) clone
  `https://github.com/osxsystem/topology` into `${TOPOLOGY_HOME:-$HOME/.topology}`; if that dir is
  already a clone, `git -C … pull --ff-only` instead.
- **Acquire the binary, prebuilt-first.** Try `fetch-gatekeeper.sh "$ROOT/bin"`. On failure, fall
  back to `cargo build --release` (today's path) and copy the artifact to `$ROOT/bin/gatekeeper`;
  if cargo is also missing, exit with both remedies named. `--build-from-source` flag forces the
  cargo path.
- **Keep the existing steps**: `CLAUDE.md -> AGENTS.md` symlink, `chmod +x` on hooks/scripts, the
  pre-commit hook *copy* (the deliberate non-symlink), the hook-config printout, the adapt/plugin
  notes.
- **Post-install manifest.** Every file the run creates or modifies is appended to a `MANIFEST`
  array at the point of mutation. The script ends with: the gatekeeper path + `--version` output,
  the manifest as absolute paths, the PATH suggestion, and a `gatekeeper doctor` run as the live
  health check.

### 6. Version bump

`0.1.0` → `0.2.0` in `gatekeeper/Cargo.toml` (+ `Cargo.lock`), `.claude-plugin/plugin.json`,
`.claude-plugin/marketplace.json`. The pinned-version download depends on a `v0.2.0` release
existing; until it is tagged, the installer's cargo fallback covers, and the fetch failure message
names the situation. Tagging happens after merge, outside this branch.

### 7. Docs

- `README.md`: the curl one-liner as the headline install, plugin flow as the Claude Code-native
  alternative, both noting no-Rust-required.
- `docs/USER-GUIDE.md`: install section rewritten around the two paths; document
  `$GATEKEEPER_BIN`, `$TOPOLOGY_HOME`, `$TOPOLOGY_RELEASE_BASE_URL`/`$TOPOLOGY_VERSION` seams and
  the resolver order; note that plugin installs get the skills natively via auto-discovery.
- `docs/adr/0011-prebuilt-binary-distribution.md` (+ index link): supersedes ADR-0010 §1's
  "PATH-only" residual and §4's single-target scope; records the trust analysis from the research.

## Acceptance criteria

1. `release.yml` builds all four targets on a `v*` tag, attaches `SHA256SUMS`, and its guard fails
   when the tag disagrees with Cargo.toml, plugin.json, or marketplace.json. (Static check on this
   branch: YAML well-formed, matrix entries present; live run happens at tag time.)
2. With a `file://` release fixture, `fetch-gatekeeper.sh /tmp/dest` installs a binary that passes
   the checksum and `--version` smoke; corrupting the fixture's checksum makes it fail non-zero
   with nothing installed.
3. `printf '{"tool_name":"Bash","tool_input":{"command":"ls"}}' | hooks/security-scan.sh` resolves
   a binary placed at `$ROOT/bin/gatekeeper` ahead of the repo build (observable via
   `GATEKEEPER_BIN` unset + a wrapper binary that tags its output); with **no** binary anywhere it
   still denies. `skill-activation.sh` with no binary still prints the advisory and exits 0.
4. `hooks/ensure-gatekeeper.sh` is silent and network-free when a binary resolves; with none and a
   `file://` fixture it provisions into `${CLAUDE_PLUGIN_DATA:-$ROOT}/bin` and reports the path;
   with none and an unreachable URL it prints the advisory and exits 0.
5. Piped install (`cat scripts/install.sh | bash` from an empty temp dir with `TOPOLOGY_HOME` set,
   fixture URL) produces a working tree + binary and ends with the manifest listing every created
   file as absolute paths; run from inside the checkout it uses the checkout and never clones.
6. `gatekeeper --version` reports `0.2.0`; all three manifests agree.
7. `just check` green (fmt, clippy, tests, shellcheck, typos, docs) and `gatekeeper check docs`
   green; ADR-0011 linked from `docs/adr/README.md`.
