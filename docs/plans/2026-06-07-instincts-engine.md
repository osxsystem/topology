# Plan: Instincts engine (Phase 2)

- **Date:** 2026-06-07
- **Feature slug:** instincts-engine
- **Design:** docs/specs/2026-06-07-instincts-engine.md (Status: draft)
- **Baseline:** tests green at `04b665a4e584d76b5adb8b094e3d4ea4ef442f92` on `feat/instincts-engine`
  — the suite is **101 passed across 3 suites** (gatekeeper bin unittests **57 passed, 2 ignored**;
  `cli_review` **1**; `cli_scan` **43**). Confirm before starting: `cd gatekeeper && cargo test` → all
  green.
- **Design ratification at plan time (maintainer to confirm):** decisions **D** (priority = ordering +
  word-budget truncation), **E** (loader fail-mode matrix), **G** (render/adapt split) are research
  leans, adopted here. If any is vetoed, stop before the affected task.

## Conventions for every task

- **No new dependencies.** Unlike the security-scanning plan, this feature adds **zero** crates and does
  **not** touch `gatekeeper/Cargo.toml` / `Cargo.lock` or any ADR. The frontmatter parser is hand-rolled
  on `std` (the four existing crates `regex`/`serde`/`serde_json`/`toml` are not needed for instincts).
- **Tests are the per-task gate.** Each task runs `cd gatekeeper && cargo test <filter>` and must end
  green (exit 0). `cargo build`/`cargo test` permit warnings; **`clippy -D warnings` and
  `cargo fmt --check` are enforced once, in the final verify task (Task 8)** — as the security plan did.
  Until `activate` is wired (Task 5), `instinct.rs` items are unused and the build emits `dead_code`
  warnings; that is expected and clears once everything is wired.
- **Test style:** the existing std style — `std::process::Command` + `env!("CARGO_BIN_EXE_gatekeeper")`
  for CLI/integration tests (as in `tests/cli_scan.rs`); plain `#[cfg(test)] mod` for unit tests. No
  `assert_cmd`/`predicates`.
- **Self-protection friction (expected, not a blocker).** Adding `gatekeeper/src/instinct.rs` and editing
  `gatekeeper/src/main.rs` trips `protected_paths` + the `tamper-security-wiring` regex, so the PreToolUse
  hook will `ask` and the pre-commit gate will require a human at commit time. `instincts/` is **not** a
  protected path. Land `main.rs` edits as discrete, reviewable commits.
- **Always-on identity.** Instincts carry **no scope** — there is no `applies` field and no per-file
  matching. `activate` emits the whole set; the loader does no context filtering.

## Files

- `gatekeeper/src/instinct.rs` — **new.** The whole engine: `Priority` + `Instinct` model, the hand-rolled
  frontmatter parser + validator, the directory loader (`load_instincts`) with sort + dedupe + fail-mode
  behavior, `render_preamble` / `budget_filter`, the `cmd_instinct` (`list` / `render`) handlers, the
  `activate_section` helper, and `#[cfg(test)]` unit modules.
- `gatekeeper/src/main.rs` — **modify.** Declare `mod instinct;`; add the `"instinct"` dispatch arm;
  inject `instinct::activate_section()` into `cmd_activate`; extend `print_help()` and the `//!` block.
- `gatekeeper/tests/cli_instinct.rs` — **new.** Integration tests running the compiled binary over a
  scratch framework root (mirrors `tests/cli_scan.rs`).
- `instincts/constraints-as-reasoning.md`, `instincts/evidence-over-assertion.md`,
  `instincts/gates-not-rules.md`, `instincts/weakest-enforcement-that-works.md`,
  `instincts/surgical-changes-only.md`, `instincts/three-language-lanes.md` — **new.** The 6 seed files.
- `docs/ARCHITECTURE.md` — **modify.** Update the §5 canonical instinct shape (drop `applies`); fix the
  stale `gatekeeper/src/json.rs` reference at `:195` (retired in ADR-0007 → `serde_json`).
- `docs/ROADMAP.md` — **modify.** Phase 2 → delivered; **rewrite the verify criterion** off scoped
  instincts; fix the `skills'`→`instinct's` reference at `:108`.
- `docs/verify/2026-06-07-instincts-engine.md` — **new** (Task 8). Verification evidence + the efficacy eval.

## Tasks

