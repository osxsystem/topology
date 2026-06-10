# Plan — one-command install

Spec: [docs/specs/2026-06-10-one-command-install.md](../specs/2026-06-10-one-command-install.md).
Decision record: [ADR-0011](../adr/0011-prebuilt-binary-distribution.md).
Branch: `feat/one-command-install` (worktree `topology-install`). One commit per task, conventional
prefixes. Every shell change must pass `shellcheck` (repo `.shellcheckrc`); finish with `just check`.

## Conventions for all tasks

- Bash: `set -euo pipefail`, POSIX-friendly where practical (AGENTS.md stack conventions).
- The fail-policy split is inviolable: `security-scan.sh` denies when no binary resolves;
  `skill-activation.sh` prints its advisory and exits 0. No task may alter those terminal branches.
- Test seams over mocks: `$TOPOLOGY_RELEASE_BASE_URL` (URL prefix, `file://` allowed) and
  `$TOPOLOGY_VERSION` (overrides the manifest-pinned version) are the only injection points.
- No binary is ever committed: task 2 adds `/bin/` to `.gitignore` before any task can create one.

## Task 1 — Version bump to 0.2.0

- Edit `gatekeeper/Cargo.toml` `version = "0.2.0"`; run `cargo build` once so `Cargo.lock` updates;
  commit the lockfile change.
- Edit `.claude-plugin/plugin.json` and `.claude-plugin/marketplace.json` (the `plugins[0].version`
  entry) to `0.2.0`.
- Check: `gatekeeper --version` prints `gatekeeper 0.2.0 (rules schema v1)`; `grep -r "0\.1\.0"`
  over `Cargo.toml`, `plugin.json`, `marketplace.json` is empty.

## Task 2 — Release matrix + widened version guard

- Rewrite `.github/workflows/release.yml`:
  - `build` job with `strategy.matrix.include` of the four (target, runner) pairs from the spec
    (`aarch64-apple-darwin`/`macos-14`, `x86_64-apple-darwin`/`macos-14`,
    `x86_64-unknown-linux-gnu`/`ubuntu-latest`, `aarch64-unknown-linux-gnu`/`ubuntu-24.04-arm`);
    `rustup target add ${{ matrix.target }}`, `cargo build --release --locked --target …`, stage as
    `gatekeeper-${{ matrix.target }}`, upload via `actions/upload-artifact`.
  - The version guard becomes a separate first job: extract the three manifest versions with
    line-anchored `grep`/`sed` (Cargo.toml `^version`, plugin.json + marketplace.json `"version"`),
    compare each to `${GITHUB_REF_NAME#v}`, fail naming the divergent file. Build jobs `needs` it.
  - `release` job: `needs: build`, downloads all artifacts, generates `SHA256SUMS` over the four
    binaries (`sha256sum gatekeeper-* > SHA256SUMS`), publishes all five files with
    `softprops/action-gh-release@v2`.
- Add `/bin/` to `.gitignore` (comment: installer/provisioner output, never committed).
- Check: `yamllint`-level sanity not in the toolchain, so verify with a YAML parse
  (`python3 -c "import yaml,sys; yaml.safe_load(open('.github/workflows/release.yml'))"`)
  plus a grep asserting all four target triples and `SHA256SUMS` appear.

## Task 3 — Hook resolver: prebuilt locations first

- `hooks/security-scan.sh`: after the `$GATEKEEPER_BIN` branch, add `elif [[ -x "$ROOT/bin/gatekeeper" ]]`
  then `elif [[ -n "${CLAUDE_PLUGIN_DATA:-}" && -x "$CLAUDE_PLUGIN_DATA/bin/gatekeeper" ]]`; keep
  the existing repo-build → PATH → deny tail byte-for-byte.
- `hooks/skill-activation.sh`: same two branches in the same position; keep its PATH-first tail and
  advisory exit 0.
- Update each script's header comment to name the full resolution order.
- Check (mirrors spec AC-3): in a temp `$ROOT` containing both a `bin/gatekeeper` wrapper and a
  `gatekeeper/target/release/gatekeeper` wrapper (each a 3-line script echoing a distinct tag),
  piping a scan event through `security-scan.sh` surfaces the `bin/` tag; deleting both and clearing
  PATH yields the deny JSON; `skill-activation.sh` under the same empty setup prints the advisory
  and exits 0.

## Task 4 — `scripts/fetch-gatekeeper.sh` (shared fetch + verify)

New script, contract from spec §3:

- Args: exactly one, `<dest-dir>`; usage error otherwise.
- Resolve repo root relative to the script (`$(dirname BASH_SOURCE)/..`) for the manifest read.
- Platform switch on `uname -s`/`uname -m` (accept `arm64` and `aarch64` spellings for both OSes);
  unsupported pairs exit 1 with the build-from-source message.
- Version: `$TOPOLOGY_VERSION` if set, else line-anchored extraction of `"version"` from
  `.claude-plugin/plugin.json`.
- Base URL: `${TOPOLOGY_RELEASE_BASE_URL:-https://github.com/osxsystem/topology/releases/download}`;
  asset URL `<base>/v<version>/gatekeeper-<triple>`, checksum URL `<base>/v<version>/SHA256SUMS`.
- Work in `mktemp -d` with an EXIT trap removing it: `curl -fsSL --max-time 60` both files; verify
  with `sha256sum -c` or `shasum -a 256 -c` (grep the SUMS file down to the one asset line first);
  `chmod +x`; smoke `./gatekeeper-<triple> --version`; `mkdir -p <dest-dir>` and atomic
  `mv` to `<dest-dir>/gatekeeper`; print the final absolute path as the only stdout line.
