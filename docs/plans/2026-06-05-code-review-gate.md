# Plan: Code-review critic gate

- **Date:** 2026-06-05
- **Feature slug:** code-review-gate
- **Design:** docs/specs/2026-06-05-code-review-gate.md (Status: approved)
- **Revision:** R2 — cross-model-reviewed (GPT-5.5, read-only, 2026-06-05; verdict `fail`, 8 blockers
  + 7 concerns, all folded in). See *R1 → R2 changes* below.
- **Baseline:** main @ `7d69b1a4d3d0b90c5c15bef7432a58952b735ad3`; the existing suite is **6 tests** —
  4 in `gatekeeper/src/main.rs` (`detects_placeholders`, `ignores_placeholders_in_comments`,
  `routes_on_keyword`, `reads_description_frontmatter`) **and 2 in `gatekeeper/src/json.rs`**.
  **Toolchain note:** `cargo` was not on `PATH` in the authoring session, so the baseline green
  state could not be executed here. Before starting, confirm it in a Rust environment:
  `cd gatekeeper && cargo test` → expect `test result: ok. 6 passed` (one summary line; both modules
  compile into the single `gatekeeper` bin test binary).

## R1 → R2 changes (from the GPT-5.5 cross-review)

- **[B#1]** `review.rs` no longer imports `crate::framework_root` — `gate_review` takes `root: &Path`;
  only `main.rs` calls `framework_root()`. (Removed an unused import that fails `clippy -D warnings`.)
- **[B#2 / B#3]** The gate's clean-tree check runs `git status --porcelain --untracked-files=all`.
  Without `-uall`, git **collapses an untracked dir to a bare `docs/`**, which slips past the
  `docs/reviews/` filter — that made the fresh-pass tests go red and several exit-1 tests false-green
  (they hit the dirty check before the condition under test). *Empirically confirmed:* default →
  `?? docs/`; `--untracked-files=all` → `?? docs/reviews/<file>`.
- **[CONCERN #1]** A failed `git status` is now **fail-closed** (exit 1), not treated as a clean tree
  (was `unwrap_or_default()`).
- **[CONCERN #2]** The `fail` body grammar now rejects a stray `None.` sitting alongside `- ` items.
- **[B#4]** The comment tests are rebuilt to be true-green: the comment is injected into an
  *otherwise-valid* `fail` doc so only the comment rule can reject it, plus an explicit
  unclosed-comment test that contrasts with `strip_comments`' fail-open behavior.
- **[B#5]** Added a divergent-branch test (real 2-commit fork) that proves `BASE` must equal the
  merge-base fork point, and that a review lying with `BASE == HEAD` is rejected.
- **[B#6]** Added an abbreviated-`BASE:` test and two spec-sample-template tests (the literal pass and
  fail examples from the spec must parse to their stated verdicts — regression guard).
- **[CONCERN #3 / #5]** Added a zero-`## Blocking findings`-heading test and an
  untracked-file-outside-`docs/reviews/` dirty test.
- **[B#7]** Added `gatekeeper/tests/cli_review.rs` — invokes the **real binary** from a **nested
  subdirectory**, proving the `main.rs → framework_root() → gate_review` wiring and that git runs with
  `-C <root>`, not the process cwd (spec acceptance line 311).
- **[B#8]** The `METHODOLOGY.md` Pillar 1 edit now uses the **verbatim** (back-ticked, two-physical-line)
  source text, so the `Edit` actually matches.
- **[CONCERN #6]** Every test count is corrected (baseline 6; unit total 36; integration 1).
- **[CONCERN #7]** `ROADMAP.md` line 7 ("Only Phase 0 is delivered") is reconciled with the new
  Phase 1.5 row; `finish-branch` commits the review artifact **on the merge/PR path** (after the user
  chooses), not unconditionally before presenting options.
- **Extra (caught while re-deriving against `clippy -D warnings`):** the gate-test module imports only
  `use std::env;` (not `fs`/`Command`, which `use super::*` already brings — re-importing warns); the
  parser's pass-check is written `content.len() != 1 || content[0] != "None."` (not `!(a && b)`, which
  trips `clippy::nonminimal_bool`).

## Files

- `gatekeeper/src/review.rs` — **new.** The fail-closed artifact parser (pure) + the `review` gate
  orchestration (git state + artifact selection + validation) + two `#[cfg(test)]` modules.
- `gatekeeper/src/main.rs` — wire `mod review;`, add the `"review"` arm to `cmd_check`, add the
  `base_arg` helper, extend `print_help()` and the `//!` doc block.
- `gatekeeper/tests/cli_review.rs` — **new.** One integration test: run the compiled binary from a
  nested subdir and assert a fresh pass (proves `framework_root()` + `git -C <root>` wiring).
- `skills/code-review/SKILL.md` — **new.** The critic skill (fresh subagent, two-dimension rubric,
  artifact grammar, atomic write).
- `hooks/skill-rules.json` — route `code-review` on review keywords.
- `docs/adr/0006-code-review-gate.md` — **new.** Records the contract + the four decisions.
- `METHODOLOGY.md` — add `review` to the §4 sequence + gate table; mark the Pillar 1 critic built.
- `README.md` — add `review` to the gate table; adjust the "what's next" line.
- `docs/ROADMAP.md` — record the gate pulled forward from Phase 5; reconcile the "delivered" note.
- `skills/verify-before-done/SKILL.md` — transition to `code-review`, not `finish-branch`.
- `skills/finish-branch/SKILL.md` — enter after `code-review`; commit the review artifact on merge/PR.
- `docs/verify/2026-06-05-code-review-gate.md` — **new** (final task) — verification evidence.

## Tasks

### Task 1: Create `review.rs` with the fail-closed parser and wire the module
- **File(s):** `gatekeeper/src/review.rs` (new), `gatekeeper/src/main.rs`
- **Change:** Create `gatekeeper/src/review.rs` with this exact content:
  ```rust
  //! Code-review gate — validate an *untrusted* review artifact against git state.
  //!
  //! The artifact is untrusted input and this parser is **fail-closed**: any deviation
  //! from the strict grammar, or any git-state mismatch, is a veto (exit 1), never a
  //! pass. See docs/specs/2026-06-05-code-review-gate.md.

  use std::fs;
  use std::path::{Path, PathBuf};
  use std::process::Command;

  /// Validated machine header of a review artifact.
  #[derive(Debug, PartialEq, Eq)]
  pub struct ParsedReview {
      pub verdict_pass: bool,
      pub head: String,
      pub base: String,
  }

  /// Strip an optional leading UTF-8 BOM, normalize CRLF/CR -> LF, and trim
  /// trailing whitespace from every line. Returns the normalized text (LF-joined).
  fn normalize(raw: &str) -> String {
      let no_bom = raw.strip_prefix('\u{feff}').unwrap_or(raw);
      let unix = no_bom.replace("\r\n", "\n").replace('\r', "\n");
      unix.split('\n')
          .map(|l| l.trim_end())
          .collect::<Vec<_>>()
          .join("\n")
  }

  /// Parse `"<key>: <sha>"`, requiring a full 40- or 64-char lowercase hex sha.
  fn parse_sha_line(line: &str, key: &str) -> Result<String, String> {
      let prefix = format!("{key}: ");
      let sha = line
          .strip_prefix(&prefix)
          .ok_or_else(|| format!("line must start with '{key}: '; got '{line}'"))?;
      let len_ok = sha.len() == 40 || sha.len() == 64;
      let hex_ok = !sha.is_empty()
          && sha.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
      if len_ok && hex_ok {
          Ok(sha.to_string())
      } else {
          Err(format!("{key} must be a full 40/64-char lowercase hex sha; got '{sha}'"))
      }
  }

  /// Indices of lines that, after normalization, equal `heading` exactly.
  fn heading_indices(lines: &[&str], heading: &str) -> Vec<usize> {
      lines
          .iter()
          .enumerate()
          .filter(|(_, l)| **l == heading)
          .map(|(i, _)| i)
          .collect()
  }

  /// Lines of the section beginning at `start` (its heading line), running until
  /// the next H2 (`## `) line or EOF. The heading line itself is excluded. An H3
  /// (`### `) does NOT end the section.
  fn section_lines<'a>(lines: &[&'a str], start: usize) -> Vec<&'a str> {
      let mut out = Vec::new();
      for l in &lines[start + 1..] {
          if l.starts_with("## ") {
              break;
          }
          out.push(*l);
      }
      out
  }

  /// True if any line opens an HTML comment.
  fn contains_comment(lines: &[&str]) -> bool {
      lines.iter().any(|l| l.contains("<!--"))
  }

  /// Non-empty, trimmed content lines of a section.
  fn content_lines<'a>(lines: &[&'a str]) -> Vec<&'a str> {
      lines
          .iter()
          .map(|l| l.trim())
          .filter(|l| !l.is_empty())
          .collect()
  }

  /// Verify a `### ` subsection exists within the criteria block and has at least
  /// one non-empty, non-heading content line before the next `### ` or the end.
  fn check_subsection(criteria: &[&str], sub: &str) -> Result<(), String> {
      let start = criteria
          .iter()
          .position(|l| *l == sub)
          .ok_or_else(|| format!("missing '{sub}' under '## Criteria checked'"))?;
      let has_content = criteria[start + 1..]
          .iter()
          .take_while(|l| !l.starts_with("### "))
          .any(|l| !l.trim().is_empty() && !l.starts_with('#'));
      if has_content {
          Ok(())
      } else {
          Err(format!("'{sub}' has no content line"))
      }
  }

  /// Parse and fully validate the artifact. Ok only if the entire grammar holds.
  pub fn parse_review(raw: &str) -> Result<ParsedReview, String> {
      let normalized = normalize(raw);
      let lines: Vec<&str> = normalized.split('\n').collect();
      if lines.len() < 3 {
          return Err("artifact has fewer than 3 header lines".into());
      }

      // Line 1: verdict is the sole authority.
      let verdict_pass = match lines[0] {
          "VERDICT: pass" => true,
          "VERDICT: fail" => false,
          other => return Err(format!("line 1 must be 'VERDICT: pass|fail'; got '{other}'")),
      };

      // Lines 2-3: full-hex HEAD / BASE.
      let head = parse_sha_line(lines[1], "HEAD")?;
      let base = parse_sha_line(lines[2], "BASE")?;

      // Exactly one '## Blocking findings'.
      let blk = heading_indices(&lines, "## Blocking findings");
      if blk.len() != 1 {
          return Err(format!(
              "expected exactly one '## Blocking findings', found {}",
              blk.len()
          ));
      }
      let blocking = section_lines(&lines, blk[0]);

      // No HTML comments in the header (lines 0..3) or the blocking section.
      if contains_comment(&lines[0..3]) || contains_comment(&blocking) {
          return Err("HTML comment in a machine-parsed region (fail-closed)".into());
      }

      // Blocking content vs. verdict.
      let content = content_lines(&blocking);
      if verdict_pass {
          if content.len() != 1 || content[0] != "None." {
              return Err("pass requires the blocking section to be exactly 'None.'".into());
          }
      } else {
          let has_item = content.iter().any(|l| l.starts_with("- "));
          let has_none = content.iter().any(|l| *l == "None.");
          if !has_item || has_none {
              return Err("fail requires >=1 blocking '- ' item and no 'None.' sentinel".into());
          }
      }

      // Exactly one '## Criteria checked' with both dimensions, each non-empty.
      let crit = heading_indices(&lines, "## Criteria checked");
      if crit.len() != 1 {
          return Err(format!(
              "expected exactly one '## Criteria checked', found {}",
              crit.len()
          ));
      }
      let criteria = section_lines(&lines, crit[0]);
      check_subsection(&criteria, "### Spec/plan")?;
      check_subsection(&criteria, "### Standards")?;

      Ok(ParsedReview { verdict_pass, head, base })
  }
  ```
  Then in `gatekeeper/src/main.rs`, add the module declaration directly under the existing
  `mod json;` line (currently line 19), so it reads:
  ```rust
  mod json;
  mod review;
  ```
- **Test:** Add a smoke test at the bottom of `review.rs` (the full battery comes in Task 2):
  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;

      const PASS: &str = "VERDICT: pass\nHEAD: 9f3c1a7e5b2d8c4f0a1b6e7d9c2f5a8b3e4d6c1a\nBASE: 2a7d4e1c9b6f3a8d5e2c1b0a9f8e7d6c5b4a3210\n\n# Review\n\n## Blocking findings\nNone.\n\n## Criteria checked\n### Spec/plan\n- crit one — met\n### Standards\n- adr rule — met\n";

      #[test]
      fn smoke_valid_pass_parses() {
          let r = parse_review(PASS).unwrap();
          assert!(r.verdict_pass);
          assert_eq!(r.head, "9f3c1a7e5b2d8c4f0a1b6e7d9c2f5a8b3e4d6c1a");
      }
  }
  ```
  Run `cd gatekeeper && cargo test review::tests::smoke_valid_pass_parses` → expect `1 passed`.
- **Commit:** `feat(gatekeeper): add fail-closed review-artifact parser`

### Task 2: Parser unit-test battery — one test per grammar rule
- **File(s):** `gatekeeper/src/review.rs` (extend the `#[cfg(test)] mod tests`)
- **Change:** Add these tests and constants alongside `smoke_valid_pass_parses`. They encode every
  parser-level acceptance criterion from the spec. `PASS` is the constant from Task 1:
  ```rust
  const FAIL_DOC: &str = "VERDICT: fail\nHEAD: 9f3c1a7e5b2d8c4f0a1b6e7d9c2f5a8b3e4d6c1a\nBASE: 2a7d4e1c9b6f3a8d5e2c1b0a9f8e7d6c5b4a3210\n\n# Review\n\n## Blocking findings\n- src/foo.rs:42 — wrong and why\n\n## Criteria checked\n### Spec/plan\n- crit — partial\n### Standards\n- rule — violated\n";

  // The literal pass/fail examples from the spec (docs/specs/2026-06-05-code-review-gate.md,
  // the two ```markdown blocks). Regression guard against a documented-but-invalid template.
  const SPEC_PASS_SAMPLE: &str = "VERDICT: pass\nHEAD: 9f3c1a7e5b2d8c4f0a1b6e7d9c2f5a8b3e4d6c1a\nBASE: 2a7d4e1c9b6f3a8d5e2c1b0a9f8e7d6c5b4a3210\n\n# Review: <feature> (<date>)\n\n## Blocking findings\nNone.\n\n## Non-blocking notes\n- <nits — never gate on these>\n\n## Criteria checked\n### Spec/plan\n- <acceptance criterion 1> — <how the diff satisfies it>\n### Standards\n- <ADR/AGENTS rule> — <conformance evidence>\n";
  const SPEC_FAIL_SAMPLE: &str = "VERDICT: fail\nHEAD: 9f3c1a7e5b2d8c4f0a1b6e7d9c2f5a8b3e4d6c1a\nBASE: 2a7d4e1c9b6f3a8d5e2c1b0a9f8e7d6c5b4a3210\n\n# Review: <feature> (<date>)\n\n## Blocking findings\n- src/foo.rs:42 — <what's wrong and why it blocks>\n\n## Non-blocking notes\n- ...\n\n## Criteria checked\n### Spec/plan\n- ...\n### Standards\n- ...\n";

  #[test]
  fn fail_doc_parses_as_fail() {
      assert!(!parse_review(FAIL_DOC).unwrap().verdict_pass);
  }
  #[test]
  fn bad_verdict_keyword_rejected() {
      let t = PASS.replacen("VERDICT: pass", "VERDICT: PASS", 1);
      assert!(parse_review(&t).is_err());
  }
  #[test]
  fn abbreviated_head_sha_rejected() {
      let t = PASS.replacen("9f3c1a7e5b2d8c4f0a1b6e7d9c2f5a8b3e4d6c1a", "9f3c1a7", 1);
      assert!(parse_review(&t).is_err());
  }
  #[test]
  fn abbreviated_base_sha_rejected() {
      let t = PASS.replacen("2a7d4e1c9b6f3a8d5e2c1b0a9f8e7d6c5b4a3210", "2a7d4e1", 1);
      assert!(parse_review(&t).is_err());
  }
  #[test]
  fn malformed_base_line_rejected() {
      let t = PASS.replacen("BASE: ", "BSE: ", 1);
      assert!(parse_review(&t).is_err());
  }
  #[test]
  fn pass_with_a_blocking_item_rejected() {
      let t = PASS.replacen("None.", "- src/x.rs:1 — sneaky", 1);
      assert!(parse_review(&t).is_err());
  }
  #[test]
  fn fail_without_items_rejected() {
      let t = FAIL_DOC.replacen("- src/foo.rs:42 — wrong and why", "None.", 1);
      assert!(parse_review(&t).is_err());
  }
  #[test]
  fn fail_with_none_and_item_rejected() {
      // A 'fail' that also carries the 'None.' sentinel is contradictory -> reject (CONCERN #2).
      let t = FAIL_DOC.replacen(
          "- src/foo.rs:42 — wrong and why",
          "None.\n- src/foo.rs:42 — wrong and why",
          1,
      );
      assert!(parse_review(&t).is_err());
  }
  #[test]
  fn two_blocking_headings_rejected() {
      let t = PASS.replacen("## Criteria checked", "## Blocking findings\nNone.\n\n## Criteria checked", 1);
      assert!(parse_review(&t).is_err());
  }
  #[test]
  fn zero_blocking_headings_rejected() {
      let t = PASS.replacen("## Blocking findings\nNone.\n\n", "", 1);
      assert!(parse_review(&t).is_err());
  }
  #[test]
  fn missing_standards_dimension_rejected() {
      let t = PASS.replacen("### Standards\n- adr rule — met\n", "", 1);
      assert!(parse_review(&t).is_err());
  }
  #[test]
  fn empty_specplan_dimension_rejected() {
      let t = PASS.replacen("### Spec/plan\n- crit one — met\n", "### Spec/plan\n", 1);
      assert!(parse_review(&t).is_err());
  }
  #[test]
  fn comment_in_blocking_section_rejected() {
      // Inject a comment into an otherwise-valid FAIL doc, so ONLY the comment rule can reject it.
      assert!(parse_review(FAIL_DOC).is_ok());
      let t = FAIL_DOC.replacen(
          "- src/foo.rs:42 — wrong and why",
          "- src/foo.rs:42 — wrong and why\n<!-- hide a finding -->",
          1,
      );
      assert!(parse_review(&t).is_err());
  }
  #[test]
  fn unclosed_comment_in_blocking_section_rejected() {
      // strip_comments() is fail-OPEN on an unclosed comment; this parser must be fail-CLOSED.
      let t = FAIL_DOC.replacen(
          "- src/foo.rs:42 — wrong and why",
          "- src/foo.rs:42 — wrong and why\n<!-- unclosed",
          1,
      );
      assert!(parse_review(&t).is_err());
  }
  #[test]
  fn bom_and_crlf_header_handled() {
      let t = format!("\u{feff}{}", PASS.replace('\n', "\r\n"));
      assert!(parse_review(&t).unwrap().verdict_pass);
  }
  #[test]
  fn honest_quoting_of_verdict_does_not_false_fail() {
      let t = PASS.replacen("# Review\n", "# Review\n\nthe critic wrote VERDICT: pass in prose\n", 1);
      assert!(parse_review(&t).unwrap().verdict_pass);
  }
  #[test]
  fn spec_pass_sample_parses() {
      assert!(parse_review(SPEC_PASS_SAMPLE).unwrap().verdict_pass);
  }
  #[test]
  fn spec_fail_sample_parses() {
      assert!(!parse_review(SPEC_FAIL_SAMPLE).unwrap().verdict_pass);
  }
  ```
- **Test:** `cd gatekeeper && cargo test review::tests` → expect `19 passed` (the 18 here + the Task 1
  smoke test).
- **Commit:** `test(gatekeeper): cover every review-parser grammar rule`

### Task 3: Add git helpers, the `gate_review` orchestration, and wire the CLI
- **File(s):** `gatekeeper/src/review.rs`, `gatekeeper/src/main.rs`
- **Change:** Append to `gatekeeper/src/review.rs` (above the test modules):
  ```rust
  /// Run `git -C <root> <args>`, returning trimmed stdout on success.
  fn git(root: &Path, args: &[&str]) -> Option<String> {
      let out = Command::new("git").arg("-C").arg(root).args(args).output().ok()?;
      if out.status.success() {
          Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
      } else {
          None
      }
  }

  /// The `review` gate. `root` is the framework root; all git runs with `-C root`
  /// so the result is independent of the process working directory. Returns a
  /// process exit code: 0 pass, 1 veto, 2 usage error.
  pub fn gate_review(root: &Path, feature: &str, base_ref: Option<&str>) -> i32 {
      if feature.is_empty() {
          eprintln!("gatekeeper: --feature <slug> is required");
          return 2;
      }

      let head = match git(root, &["rev-parse", "HEAD"]) {
          Some(h) => h,
          None => {
              println!("FAIL review gate: not a git repository (git rev-parse HEAD failed)");
              return 1;
          }
      };

      // Clean worktree, ignoring untracked/modified paths under docs/reviews/.
      // `--untracked-files=all` lists files individually so an untracked directory is NOT
      // collapsed to a bare `docs/` entry (which would slip past the docs/reviews/ filter).
      // A failed status is fail-closed, never assumed clean.
      let porcelain = match git(root, &["status", "--porcelain", "--untracked-files=all"]) {
          Some(p) => p,
          None => {
              println!("FAIL review gate: git status failed");
              return 1;
          }
      };
      let dirty: Vec<&str> = porcelain
          .lines()
          .filter(|l| !l.get(3..).unwrap_or("").starts_with("docs/reviews/"))
          .collect();
      if !dirty.is_empty() {
          println!("FAIL review gate: uncommitted changes outside docs/reviews/:");
          for l in &dirty {
              println!("  {l}");
          }
          return 1;
      }

      let branch = base_ref.unwrap_or("main");
      let base = match git(root, &["merge-base", branch, "HEAD"]) {
          Some(b) => b,
          None => {
              println!("FAIL review gate: cannot resolve merge-base of '{branch}' and HEAD");
              return 1;
          }
      };

      // Select artifacts whose line-2 HEAD equals the current HEAD.
      let dir = root.join("docs").join("reviews");
      let suffix = format!("-{feature}.md");
      let want_head = format!("HEAD: {head}");
      let mut matches: Vec<PathBuf> = Vec::new();
      if let Ok(rd) = fs::read_dir(&dir) {
          for e in rd.flatten() {
              let p = e.path();
              let name = match p.file_name() {
                  Some(n) => n.to_string_lossy().to_string(),
                  None => continue,
              };
              if !name.ends_with(&suffix) {
                  continue;
              }
              let txt = fs::read_to_string(&p).unwrap_or_default();
              if normalize(&txt).split('\n').nth(1) == Some(want_head.as_str()) {
                  matches.push(p);
              }
          }
      }

      match matches.len() {
          0 => {
              println!("FAIL review gate: no review artifact names current HEAD {head}");
              1
          }
          1 => {
              let path = &matches[0];
              let text = fs::read_to_string(path).unwrap_or_default();
              match parse_review(&text) {
                  Err(e) => {
                      println!("FAIL review gate: {} — {e}", path.display());
                      1
                  }
                  Ok(r) => {
                      if r.base != base {
                          println!(
                              "FAIL review gate: BASE {} != computed merge-base {base}",
                              r.base
                          );
                          return 1;
                      }
                      if !r.verdict_pass {
                          println!("FAIL review gate: verdict is fail ({})", path.display());
                          return 1;
                      }
                      println!("PASS review gate: {}", path.display());
                      0
                  }
              }
          }
          _ => {
              println!(
                  "FAIL review gate: {} artifacts name current HEAD (ambiguous):",
                  matches.len()
              );
              for p in &matches {
                  println!("  {}", p.display());
              }
              1
          }
      }
  }
  ```
  In `gatekeeper/src/main.rs`, add the `"review"` arm to the `cmd_check` match (after the
  `"finish"` arm, before `other`):
  ```rust
          "review" => review::gate_review(&framework_root(), &feature_arg(args), base_arg(args).as_deref()),
  ```
  Add the `base_arg` helper directly below the existing `feature_arg` function:
  ```rust
  fn base_arg(args: &[String]) -> Option<String> {
      let mut it = args.iter();
      while let Some(a) = it.next() {
          if a == "--base" {
              return it.next().cloned();
          }
      }
      None
  }
  ```
  Extend `print_help()`: replace the verify+finish lines of the USAGE block —
  ```rust
           gatekeeper check verify --feature <slug>\n  \
           gatekeeper check finish -- <command...>\n"
  ```
  with (inserting the `review` line between them):
  ```rust
           gatekeeper check verify --feature <slug>\n  \
           gatekeeper check review --feature <slug> [--base <ref>]\n  \
           gatekeeper check finish -- <command...>\n"
  ```
  Extend the `//!` doc block: replace the verify+finish lines —
  ```rust
  //!   gatekeeper check verify  --feature S    Verify gate: a verification note exists.
  //!   gatekeeper check finish  -- <cmd...>    Finish gate: <cmd> exits 0.
  ```
  with:
  ```rust
  //!   gatekeeper check verify  --feature S    Verify gate: a verification note exists.
  //!   gatekeeper check review  --feature S    Review gate: a fresh critic's artifact passes.
  //!   gatekeeper check finish  -- <cmd...>    Finish gate: <cmd> exits 0.
  ```
- **Test:** `cd gatekeeper && cargo test` → the existing suite is still green and `cargo build`
  succeeds. Manual usage check: `cargo run -- check review` (no `--feature`) → prints the `--feature`
  error and exits `2`.
- **Commit:** `feat(gatekeeper): add review gate (git state + artifact selection)`

### Task 4: Integration tests for the gate (temp repo, in-crate + a nested-dir CLI test)
- **File(s):** `gatekeeper/src/review.rs` (add a second test module `gate_tests`),
  `gatekeeper/tests/cli_review.rs` (new)
- **Change (a):** Add the `gate_tests` module to `review.rs`. It builds a throwaway git repo per
  test, so git state is hermetic. It never changes the process working directory (cargo runs tests in
  parallel threads in one process), which also proves the gate uses `git -C root`, not cwd. **Note the
  import block is only `use super::*;` plus `use std::env;`** — `fs`, `Command`, `Path`, and `PathBuf`
  already come through the glob, and re-importing them trips `clippy -D warnings`:
  ```rust
  #[cfg(test)]
  mod gate_tests {
      use super::*;
      use std::env;

      fn run(root: &Path, args: &[&str]) {
          let ok = Command::new("git").arg("-C").arg(root).args(args).status().unwrap().success();
          assert!(ok, "git {args:?} failed");
      }

      // Build a repo with one commit on `main`; return (root, head_sha).
      fn repo(tag: &str) -> (PathBuf, String) {
          let root = env::temp_dir().join(format!("topo_gate_{}_{}", tag, std::process::id()));
          let _ = fs::remove_dir_all(&root);
          fs::create_dir_all(&root).unwrap();
          run(&root, &["init", "-q", "-b", "main"]);
          run(&root, &["config", "user.email", "t@t.t"]);
          run(&root, &["config", "user.name", "t"]);
          fs::write(root.join("a.txt"), "one\n").unwrap();
          run(&root, &["add", "."]);
          run(&root, &["commit", "-q", "-m", "init"]);
          let head = git(&root, &["rev-parse", "HEAD"]).unwrap();
          (root, head)
      }

      fn write_artifact(root: &Path, head: &str, base: &str, verdict_pass: bool) {
          let dir = root.join("docs").join("reviews");
          fs::create_dir_all(&dir).unwrap();
          let (v, blk) = if verdict_pass {
              ("pass", "None.")
          } else {
              ("fail", "- a.txt:1 — wrong")
          };
          let body = format!(
              "VERDICT: {v}\nHEAD: {head}\nBASE: {base}\n\n# Review\n\n## Blocking findings\n{blk}\n\n## Criteria checked\n### Spec/plan\n- crit — met\n### Standards\n- rule — met\n"
          );
          fs::write(dir.join("2026-06-05-code-review-gate.md"), body).unwrap();
      }

      #[test]
      fn fresh_pass_exits_zero() {
          let (root, head) = repo("pass");
          write_artifact(&root, &head, &head, true); // single-commit repo: merge-base(main,HEAD)==HEAD
          assert_eq!(gate_review(&root, "code-review-gate", None), 0);
          let _ = fs::remove_dir_all(&root);
      }
      #[test]
      fn fresh_artifact_does_not_self_dirty() {
          // The untracked artifact under docs/reviews/ is present (-uall shows it) ...
          let (root, head) = repo("selfdirty");
          write_artifact(&root, &head, &head, true);
          let porcelain = git(&root, &["status", "--porcelain", "--untracked-files=all"]).unwrap();
          assert!(porcelain.lines().any(|l| l.contains("docs/reviews/")));
          // ... yet the gate still passes, because the clean-tree check excludes docs/reviews/.
          assert_eq!(gate_review(&root, "code-review-gate", None), 0);
          let _ = fs::remove_dir_all(&root);
      }
      #[test]
      fn stale_head_exits_one() {
          let (root, head) = repo("stale");
          write_artifact(&root, "0000000000000000000000000000000000000000", &head, true);
          assert_eq!(gate_review(&root, "code-review-gate", None), 1);
          let _ = fs::remove_dir_all(&root);
      }
      #[test]
      fn dirty_outside_reviews_exits_one() {
          let (root, head) = repo("dirty");
          write_artifact(&root, &head, &head, true);
          fs::write(root.join("a.txt"), "changed\n").unwrap(); // tracked file modified
          assert_eq!(gate_review(&root, "code-review-gate", None), 1);
          let _ = fs::remove_dir_all(&root);
      }
      #[test]
      fn untracked_outside_reviews_exits_one() {
          // An untracked file OUTSIDE docs/reviews/ must dirty the tree (proves the filter scope).
          let (root, head) = repo("untracked_out");
          write_artifact(&root, &head, &head, true);
          fs::write(root.join("stray.txt"), "junk\n").unwrap();
          assert_eq!(gate_review(&root, "code-review-gate", None), 1);
          let _ = fs::remove_dir_all(&root);
      }
      #[test]
      fn wrong_base_exits_one() {
          let (root, head) = repo("wrongbase");
          write_artifact(&root, &head, "1111111111111111111111111111111111111111", true);
          assert_eq!(gate_review(&root, "code-review-gate", None), 1);
          let _ = fs::remove_dir_all(&root);
      }
      #[test]
      fn fail_verdict_exits_one() {
          let (root, head) = repo("failv");
          write_artifact(&root, &head, &head, false);
          assert_eq!(gate_review(&root, "code-review-gate", None), 1);
          let _ = fs::remove_dir_all(&root);
      }
      #[test]
      fn unresolvable_base_exits_one() {
          let (root, head) = repo("nobase");
          write_artifact(&root, &head, &head, true);
          assert_eq!(gate_review(&root, "code-review-gate", Some("no-such-branch")), 1);
          let _ = fs::remove_dir_all(&root);
      }
      #[test]
      fn not_a_repo_exits_one() {
          let root = env::temp_dir().join(format!("topo_gate_norepo_{}", std::process::id()));
          let _ = fs::remove_dir_all(&root);
          fs::create_dir_all(&root).unwrap();
          assert_eq!(gate_review(&root, "code-review-gate", None), 1);
          let _ = fs::remove_dir_all(&root);
      }
      #[test]
      fn ambiguous_two_artifacts_same_head_exits_one() {
          let (root, head) = repo("ambig");
          write_artifact(&root, &head, &head, true); // 2026-06-05-code-review-gate.md
          let body = fs::read_to_string(root.join("docs/reviews/2026-06-05-code-review-gate.md")).unwrap();
          fs::write(root.join("docs/reviews/2026-06-06-code-review-gate.md"), body).unwrap();
          assert_eq!(gate_review(&root, "code-review-gate", None), 1);
          let _ = fs::remove_dir_all(&root);
      }
      #[test]
      fn divergent_branch_uses_fork_point_as_base() {
          // Real divergence: main stays at C0; feature advances to C1. merge-base(main,HEAD)==C0.
          let (root, base) = repo("divergent");
          run(&root, &["checkout", "-q", "-b", "feature"]);
          fs::write(root.join("a.txt"), "two\n").unwrap();
          run(&root, &["add", "."]);
          run(&root, &["commit", "-q", "-m", "feature work"]);
          let head = git(&root, &["rev-parse", "HEAD"]).unwrap();
          assert_ne!(head, base);
          // Correct review: BASE is the fork point, not HEAD.
          write_artifact(&root, &head, &base, true);
          assert_eq!(gate_review(&root, "code-review-gate", None), 0);
          // A review lying with BASE == HEAD must be rejected (merge-base != HEAD here).
          write_artifact(&root, &head, &head, true);
          assert_eq!(gate_review(&root, "code-review-gate", None), 1);
          let _ = fs::remove_dir_all(&root);
      }
  }
  ```
- **Change (b):** Create `gatekeeper/tests/cli_review.rs` — an integration test that runs the
  **compiled binary** from a **nested subdirectory** (spec acceptance line 311). `skills/` marks the
  framework root so `framework_root()` resolves to `root` when invoked from `root/src/deep/nested`:
  ```rust
  use std::fs;
  use std::path::Path;
  use std::process::Command;

  fn git(root: &Path, args: &[&str]) {
      let ok = Command::new("git").arg("-C").arg(root).args(args).status().unwrap().success();
      assert!(ok, "git {args:?} failed");
  }

  #[test]
  fn review_gate_runs_from_nested_subdir() {
      let root = std::env::temp_dir().join(format!("topo_cli_{}", std::process::id()));
      let _ = fs::remove_dir_all(&root);
      fs::create_dir_all(root.join("skills")).unwrap(); // marks the framework root
      git(&root, &["init", "-q", "-b", "main"]);
      git(&root, &["config", "user.email", "t@t.t"]);
      git(&root, &["config", "user.name", "t"]);
      fs::write(root.join("a.txt"), "one\n").unwrap();
      git(&root, &["add", "."]);
      git(&root, &["commit", "-q", "-m", "init"]);

      let out = Command::new("git")
          .arg("-C")
          .arg(&root)
          .args(["rev-parse", "HEAD"])
          .output()
          .unwrap();
      let head = String::from_utf8(out.stdout).unwrap().trim().to_string();

      let dir = root.join("docs").join("reviews");
      fs::create_dir_all(&dir).unwrap();
      let body = format!(
          "VERDICT: pass\nHEAD: {head}\nBASE: {head}\n\n# Review\n\n## Blocking findings\nNone.\n\n## Criteria checked\n### Spec/plan\n- crit — met\n### Standards\n- rule — met\n"
      );
      fs::write(dir.join("2026-06-05-code-review-gate.md"), body).unwrap();

      let nested = root.join("src").join("deep").join("nested");
      fs::create_dir_all(&nested).unwrap();
      let status = Command::new(env!("CARGO_BIN_EXE_gatekeeper"))
          .current_dir(&nested)
          .args(["check", "review", "--feature", "code-review-gate"])
          .status()
          .unwrap();
      assert_eq!(status.code(), Some(0), "gate should pass from a nested subdir");
      let _ = fs::remove_dir_all(&root);
  }
  ```
- **Test:** `cd gatekeeper && cargo test review::gate_tests` → expect `11 passed`;
  `cargo test --test cli_review` → expect `1 passed`. (Both require `git` on PATH — already a runtime
  dependency of the gate.)
- **Commit:** `test(gatekeeper): integration-test the review gate (temp repo + nested-dir CLI)`

### Task 5: Author the `code-review` critic skill
- **File(s):** `skills/code-review/SKILL.md` (new)
- **Change:** Create the file with this exact content:
  ```markdown
  ---
  name: code-review
  description: Dispatch a fresh-context critic subagent to audit the branch diff against the plan and the repo's standards, then write a commit-bound review artifact the review gate checks. Use after verify-before-done passes and before finish-branch, or when the user asks for a review, audit, or critique before merge.
  ---

  # Code Review (the review gate)

  The author cannot grade their own work — a separate critic must. Dispatch a **fresh subagent**
  (no memory of writing the code), preferably a **different model** where the harness allows it.

  ## Process

  1. **Compute the diff scope.** `base = git merge-base <integration-branch> HEAD` (default
     `main`; override with `--base`). The critic reviews `git diff <base>...HEAD` (three-dot).
  2. **Dispatch one fresh critic subagent** with the diff, the design doc, the plan, and the repo
     standards (`docs/adr/`, `AGENTS.md`, `METHODOLOGY.md`, `CONTEXT.md` if present).
  3. **Review two dimensions separately and document each:**
     - **Spec/plan conformance** — does the diff implement the acceptance criteria and the plan?
       Flag missing, partial, or scope-creep.
     - **Standards conformance** — does the diff follow the cited ADRs / AGENTS / METHODOLOGY?
       Cite the standard.
  4. **Require evidence.** Every blocking finding cites `file:line`. No location -> not a blocker.
  5. **Seek reasons to FAIL first.** Skip tooling-enforced checks (lint/format/types the `finish`
     gate or linters catch). Distinguish hard violations from judgement calls.
  6. **Write the artifact atomically** to `docs/reviews/<YYYY-MM-DD>-<feature-slug>.md`
     (write a temp file, then rename) using the exact grammar below. Never put HTML comments or
     raw diff lines in the machine-parsed regions.

  ## Artifact grammar (the gate's contract)

  Lines 1-3 are the machine header. Use `git rev-parse HEAD` and the computed merge-base verbatim
  (full SHAs). A passing review looks exactly like:

  ```
  VERDICT: pass
  HEAD: <full 40/64-hex sha from git rev-parse HEAD>
  BASE: <full 40/64-hex merge-base sha>

  # Review: <feature> (<date>)

  ## Blocking findings
  None.

  ## Non-blocking notes
  - <nits — never gate on these>

  ## Criteria checked
  ### Spec/plan
  - <acceptance criterion> — <how the diff satisfies it>
  ### Standards
  - <ADR/AGENTS rule> — <conformance evidence>
  ```

  A failing review is identical except line 1 is `VERDICT: fail` and `## Blocking findings` lists
  one or more `- <file:line> — <why it blocks>` items instead of `None.`.

  ## Gate check

  ```bash
  gatekeeper check review --feature <feature-slug> [--base <ref>]
  ```

  Passes only when: the worktree is clean (except `docs/reviews/`), exactly one artifact names the
  current `HEAD`, its `BASE` equals the computed merge-base, both rubric dimensions are present, and
  the verdict is `pass` with no blocking findings. Every ambiguity fails closed. Then transition to
  `finish-branch`.

  ## The bar

  A critic that found problems cannot be rubber-stamped, and a review of stale, dirty, or
  wrong-based code cannot be replayed. The parser and git state — not the model's prose — are the
  trust boundary.
  ```
- **Test:** `cd gatekeeper && cargo run -- list` → expect a `code-review` line whose description
  starts `Dispatch a fresh-context critic subagent ...` (proves frontmatter parses).
- **Commit:** `feat(skills): add code-review critic skill`

### Task 6: Route the `code-review` skill in `skill-rules.json`
- **File(s):** `hooks/skill-rules.json`
- **Change:** Insert a `code-review` entry between the `verify-before-done` and `finish-branch`
  entries (so routing order mirrors the gate order):
  ```json
      "code-review": {
        "type": "process",
        "enforcement": "require",
        "priority": "high",
        "promptTriggers": {
          "keywords": ["review", "audit", "critique", "before merge", "code review"]
        }
      },
  ```
- **Test:** `cd gatekeeper && printf 'please review this before merge' | cargo run -- activate`
  → expect output listing `- code-review [require]`.
- **Commit:** `feat(hooks): route the code-review skill on review keywords`

### Task 7: Write ADR 0006
- **File(s):** `docs/adr/0006-code-review-gate.md` (new)
- **Change:** Create the file with this exact content:
  ```markdown
  # 0006 — The code-review gate is a commit-bound, fail-closed critic artifact

  - **Status:** Accepted
  - **Date:** 2026-06-05

  Topology's `verify` gate has the author grade their own work — the weakest form of review, and the
  system's main gap. We add a `review` gate between `verify` and `finish`: a fresh-context critic
  subagent audits the branch diff and writes a review artifact that a new `gatekeeper check review`
  subcommand validates against git state.

  ## Why

  LLMs exhibit measurable self-preference bias, worst exactly when the output is wrong, and
  self-refinement amplifies it — so a *separate* critic is the evidence-backed fix (Zheng et al.
  2023; Panickssery et al. 2024). The critic's verdict is untrusted: prompt injection coerces LLM
  judges into passing verdicts at 30-73.8% on a code-review task (Maloyan & Namiot 2025). The trust
  boundary is therefore the deterministic parser + git state, not the model.

  ## Decisions

  - **(a) Artifact bound to a clean commit + merge-base.** The review counts only for the exact
    `HEAD` (clean worktree, excluding `docs/reviews/`), against the verified `git merge-base` of the
    integration branch. A stale, dirty, or wrong-based review cannot be replayed.
  - **(b) Fail-closed grammar; `strip_comments` not reused.** A line-1 verdict, full-hex
    HEAD/BASE, exactly one blocking heading, pass <=> `None.`, both rubric dimensions required, and
    no HTML comments in parsed regions. Any deviation is a veto. `strip_comments` is fail-open on an
    unclosed comment, so the parser does not reuse it.
  - **(c) A single two-dimension critic.** Deep research refuted that multiple parallel *agents*
    catch more than one critic, so we ship one critic — but it audits *both* the Spec/plan and
    Standards *dimensions*, gate-enforced. Multi-critic voting (the one evidence-backed reason to run
    more than one agent, for injection-robustness) is deferred.
  - **(d) Pulled ahead of security scanning.** The self-review gap is the highest-leverage fix, so
    the gate ships before Phase 1 security scanning.

  ## Consequences

  - `gatekeeper` grows a `review.rs` module (pure parser + git-state gate); no new dependency.
  - A residual is accepted and documented: a fully-subverted critic emitting a clean `pass` for the
    correct head/base is undetectable by any parser. Reducers: `file:line` evidence, a different-model
    critic, and future voting.
  - The review artifact is a transient gate input committed by `finish-branch`; re-running the gate
    after that commit fails (it is a pre-finish check, not a CI replay). A tree/diff-hash binding
    would make it replay-safe (future work).
  ```
- **Test:** `ls docs/adr/0006-code-review-gate.md` → file exists; visually confirm it renders.
- **Commit:** `docs(adr): record the code-review gate contract (ADR 0006)`

### Task 8: Update METHODOLOGY.md (sequence, gate table, pillar status)
- **File(s):** `METHODOLOGY.md`
- **Change:** Three edits, each old-string taken **verbatim** from the file.
  1. In the §4 sequence diagram, replace these two lines (lines 114-115):
     ```
     research ─► brainstorm-design ─► write-plan ─► tdd-loop ─► verify-before-done ─► finish-branch
     (research)    (design gate)      (plan gate)   (tdd gate)     (verify gate)        (finish gate)
     ```
     with:
     ```
     research ─► brainstorm-design ─► write-plan ─► tdd-loop ─► verify-before-done ─► code-review ─► finish-branch
     (research)    (design gate)      (plan gate)   (tdd gate)     (verify gate)      (review gate)   (finish gate)
     ```
  2. In the §4 gate table, replace the `finish` row (line 127) —
     ```
     | **finish** `[built]` | the full test suite passes | `gatekeeper check finish -- <cmd>` |
     ```
     with the new `review` row followed by the unchanged `finish` row:
     ```
     | **review** `[built]` | a fresh-context critic's artifact passes — bound to the current clean `HEAD` and merge-base, both rubric dimensions present, no blocking findings | `gatekeeper check review --feature <slug> [--base <ref>]` |
     | **finish** `[built]` | the full test suite passes | `gatekeeper check finish -- <cmd>` |
     ```
  3. In Pillar 1, replace these two **verbatim** lines (lines 142-143) —
     ```
     Today: 7 process skills. Planned: domain
     skills, plus meta skills `capture-gotcha`, `new-skill`, and a `code-review` critic.
     ```
     with:
     ```
     Today: 8 process skills (including the `code-review` critic). Planned: domain
     skills, plus meta skills `capture-gotcha` and `new-skill`.
     ```
- **Test:** `grep -n 'review gate' METHODOLOGY.md` → expect the new sequence + table lines;
  confirm the §4 gate table now has 7 data rows (research, design, plan, tdd, verify, review, finish).
- **Commit:** `docs(methodology): add the review gate to the sequence and table`

### Task 9: Update README.md gate table and the "what's next" line
- **File(s):** `README.md`
- **Change:** Two edits.
  1. In the "gates, not rules" table, replace the `finish` row (verbatim) —
     ```
     | `finish`  | the full test suite passes (`gatekeeper check finish -- <cmd>`) |
     ```
     with the new `review` row followed by the unchanged `finish` row:
     ```
     | `review`  | a fresh-context critic's review artifact passes for the current clean `HEAD` (bound to merge-base, both dimensions, no blockers) |
     | `finish`  | the full test suite passes (`gatekeeper check finish -- <cmd>`) |
     ```
  2. Replace `- **[docs/ROADMAP.md](docs/ROADMAP.md)** — the phased path from today's gates to the full system (security scanning is next).`
     with `- **[docs/ROADMAP.md](docs/ROADMAP.md)** — the phased path from today's gates to the full system (the code-review gate landed early; security scanning is next).`
- **Test:** `grep -n '`review`' README.md` → expect the new gate-table row.
- **Commit:** `docs(readme): document the review gate`

### Task 10: Update the verify and finish skills for the new order
- **File(s):** `skills/verify-before-done/SKILL.md`, `skills/finish-branch/SKILL.md`
- **Change:** Two files.
  1. In `skills/verify-before-done/SKILL.md`, replace `Passes when the verification note exists. Then transition to `finish-branch`.`
     with `Passes when the verification note exists. Then transition to `code-review`.`
  2. In `skills/finish-branch/SKILL.md`:
     - replace `Only enter after `verify-before-done` passes.` with
       `Only enter after `code-review` passes (which itself follows `verify-before-done`).`
     - replace the step-4 line `4. **On merge/PR**, write the summary from the design + verify docs so the history is legible.`
       with:
       `4. **On merge/PR**, first commit the review artifact for this `HEAD` (`git add docs/reviews/ && git commit -m "docs(review): <feature> code review"`) so the merge records the review, then write the summary from the design, verify, and review docs so the history is legible.`
       *(The artifact is committed only on the merge/PR path — after the user has chosen in step 3 —
       so the gate never changes history before presenting options.)*
- **Test:** `grep -n 'code-review' skills/verify-before-done/SKILL.md skills/finish-branch/SKILL.md`
  → expect the transition + entry-condition lines; confirm the artifact-commit text is inside step 4.
- **Commit:** `docs(skills): thread the review gate through verify and finish`

### Task 11: Record the pull-forward in ROADMAP.md
- **File(s):** `docs/ROADMAP.md`
- **Change:** Three edits.
  1. Reconcile the intro note (line 7) — replace
     `> This is the plan, not a changelog. Only **Phase 0** is delivered. Phases 1–6 are designed and`
     with
     `> This is the plan, not a changelog. **Phase 0** is delivered, plus the **code-review gate** pulled forward from Phase 5 (Phase 1.5 below). Phases 1–6 are otherwise designed and`
  2. In "Phase 5 — Memory + research-first hardening", under **Deliverables**, replace
     `- Domain skills for the house stack + a `code-review` critic skill that dispatches a fresh-context subagent.`
     with
     `- Domain skills for the house stack. *(The `code-review` critic skill + `review` gate were pulled forward and delivered 2026-06-05 — see `docs/adr/0006-code-review-gate.md`.)*`
  3. In "Status at a glance", replace the Phase 1 row (line 161) —
     `| 1 | Security scanning | ⏳ planned (next) |`
     with the Phase 1 row followed by a new Phase 1.5 row:
     ```
     | 1 | Security scanning | ⏳ planned (next) |
     | 1.5 | Code-review gate (pulled forward) | ✅ delivered |
     ```
- **Test:** `grep -n 'pulled forward' docs/ROADMAP.md` → expect edits 1-3 present; `grep -n 'Only \*\*Phase 0\*\*' docs/ROADMAP.md` → no match (the stale claim is gone).
- **Commit:** `docs(roadmap): record the code-review gate pulled forward from Phase 5`

### Task 12: Full verification + evidence note
- **File(s):** `docs/verify/2026-06-05-code-review-gate.md` (new)
- **Change:** Run the full suite and quality gates, then record the evidence. Commands:
  ```bash
  cd gatekeeper && cargo test
  cargo fmt --check
  cargo clippy -- -D warnings
  ```
  Create `docs/verify/2026-06-05-code-review-gate.md` capturing, for each spec acceptance criterion,
  the command run and the actual output observed (the `cargo test` summary lines; the
  `cargo run -- list` and `activate` checks from Tasks 5-6; a `gatekeeper check review` run in this
  repo). Map each line to the acceptance criterion it satisfies.
- **Test:** `cargo test` → **two** `test result: ok.` lines: the `gatekeeper` bin unittests report
  **36 passed** (4 `main.rs` + 2 `json.rs` + 19 `review::tests` + 11 `review::gate_tests`), and the
  `cli_review` integration binary reports **1 passed** (doctests: 0). `cargo fmt --check` exits 0;
  `cargo clippy -- -D warnings` exits 0; `gatekeeper check verify --feature code-review-gate` → exit 0.
- **Commit:** `test(gatekeeper): verify the code-review gate end to end`

## Sequencing notes

- Tasks 1-4 are the gate itself (TDD: parser → parser tests → gate → gate tests + the CLI test).
  Tasks 5-11 are the skill, routing, ADR, and doc wiring (no code risk). Task 12 is the verify gate.
- Tasks 1-2 and 3-4 may each be split finer during `tdd-loop` (one failing test at a time); they are
  grouped here because each pair is one cohesive unit.
- **Cargo.lock heads-up.** `gatekeeper/Cargo.lock` is currently untracked; for a binary crate it
  should be committed. Commit it on the branch (with the source), otherwise `gatekeeper check review`
  on this feature's own branch will see an untracked file outside `docs/reviews/` and veto (now that
  the clean-tree check is correct via `--untracked-files=all`).
- The whole feature lives on a branch off `main` (the design + plan docs are committed first); the
  review artifact for this feature's own branch is written, gated, then committed by `finish-branch`
  on the merge/PR path.