### Task 1: `instinct.rs` — model + frontmatter parser/validator; wire `mod instinct;`
- **File(s):** `gatekeeper/src/instinct.rs` (new), `gatekeeper/src/main.rs`
- **Change (a):** Create `gatekeeper/src/instinct.rs` with this exact content:
  ```rust
  //! Instincts engine — the weakest operator: tiny, always-on, reasoning-based guardrails.
  //!
  //! An instinct is a hyper-lean Markdown file (`instincts/<id>.md`): YAML-ish frontmatter (`id`,
  //! `priority`, optional `schema`/`source`) + a 1–2 sentence *why* body. Instincts carry NO scope —
  //! they are always-on; `activate` injects the whole set. The frontmatter is parsed by hand (std only;
  //! no YAML crate). See docs/specs/2026-06-07-instincts-engine.md.

  use std::collections::HashSet;
  use std::fs;
  use std::path::{Path, PathBuf};

  const SCHEMA_VERSION: u32 = 1;

  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  enum Priority {
      High,
      Medium,
      Low,
  }

  impl Priority {
      fn rank(self) -> u8 {
          match self {
              Priority::High => 0,
              Priority::Medium => 1,
              Priority::Low => 2,
          }
      }
      fn as_str(self) -> &'static str {
          match self {
              Priority::High => "high",
              Priority::Medium => "medium",
              Priority::Low => "low",
          }
      }
      fn parse(s: &str) -> Result<Priority, String> {
          match s {
              "high" => Ok(Priority::High),
              "medium" => Ok(Priority::Medium),
              "low" => Ok(Priority::Low),
              other => Err(format!("invalid priority '{other}' (expected high|medium|low)")),
          }
      }
  }

  #[derive(Debug, Clone)]
  pub struct Instinct {
      id: String,
      priority: Priority,
      #[allow(dead_code)] // accepted + validated; surfaced in Phase 3, not Phase 2
      schema: u32,
      #[allow(dead_code)] // accept-but-ignore provenance; read by Phase-3 promote
      source: Option<String>,
      body: String,
  }

  impl Instinct {
      /// The body collapsed to a single whitespace-normalized line (for preamble rendering).
      fn body_oneline(&self) -> String {
          self.body.split_whitespace().collect::<Vec<_>>().join(" ")
      }
      /// Word count of the body — the unit `--budget` truncates on.
      fn word_count(&self) -> usize {
          self.body.split_whitespace().count()
      }
  }

  /// Strip one layer of matching surrounding quotes, if present.
  fn unquote(v: &str) -> &str {
      let v = v.trim();
      if (v.starts_with('"') && v.ends_with('"') && v.len() >= 2)
          || (v.starts_with('\'') && v.ends_with('\'') && v.len() >= 2)
      {
          &v[1..v.len() - 1]
      } else {
          v
      }
  }

  /// kebab-case, 1..=64 chars, no leading/trailing/double hyphen, no reserved word.
  fn validate_id(id: &str) -> Result<(), String> {
      if id.is_empty() || id.len() > 64 {
          return Err(format!("id '{id}': must be 1..=64 chars"));
      }
      let lc = id.to_lowercase();
      if lc.contains("claude") || lc.contains("anthropic") {
          return Err(format!("id '{id}': must not contain a reserved word (claude/anthropic)"));
      }
      let charset_ok = id
          .chars()
          .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
      if !charset_ok || id.starts_with('-') || id.ends_with('-') || id.contains("--") {
          return Err(format!(
              "id '{id}': must be kebab-case [a-z0-9-] with no leading/trailing/double hyphen"
          ));
      }
      Ok(())
  }

  /// Parse one instinct file: `---`-fenced frontmatter then a Markdown body. Any defect is an Err
  /// (the caller maps it to skip+warn or exit 2 per the fail-mode matrix).
  fn parse_instinct(raw: &str) -> Result<Instinct, String> {
      let text = raw.replace("\r\n", "\n");
      let after_open = text
          .strip_prefix("---\n")
          .ok_or("missing opening '---' frontmatter fence")?;

      // Walk frontmatter lines until a line that is exactly "---".
      let mut id: Option<String> = None;
      let mut priority = Priority::Medium;
      let mut schema = SCHEMA_VERSION;
      let mut source: Option<String> = None;
      let mut body_offset: Option<usize> = None;
      let mut offset = 0usize;
      for line in after_open.split_inclusive('\n') {
          let content = line.trim_end_matches('\n');
          if content.trim() == "---" {
              body_offset = Some(offset + line.len());
              break;
          }
          offset += line.len();
          let trimmed = content.trim();
          if trimmed.is_empty() || trimmed.starts_with('#') {
              continue;
          }
          let (key, value) = trimmed
              .split_once(':')
              .ok_or_else(|| format!("frontmatter line is not 'key: value': {trimmed}"))?;
          let key = key.trim();
          let value = unquote(value);
          match key {
              "id" => id = Some(value.to_string()),
              "priority" => priority = Priority::parse(value)?,
              "schema" => {
                  schema = value
                      .parse::<u32>()
                      .map_err(|_| format!("schema '{value}': expected a non-negative integer"))?;
              }
              "source" => source = Some(value.to_string()),
              other => return Err(format!("unknown frontmatter field '{other}'")),
          }
      }

      let body_offset = body_offset.ok_or("missing closing '---' frontmatter fence")?;
      let id = id.ok_or("missing required field 'id'")?;
      validate_id(&id)?;
      if schema != SCHEMA_VERSION {
          return Err(format!(
              "unsupported schema {schema} (expected {SCHEMA_VERSION})"
          ));
      }
      let body = after_open[body_offset..].trim().to_string();
      if body.is_empty() {
          return Err(format!("instinct '{id}': body (the why) is empty"));
      }
      Ok(Instinct {
          id,
          priority,
          schema,
          source,
          body,
      })
  }

  #[cfg(test)]
  mod parse_tests {
      use super::*;

      const VALID: &str = "---\nid: evidence-over-assertion\npriority: high\nsource: doc:ROADMAP\n---\n\"Done\" means a re-runnable command and its output, never a feeling.\n";

      #[test]
      fn valid_parses() {
          let i = parse_instinct(VALID).unwrap();
          assert_eq!(i.id, "evidence-over-assertion");
          assert_eq!(i.priority, Priority::High);
          assert_eq!(i.schema, 1);
          assert_eq!(i.source.as_deref(), Some("doc:ROADMAP"));
          assert!(i.body.starts_with("\"Done\""));
      }
      #[test]
      fn priority_defaults_to_medium() {
          let src = "---\nid: surgical-changes-only\n---\nChange only what the task needs.\n";
          assert_eq!(parse_instinct(src).unwrap().priority, Priority::Medium);
      }
      #[test]
      fn missing_id_rejected() {
          assert!(parse_instinct("---\npriority: high\n---\nbody\n").is_err());
      }
      #[test]
      fn unknown_field_rejected() {
          let bad = "---\nid: x\napplies: always\n---\nbody\n";
          let err = parse_instinct(bad).unwrap_err();
          assert!(err.contains("unknown frontmatter field 'applies'"), "{err}");
      }
      #[test]
      fn bad_priority_rejected() {
          assert!(parse_instinct("---\nid: x\npriority: urgent\n---\nbody\n").is_err());
      }
      #[test]
      fn bad_schema_rejected() {
          assert!(parse_instinct("---\nid: x\nschema: 9\n---\nbody\n").is_err());
      }
      #[test]
      fn reserved_word_in_id_rejected() {
          assert!(parse_instinct("---\nid: ask-claude\n---\nbody\n").is_err());
      }
      #[test]
      fn non_kebab_id_rejected() {
          assert!(parse_instinct("---\nid: Bad_Id\n---\nbody\n").is_err());
          assert!(parse_instinct("---\nid: -lead\n---\nbody\n").is_err());
          assert!(parse_instinct("---\nid: dou--ble\n---\nbody\n").is_err());
      }
      #[test]
      fn empty_body_rejected() {
          assert!(parse_instinct("---\nid: x\n---\n\n").is_err());
      }
      #[test]
      fn missing_closing_fence_rejected() {
          assert!(parse_instinct("---\nid: x\nbody with no fence\n").is_err());
      }
  }
  ```