- Check (spec AC-2): build a `file://` fixture from the repo's own release binary
  (`cargo build --release`, copy as `gatekeeper-<host-triple>`, real `SHA256SUMS`); fetch into a
  temp dest succeeds and the result runs `--version`; corrupt one hex digit in the fixture SUMS →
  exit non-zero and dest stays empty.

## Task 5 — `hooks/ensure-gatekeeper.sh` + SessionStart wiring

- New hook script: compute `ROOT` like the other hooks; probe, in order, `$GATEKEEPER_BIN`,
  `$ROOT/bin/gatekeeper`, `$CLAUDE_PLUGIN_DATA/bin/gatekeeper`, both repo target builds, and
  `command -v gatekeeper`; if any hits, exit 0 with no output.
- Otherwise run `"$ROOT/scripts/fetch-gatekeeper.sh" "${CLAUDE_PLUGIN_DATA:-$ROOT}/bin"`; on
  success print `Topology: gatekeeper <version> provisioned at <path>`; on failure print the
  advisory naming both remedies (`scripts/install.sh`, `cargo build --release` in `gatekeeper/`)
  and exit 0. The script never exits non-zero.
- `hooks/hooks.json`: add the `SessionStart` block running it via `${CLAUDE_PLUGIN_ROOT}` with
  `"timeout": 90`.
- Check (spec AC-4): with a stub binary on the probe chain the script emits nothing and a
  `TOPOLOGY_RELEASE_BASE_URL` pointing at a nonexistent path proves no fetch was attempted; with an
  empty chain + the task-4 fixture it provisions into a temp `CLAUDE_PLUGIN_DATA/bin` and prints
  the path; with an empty chain + unreachable URL it prints the advisory and exits 0.

## Task 6 — Installer rework (`scripts/install.sh`)

- Root location: if `${BASH_SOURCE[0]:-}` names an existing file, root = its `../`; else
  `ROOT="${TOPOLOGY_HOME:-$HOME/.topology}"` and either `git clone` (absent) or
  `git -C "$ROOT" pull --ff-only` (present clone); refuse a non-git existing dir with a clear error.
- `MANIFEST=()` array + `note() { MANIFEST+=("$1"); }` helper; call it at every file mutation:
  binary, symlink, pre-commit copy, chmod targets are listed once as "made executable".
- Binary step: unless `--build-from-source` was passed, try
  `scripts/fetch-gatekeeper.sh "$ROOT/bin"`; on failure (or the flag) fall back to the existing
  cargo build, then `mkdir -p bin && cp target-binary bin/gatekeeper`; if neither path can produce
  a binary, exit 1 naming both remedies. All later references (`$BIN`, verify lines, PATH hint) use
  `$ROOT/bin/gatekeeper`.
- Keep symlink/chmod/pre-commit/hook-config/adapt/plugin sections, updating the plugin note: the
  binary prerequisite paragraph is replaced by the self-provisioning story (ADR-0011 §3).
- Final section prints: `gatekeeper --version` output and path, `Files created or modified:` with
  one absolute path per line from `MANIFEST`, the PATH suggestion, then runs
  `"$ROOT/bin/gatekeeper" doctor` as the closing health check.
- Check (spec AC-5): from an empty temp dir, `TOPOLOGY_HOME=$tmp/topology TOPOLOGY_RELEASE_BASE_URL=<fixture> bash < scripts/install.sh`
  clones, installs the fixture binary, and the output's manifest block lists the binary, the
  symlink, and the pre-commit hook as absolute paths; a second run inside the checkout reuses it
  (no clone) and still ends with the manifest.

## Task 7 — Docs

- `README.md`: replace the install section lead with the curl one-liner; plugin flow follows with
  "the binary self-provisions on first session"; keep the build-from-source path as the fallback
  paragraph.
- `docs/USER-GUIDE.md`: rewrite the install section around the two paths; add a "binary
  resolution order" list (override → `bin/` → plugin data → repo builds → PATH, with the per-hook
  fail policies); document `$TOPOLOGY_HOME`, `$TOPOLOGY_RELEASE_BASE_URL`, `$TOPOLOGY_VERSION`
  alongside the existing `$GATEKEEPER_BIN`/`$TOPOLOGY_ROOT` entries; one paragraph stating plugin
  installs register `skills/` natively via auto-discovery (and why there is no manifest field).
- `docs/ROADMAP.md`: one-line addendum under Phase 6 pointing at ADR-0011 and the verify doc.
- Check: `gatekeeper check docs` green; `lychee`-style relative links resolve (just links is
  network-gated, so verify the new relative paths by `ls`).

## Task 8 — Verify + review artifacts, finish

- Run the spec's seven acceptance criteria end-to-end; record commands + observed output in
  `docs/verify/2026-06-10-one-command-install.md` (reproduce-then-resolve framing: the "before"
  is the cargo-required, manifest-less installer on `main`).
- Fresh-context critic reviews the branch diff against the two rubric dimensions; artifact at
  `docs/reviews/2026-06-10-one-command-install.md`; `gatekeeper check review --feature
  one-command-install --base main` passes on clean HEAD.
- `gatekeeper check finish -- just check` green; push; PR titled
  `feat: one-command install — prebuilt binaries, self-provisioning plugin, installer manifest`.