- **Change (b):** In `gatekeeper/src/main.rs`, add the module declaration directly below `mod scan;`
  (line 26), so the block reads:
  ```rust
  mod instinct;
  mod review;
  mod scan;
  ```
  (Cargo orders modules by source position, not alphabetically; place `mod instinct;` so the block is
  alphabetical: `instinct`, `review`, `scan`.)
- **Test:** `cd gatekeeper && cargo test instinct::parse_tests` → **11 passed** (exit 0). `cargo test`
  may print `dead_code` warnings for not-yet-called `instinct.rs` items — expected until Task 5.
- **Commit:** `feat(gatekeeper): instinct model + fail-loud frontmatter parser`

### Task 2: `instinct.rs` — directory loader with sort + dedupe + fail-mode behavior
- **File(s):** `gatekeeper/src/instinct.rs` (append above `#[cfg(test)] mod parse_tests`)
- **Change:** Append:
  ```rust
  /// Load every `*.md` instinct under `dir`, sorted by (priority high→low, then id).
  ///
  /// Fail-mode (design decision E): a missing dir yields an empty set in both modes. On a per-file
  /// parse error or a duplicate id, `strict` mode returns Err (the `list`/`render` path → exit 2);
  /// non-strict mode skips the offender, pushes a warning, and continues (the `activate` path → exit 0,
  /// never breaking the turn).
  fn load_instincts(
      dir: &Path,
      strict: bool,
      warnings: &mut Vec<String>,
  ) -> Result<Vec<Instinct>, String> {
      let entries = match fs::read_dir(dir) {
          Ok(e) => e,
          Err(ref e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
          Err(e) => {
              let msg = format!("cannot read {}: {e}", dir.display());
              return if strict {
                  Err(msg)
              } else {
                  warnings.push(msg);
                  Ok(Vec::new())
              };
          }
      };

      let mut paths: Vec<PathBuf> = entries
          .filter_map(|e| e.ok().map(|e| e.path()))
          .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("md"))
          .collect();
      paths.sort(); // deterministic processing order

      let mut out = Vec::new();
      let mut seen = HashSet::new();
      for p in paths {
          let raw = match fs::read_to_string(&p) {
              Ok(r) => r,
              Err(e) => {
                  let msg = format!("{}: read error: {e}", p.display());
                  if strict {
                      return Err(msg);
                  }
                  warnings.push(msg);
                  continue;
              }
          };
          let inst = match parse_instinct(&raw) {
              Ok(i) => i,
              Err(e) => {
                  let msg = format!("{}: {e}", p.display());
                  if strict {
                      return Err(msg);
                  }
                  warnings.push(msg);
                  continue;
              }
          };
          if !seen.insert(inst.id.clone()) {
              let msg = format!("duplicate instinct id '{}' ({})", inst.id, p.display());
              if strict {
                  return Err(msg);
              }
              warnings.push(msg);
              continue;
          }
          out.push(inst);
      }

      out.sort_by(|a, b| {
          a.priority
              .rank()
              .cmp(&b.priority.rank())
              .then_with(|| a.id.cmp(&b.id))
      });
      Ok(out)
  }

  #[cfg(test)]
  mod load_tests {
      use super::*;

      fn scratch(tag: &str) -> PathBuf {
          let dir = std::env::temp_dir().join(format!("topo_instload_{tag}_{}", std::process::id()));
          let _ = fs::remove_dir_all(&dir);
          fs::create_dir_all(&dir).unwrap();
          dir
      }
      fn write(dir: &Path, name: &str, body: &str) {
          fs::write(dir.join(name), body).unwrap();
      }

      #[test]
      fn missing_dir_is_empty_both_modes() {
          let dir = std::env::temp_dir().join("topo_instload_absent_does_not_exist");
          let mut w = Vec::new();
          assert!(load_instincts(&dir, true, &mut w).unwrap().is_empty());
          assert!(load_instincts(&dir, false, &mut w).unwrap().is_empty());
          assert!(w.is_empty());
      }
      #[test]
      fn sorts_priority_then_id() {
          let dir = scratch("sort");
          write(&dir, "b.md", "---\nid: b-medium\npriority: medium\n---\nwhy b\n");
          write(&dir, "a.md", "---\nid: a-high\npriority: high\n---\nwhy a\n");
          write(&dir, "c.md", "---\nid: c-high\npriority: high\n---\nwhy c\n");
          let mut w = Vec::new();
          let list = load_instincts(&dir, true, &mut w).unwrap();
          let ids: Vec<&str> = list.iter().map(|i| i.id.as_str()).collect();
          assert_eq!(ids, vec!["a-high", "c-high", "b-medium"], "high (by id) then medium");
          let _ = fs::remove_dir_all(&dir);
      }
      #[test]
      fn malformed_file_strict_errs_soft_warns() {
          let dir = scratch("malformed");
          write(&dir, "ok.md", "---\nid: ok\n---\nfine\n");
          write(&dir, "bad.md", "---\nid: bad\nbogus: 1\n---\nnope\n");
          let mut w = Vec::new();
          assert!(load_instincts(&dir, true, &mut w).is_err());
          let mut w2 = Vec::new();
          let soft = load_instincts(&dir, false, &mut w2).unwrap();
          assert_eq!(soft.len(), 1, "soft mode keeps the good file");
          assert_eq!(w2.len(), 1, "soft mode warns about the bad one");
          let _ = fs::remove_dir_all(&dir);
      }
      #[test]
      fn duplicate_id_strict_errs() {
          let dir = scratch("dupe");
          write(&dir, "one.md", "---\nid: dup\n---\nfirst\n");
          write(&dir, "two.md", "---\nid: dup\n---\nsecond\n");
          let mut w = Vec::new();
          let err = load_instincts(&dir, true, &mut w).unwrap_err();
          assert!(err.contains("duplicate instinct id 'dup'"), "{err}");
          let _ = fs::remove_dir_all(&dir);
      }
  }
  ```
- **Test:** `cd gatekeeper && cargo test instinct::load_tests` → **4 passed** (exit 0).
- **Commit:** `feat(gatekeeper): instinct directory loader (sorted, deduped, fail-mode matrix)`

### Task 3: `instinct.rs` — preamble rendering + word-budget truncation + `activate_section`
- **File(s):** `gatekeeper/src/instinct.rs` (append above `#[cfg(test)] mod parse_tests`)
- **Change:** Append:
  ```rust
  /// The fixed header that demarcates the instincts section from routed skills in the preamble.
  const PREAMBLE_HEADER: &str =
      "Always-on instincts — how to reason here (framing you may reason past only with cause):";

  /// Render the preamble section: header + one `- [id] why` line per instinct. Ends with a newline.
  /// Note the deliberate absence of skills' `[enforcement]` tag — instincts are soft framing.
  fn render_preamble(items: &[&Instinct]) -> String {
      let mut s = String::from(PREAMBLE_HEADER);
      s.push('\n');
      for i in items {
          s.push_str(&format!("  - [{}] {}\n", i.id, i.body_oneline()));
      }
      s
  }

  /// Keep the highest-priority prefix whose total body word count fits `budget` (design decision D:
  /// drop lowest-priority-first, whole instincts only — never split a body). `None` keeps all.
  fn budget_filter<'a>(items: &'a [Instinct], budget: Option<usize>) -> Vec<&'a Instinct> {
      match budget {
          None => items.iter().collect(),
          Some(max) => {
              let mut used = 0usize;
              let mut kept = Vec::new();
              for i in items {
                  let w = i.word_count();
                  if used + w > max {
                      break;
                  }
                  used += w;
                  kept.push(i);
              }
              kept
          }
      }
  }

  /// Soft load + render for `cmd_activate`. Warns to stderr; returns "" when there are no instincts
  /// (so a missing `instincts/` dir adds nothing and never breaks the turn).
  pub fn activate_section(root: &Path) -> String {
      let mut warnings = Vec::new();
      let instincts =
          load_instincts(&root.join("instincts"), false, &mut warnings).unwrap_or_default();
      for w in &warnings {
          eprintln!("gatekeeper: instinct {w}");
      }
      if instincts.is_empty() {
          String::new()
      } else {
          let refs: Vec<&Instinct> = instincts.iter().collect();
          render_preamble(&refs)
      }
  }

  #[cfg(test)]
  mod render_tests {
      use super::*;

      fn inst(id: &str, prio: Priority, body: &str) -> Instinct {
          Instinct {
              id: id.to_string(),
              priority: prio,
              schema: 1,
              source: None,
              body: body.to_string(),
          }
      }

      #[test]
      fn preamble_has_header_and_id_lines_no_enforcement_tag() {
          let a = inst("evidence-over-assertion", Priority::High, "Done means a re-runnable command.");
          let refs = vec![&a];
          let out = render_preamble(&refs);
          assert!(out.starts_with(PREAMBLE_HEADER));
          assert!(out.contains("  - [evidence-over-assertion] Done means a re-runnable command."));
          assert!(!out.contains("[suggest]") && !out.contains("[block]"), "no enforcement tag");
      }
      #[test]
      fn budget_drops_lowest_priority_whole() {
          // high=2 words, medium=5 words; sorted high first.
          let hi = inst("a-hi", Priority::High, "one two");
          let mid = inst("b-mid", Priority::Medium, "one two three four five");
          let all = vec![hi, mid];
          // budget 4: only the 2-word high fits; the 5-word medium is dropped whole.
          let kept = budget_filter(&all, Some(4));
          assert_eq!(kept.len(), 1);
          assert_eq!(kept[0].id, "a-hi");
          // budget 7: both fit (2 + 5).
          assert_eq!(budget_filter(&all, Some(7)).len(), 2);
          // no budget: all.
          assert_eq!(budget_filter(&all, None).len(), 2);
      }
      #[test]
      fn body_oneline_collapses_whitespace() {
          let i = inst("x", Priority::Low, "line one\n  line   two\n");
          assert_eq!(i.body_oneline(), "line one line two");
          assert_eq!(i.word_count(), 4);
      }
  }
  ```
- **Test:** `cd gatekeeper && cargo test instinct::render_tests` → **3 passed** (exit 0).
- **Commit:** `feat(gatekeeper): instinct preamble rendering + word-budget truncation`

### Task 4: `cmd_instinct` (`list` / `render`) + CLI wiring + integration harness
- **File(s):** `gatekeeper/src/instinct.rs`, `gatekeeper/src/main.rs`, `gatekeeper/tests/cli_instinct.rs` (new)
- **Change (a):** Append the dispatcher + handlers to `instinct.rs` (above `#[cfg(test)] mod parse_tests`):
  ```rust
  /// Entry point for `gatekeeper instinct ...`. Returns the process exit code (0 / 2).
  pub fn cmd_instinct(args: &[String], root: &Path) -> i32 {
      match args.first().map(String::as_str) {
          Some("list") => cmd_list_instincts(root),
          Some("render") => cmd_render(&args[1..], root),
          _ => {
              eprintln!("gatekeeper instinct: expected `list` or `render [--harness <h>] [--budget <n>]`");
              2
          }
      }
  }

  fn cmd_list_instincts(root: &Path) -> i32 {
      let mut warnings = Vec::new();
      match load_instincts(&root.join("instincts"), true, &mut warnings) {
          Ok(list) => {
              for i in &list {
                  println!("{}\t{}", i.id, i.priority.as_str());
              }
              0
          }
          Err(e) => {
              eprintln!("gatekeeper instinct list: {e}");
              2
          }
      }
  }

  fn cmd_render(args: &[String], root: &Path) -> i32 {
      let mut harness = "claude".to_string();
      let mut budget: Option<usize> = None;
      let mut i = 0;
      while i < args.len() {
          match args[i].as_str() {
              "--harness" => match args.get(i + 1) {
                  Some(h) => {
                      harness = h.clone();
                      i += 2;
                  }
                  None => {
                      eprintln!("gatekeeper instinct render: --harness needs a value");
                      return 2;
                  }
              },
              "--budget" => match args.get(i + 1).and_then(|n| n.parse::<usize>().ok()) {
                  Some(n) => {
                      budget = Some(n);
                      i += 2;
                  }
                  None => {
                      eprintln!("gatekeeper instinct render: --budget needs a non-negative integer");
                      return 2;
                  }
              },
              other => {
                  eprintln!("gatekeeper instinct render: unknown flag '{other}'");
                  return 2;
              }
          }
      }
      if harness != "claude" {
          eprintln!("gatekeeper instinct render: harness '{harness}' not supported in Phase 2 (only 'claude')");
          return 2;
      }
      let mut warnings = Vec::new();
      let list = match load_instincts(&root.join("instincts"), true, &mut warnings) {
          Ok(l) => l,
          Err(e) => {
              eprintln!("gatekeeper instinct render: {e}");
              return 2;
          }
      };
      let kept = budget_filter(&list, budget);
      print!("{}", render_preamble(&kept));
      0
  }
  ```
- **Change (b):** In `gatekeeper/src/main.rs`, add the dispatch arm directly below the `scan` arm
  (line 43):
  ```rust
          Some("instinct") => instinct::cmd_instinct(&args[1..], &framework_root()),
  ```
  Extend `print_help()` — replace the `gatekeeper scan --staged | --check-path <path>\n"` line (line 69)
  with:
  ```rust
           gatekeeper scan --staged | --check-path <path>\n  \
           gatekeeper instinct list\n  \
           gatekeeper instinct render [--harness <h>] [--budget <n>]\n"
  ```
  Extend the `//!` block — replace the `gatekeeper scan --check-path <path>` line (line 14) with:
  ```rust
  //!   gatekeeper scan --check-path <path>     Exit 1 iff <path> is a protected safety file.
  //!   gatekeeper instinct list                List always-on instincts (id + priority).
  //!   gatekeeper instinct render [--harness H] [--budget N]   Render the always-on preamble subset.
  ```
- **Change (c):** Create `gatekeeper/tests/cli_instinct.rs` with the harness + the `list`/`render`/budget/
  unsupported-harness cases:
  ```rust
  use std::fs;
  use std::io::Write;
  use std::path::{Path, PathBuf};
  use std::process::{Command, Stdio};

  /// A minimal framework root: a `skills/` marker (so `framework_root()` resolves here) and an
  /// `instincts/` dir with one high + one medium seed.
  fn scratch_root(tag: &str) -> PathBuf {
      let root = std::env::temp_dir().join(format!("topo_inst_{tag}_{}", std::process::id()));
      let _ = fs::remove_dir_all(&root);
      fs::create_dir_all(root.join("skills")).unwrap();
      fs::create_dir_all(root.join("instincts")).unwrap();
      fs::write(
          root.join("instincts").join("evidence-over-assertion.md"),
          "---\nid: evidence-over-assertion\npriority: high\nsource: doc:ROADMAP\n---\nDone means a re-runnable command and its output, never a feeling.\n",
      )
      .unwrap();
      fs::write(
          root.join("instincts").join("surgical-changes-only.md"),
          "---\nid: surgical-changes-only\npriority: medium\nsource: doc:EXTENDING\n---\nChange only what the task needs; no drive-by refactors.\n",
      )
      .unwrap();
      root
  }

  /// Run `gatekeeper <args>` from `cwd`, feeding `stdin`. Returns (exit code, stdout).
  fn run(cwd: &Path, args: &[&str], stdin: &[u8]) -> (i32, String) {
      let mut child = Command::new(env!("CARGO_BIN_EXE_gatekeeper"))
          .current_dir(cwd)
          .args(args)
          .stdin(Stdio::piped())
          .stdout(Stdio::piped())
          .stderr(Stdio::null())
          .spawn()
          .unwrap();
      child.stdin.take().unwrap().write_all(stdin).unwrap();
      let out = child.wait_with_output().unwrap();
      (
          out.status.code().unwrap_or(-1),
          String::from_utf8_lossy(&out.stdout).into_owned(),
      )
  }

  #[test]
  fn list_enumerates_sorted_with_priority() {
      let root = scratch_root("list");
      let (code, out) = run(&root, &["instinct", "list"], b"");
      assert_eq!(code, 0);
      let lines: Vec<&str> = out.lines().collect();
      assert_eq!(lines, vec![
          "evidence-over-assertion\thigh",
          "surgical-changes-only\tmedium",
      ], "high sorts before medium");
      let _ = fs::remove_dir_all(&root);
  }

  #[test]
  fn render_claude_emits_header_and_id_lines() {
      let root = scratch_root("render");
      let (code, out) = run(&root, &["instinct", "render", "--harness", "claude"], b"");
      assert_eq!(code, 0);
      assert!(out.starts_with("Always-on instincts — how to reason here"));
      assert!(out.contains("  - [evidence-over-assertion] Done means a re-runnable command"));
      assert!(out.contains("  - [surgical-changes-only] Change only what the task needs"));
      let _ = fs::remove_dir_all(&root);
  }

  #[test]
  fn render_unsupported_harness_exits_2() {
      let root = scratch_root("harness");
      let (code, out) = run(&root, &["instinct", "render", "--harness", "cursor"], b"");
      assert_eq!(code, 2, "non-claude harness is a usage error in Phase 2");
      assert!(out.is_empty());
      let _ = fs::remove_dir_all(&root);
  }

  #[test]
  fn render_budget_drops_lowest_priority_whole() {
      let root = scratch_root("budget");
      // The high body is 11 words; budget 11 keeps it and drops the medium whole.
      let (code, out) = run(&root, &["instinct", "render", "--budget", "11"], b"");
      assert_eq!(code, 0);
      assert!(out.contains("evidence-over-assertion"));
      assert!(!out.contains("surgical-changes-only"), "medium dropped under tight budget");
      let _ = fs::remove_dir_all(&root);
  }

  #[test]
  fn list_on_malformed_file_exits_2() {
      let root = scratch_root("badlist");
      fs::write(
          root.join("instincts").join("bad.md"),
          "---\nid: bad\napplies: always\n---\nscoped is no longer a field\n",
      )
      .unwrap();
      let (code, _) = run(&root, &["instinct", "list"], b"");
      assert_eq!(code, 2, "unknown frontmatter field fails loud in list");
      let _ = fs::remove_dir_all(&root);
  }
  ```
- **Test:** `cd gatekeeper && cargo test --test cli_instinct` → **5 passed** (exit 0). Manual from repo
  root (after Task 6): `gatekeeper instinct list` prints 6 rows.
- **Commit:** `feat(gatekeeper): instinct list/render subcommands wired into the CLI`

### Task 5: Inject the instincts section into `cmd_activate`
- **File(s):** `gatekeeper/src/main.rs`, `gatekeeper/tests/cli_instinct.rs`
- **Change (a):** In `cmd_activate` (`main.rs`), insert the instincts section between the routed-skills
  block and the gate-warning line. Replace this (lines 157–158):
  ```rust
      }
      println!("You may not write production code before the design and plan gates pass.");
  ```
  with:
  ```rust
      }
      print!("{}", instinct::activate_section(&framework_root()));
      println!("You may not write production code before the design and plan gates pass.");
  ```
  (`activate_section` returns "" when there are no instincts, so the output is unchanged in that case —
  the turn is never broken.)
- **Change (b):** Add to `cli_instinct.rs`:
  ```rust
  #[test]
  fn activate_injects_instincts_between_skills_and_gate_warning() {
      let root = scratch_root("activate");
      // No skill-rules.json in the scratch root → no routed skills, but instincts still inject.
      let (code, out) = run(&root, &["activate"], b"please refactor the parser\n");
      assert_eq!(code, 0);
      let header = out.find("Always-on instincts —").expect("instincts header present");
      let gate = out.find("You may not write production code").expect("gate warning present");
      assert!(header < gate, "instincts must appear before the gate-warning line");
      assert!(out.contains("  - [evidence-over-assertion]"));
      let _ = fs::remove_dir_all(&root);
  }

  #[test]
  fn activate_with_no_instincts_dir_does_not_break_turn() {
      let root = std::env::temp_dir().join(format!("topo_inst_noinst_{}", std::process::id()));
      let _ = fs::remove_dir_all(&root);
      fs::create_dir_all(root.join("skills")).unwrap(); // marker, but NO instincts/ dir
      let (code, out) = run(&root, &["activate"], b"hello\n");
      assert_eq!(code, 0, "missing instincts/ dir must not break the turn");
      assert!(!out.contains("Always-on instincts —"), "no section when there are no instincts");
      assert!(out.contains("You may not write production code"));
      let _ = fs::remove_dir_all(&root);
  }

  #[test]
  fn activate_skips_malformed_file_and_still_exits_0() {
      let root = scratch_root("activate_soft");
      fs::write(
          root.join("instincts").join("broken.md"),
          "no frontmatter fence at all\n",
      )
      .unwrap();
      let (code, out) = run(&root, &["activate"], b"hi\n");
      assert_eq!(code, 0, "a malformed instinct is skipped, not fatal, at activate time");
      assert!(out.contains("  - [evidence-over-assertion]"), "the good instincts still render");
      let _ = fs::remove_dir_all(&root);
  }
  ```
- **Test:** `cd gatekeeper && cargo test --test cli_instinct activate` → **3 passed**; full
  `cargo test --test cli_instinct` → **8 passed**. After this task `instinct.rs` is fully wired
  (no more `dead_code`).
- **Commit:** `feat(gatekeeper): inject always-on instincts into the activate preamble`

### Task 6: Author the 6 always-on seed files
- **File(s):** `instincts/*.md` (6 new files)
- **Change:** Create each file with this exact content.
  - `instincts/constraints-as-reasoning.md`:
    ```markdown
    ---
    id: constraints-as-reasoning
    priority: high
    source: doc:ADR-0004
    ---
    A guardrail phrased as the reasoning behind it generalizes to cases you did not foresee; a bare
    "NEVER X" only covers the one you did. State the why, so the rule still holds when the situation shifts.
    ```
  - `instincts/evidence-over-assertion.md`:
    ```markdown
    ---
    id: evidence-over-assertion
    priority: high
    source: doc:ROADMAP
    ---
    "Done" means a re-runnable command and its output, never a feeling. A claim you cannot replay is a
    guess wearing a verdict's clothes.
    ```
  - `instincts/gates-not-rules.md`:
    ```markdown
    ---
    id: gates-not-rules
    priority: high
    source: doc:AGENTS.md
    ---
    Phrase a commitment as trigger → check → act, not as a soft rule with an invisible opt-out. A rule you
    can silently skip is not a rule.
    ```
  - `instincts/weakest-enforcement-that-works.md`:
    ```markdown
    ---
    id: weakest-enforcement-that-works
    priority: medium
    source: doc:METHODOLOGY
    ---
    Reach for the lightest operator that still works — instinct before skill before gate before scan — and
    earn added strength only with evidence. Over-enforcing costs more than it saves.
    ```
  - `instincts/surgical-changes-only.md`:
    ```markdown
    ---
    id: surgical-changes-only
    priority: medium
    source: doc:EXTENDING
    ---
    Change what the task needs and no more; a drive-by refactor hides the real diff and widens the blast
    radius. If it is not required, leave it.
    ```
  - `instincts/three-language-lanes.md`:
    ```markdown
    ---
    id: three-language-lanes
    priority: high
    source: doc:ARCHITECTURE
    ---
    Put each change in its lane — Markdown is the source of truth, Rust enforces, Bash only glues. Never
    bridge a behavior across lanes (no logic in Bash, no enforcement in Markdown).
    ```
- **Test:** from the repo root, `cargo run --manifest-path gatekeeper/Cargo.toml -- instinct list` →
  prints exactly **6** rows, the four `high` ids first (alphabetical), then the two `medium`:
  ```
  constraints-as-reasoning	high
  evidence-over-assertion	high
  gates-not-rules	high
  three-language-lanes	high
  surgical-changes-only	medium
  weakest-enforcement-that-works	medium
  ```
  And `... -- instinct render --harness claude` emits the header + 6 `- [id] why` lines, exit 0.
- **Commit:** `feat(instincts): add the six always-on seed instincts`

### Task 7: Documentation — ARCHITECTURE §5 + ROADMAP Phase 2 + the two stale references
- **File(s):** `docs/ARCHITECTURE.md`, `docs/ROADMAP.md`
- **Change (a) — `docs/ARCHITECTURE.md`:** Update the §5 canonical instinct shape so it has **no
  `applies` field** and matches the shipped seeds:
  ```markdown
  ---
  id: evidence-over-assertion
  priority: high
  source: doc:ROADMAP
  ---
  "Done" means a re-runnable command and its output, never a feeling.
  ```
- **Change (b) — `docs/ARCHITECTURE.md:195`:** Fix the stale parser reference. Replace the clause
  calling `gatekeeper/src/json.rs` "the dependency-free JSON parser" with: routing parses
  `skill-rules.json` via `serde_json` (ADR-0007 retired the hand-rolled `json.rs`).
- **Change (c) — `docs/ROADMAP.md:108`:** Replace "Cursor … globs from each **skill's** `applies`" with
  language that reflects reality: skills route on **keywords** and have no `applies`; per-path Cursor
  scoping derives from the **skill router**, while always-on **instincts** map to Cursor's **Always** mode.
- **Change (d) — `docs/ROADMAP.md` Phase 2:** Mark Phase 2 **delivered** in the status table/section, and
  **rewrite the verify criterion**. Old (scoped) criterion: "a Kotlin instinct doesn't fire on a
  Markdown-only prompt." New criterion: "`gatekeeper activate` injects the always-on instincts under the
  `Always-on instincts —` header for any prompt; a missing `instincts/` dir yields no instincts and exit
  0; `instinct render --harness claude` reproduces the same bodies."
- **Test:** `cd gatekeeper && cargo test` → still **101 + new** green (doc-only changes do not affect
  tests). Grep gates: `rg "json\.rs" docs/ARCHITECTURE.md` → no "dependency-free JSON parser" claim
  remains; `rg "skill's \`applies\`" docs/ROADMAP.md` → no match.
- **Commit:** `docs: instinct canonical shape (no applies), Phase 2 delivered, fix stale json.rs/applies refs`

### Task 8: Verify — clippy/fmt clean, full suite, and the efficacy eval note
- **File(s):** `docs/verify/2026-06-07-instincts-engine.md` (new)
- **Change (a) — enforce quality gates** (first time this run):
  - `cd gatekeeper && cargo fmt --check` → exits 0 (no diff).
  - `cd gatekeeper && cargo clippy --all-targets -- -D warnings` → exits 0 (no warnings; `instinct.rs`
    is fully wired, so no `dead_code`).
- **Change (b) — full suite:** `cd gatekeeper && cargo test` → **green**; expect the baseline 101 plus
  the new tests (instinct unit: 11 + 4 + 3 = 18; `cli_instinct`: 8) = **127 passed across 4 suites**,
  2 ignored.
- **Change (c) — efficacy eval (design decision H)** — record in the verify note: pick one representative
  prompt (e.g. *"the scan is slow, just make it pass"*). Capture the agent's first proposed action in two
  conditions — with the `activate` preamble injected (instincts present) vs. with `instincts/` temporarily
  empty — and record whether the framing shifts the first action toward an evidence/gate step rather than
  diving into code. Score per instinct; note which (if any) earn their always-on slot, and flag
  `three-language-lanes` for the keep/prune decision. This converts the "reasoning generalizes" premise
  (ADR-0004) from asserted to evidenced.
- **Change (d) — write `docs/verify/2026-06-07-instincts-engine.md`:** record the exact commands above,
  their output (test counts, clippy/fmt clean), the eval transcript + verdict, and confirm each
  acceptance criterion from the design doc with a re-runnable command.
- **Test:** the verify note's commands are themselves the test; all exit 0 / green as stated.
- **Commit:** `test(instincts): verify note — suite green, clippy/fmt clean, efficacy eval recorded`

<!-- No "TBD", "later", "similar to", or "appropriate" placeholders. The plan gate rejects them. -->
