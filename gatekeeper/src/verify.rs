//! Verify gate — evidence replay engine (spec §3).
//!
//! # Overview
//!
//! In **presence** mode (default) the gate only checks the file exists and emits a
//! static-analysis SHADOW line per artifact.  No commands are ever executed.
//!
//! In **replay** mode (`[verify] mode = "replay"`) evidence blocks are parsed and
//! each `$ step` is executed from the project root (argv split, no shell).
//! Fail-closed: zero blocks = fail, malformed block = fail.
//!
//! Under **`GATEKEEPER_SHADOW=replay`** (env var) the engine runs legacy-extraction +
//! evidence replay on every artifact regardless of configured mode, but the gate
//! exit code is still presence-mode's — shadow data only, never a demotion.
//!
//! # Evidence block grammar
//!
//! ````markdown
//! ```evidence
//! $ cargo test --manifest-path gatekeeper/Cargo.toml
//! # expect: test result: ok
//! # expect-re: \d+ passed
//! ```
//! ````
//!
//! - Lines starting with `$ ` open a step; their remainder is the raw command.
//! - Directive lines (`# expect:` / `# expect-re:`) bind to the preceding step.
//! - Any `^#\s*[\w-]+\s*:` line not recognized as a directive → block is malformed.
//! - A directive with no preceding step → block is malformed.
//! - A block with no `$ ` line → malformed.

use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use regex::Regex;

use crate::config::{ProjectConfig, VerifyMode};

// ── metachar / env-assignment rejection ──────────────────────────────────────

const METACHARS: &[char] = &['|', '&', ';', '<', '>', '$', '`', '\\', '(', ')', '"', '\''];

/// Reject a raw command string if it contains any metachar.
fn has_metachar(cmd: &str) -> bool {
    cmd.chars().any(|c| METACHARS.contains(&c))
}

/// Reject `NAME=value`-style env-assignment prefix (`[A-Za-z_][A-Za-z0-9_]*=`).
fn is_env_assignment(token: &str) -> bool {
    let mut chars = token.chars();
    let first = match chars.next() {
        Some(c) => c,
        None => return false,
    };
    if !first.is_ascii_alphabetic() && first != '_' {
        return false;
    }
    for c in chars {
        if c == '=' {
            return true;
        }
        if !c.is_ascii_alphanumeric() && c != '_' {
            break;
        }
    }
    false
}

/// Returns `true` if the command's leading argv tokens match an allowlist entry
/// using **token-boundary** matching (not raw-prefix substring).
///
/// Each allowlist entry is whitespace-split into tokens; a command matches iff
/// its leading argv tokens equal the entry's tokens.
///
/// Example: `"cargo test"` matches `"cargo test --manifest-path …"` but NOT
/// `"cargo testfoo"`.
pub fn is_command_allowed(cmd_argv: &[&str], allowed_prefixes: &[String]) -> bool {
    for entry in allowed_prefixes {
        let entry_tokens: Vec<&str> = entry.split_whitespace().collect();
        if entry_tokens.is_empty() {
            continue;
        }
        if cmd_argv.len() < entry_tokens.len() {
            continue;
        }
        if cmd_argv[..entry_tokens.len()] == *entry_tokens.as_slice() {
            return true;
        }
    }
    false
}

// ── SHADOW JSONL line emission ────────────────────────────────────────────────

/// The configured state for a check in SHADOW output.
#[derive(Debug, Clone, Copy)]
pub enum ShadowConfigured {
    Default,
    #[allow(dead_code)]
    Off,
    On,
    ShadowEnv,
}

impl ShadowConfigured {
    fn as_str(self) -> &'static str {
        match self {
            ShadowConfigured::Default => "default",
            ShadowConfigured::Off => "off",
            ShadowConfigured::On => "on",
            ShadowConfigured::ShadowEnv => "shadow-env",
        }
    }
}

/// The result field in a SHADOW line.
#[derive(Debug, Clone, Copy)]
pub enum ShadowResult {
    Pass,
    Fail,
    Skip,
    Static,
}

impl ShadowResult {
    fn as_str(self) -> &'static str {
        match self {
            ShadowResult::Pass => "pass",
            ShadowResult::Fail => "fail",
            ShadowResult::Skip => "skip",
            ShadowResult::Static => "static",
        }
    }
}

/// Emit one SHADOW JSONL line to stderr.
///
/// **Stderr contract (unchanged):** always emits exactly one line with prefix `SHADOW ` followed
/// by a JSON object with the seven fields: `gate`, `check`, `configured`, `artifact`, `command`,
/// `result`, `detail`. Integration tests pin this exact field set — do not change it.
///
/// **Best-effort JSONL sink:** also appends the verdict (with an extra leading `ts` field) to
/// `<artifacts_root>/logs/shadow.jsonl` for burn-in false-block rate measurement. The sink is
/// fail-silent — any I/O error is ignored so gates never break because the sink can't write.
/// Framework repo: `docs/logs/shadow.jsonl` (gitignored). Governed project:
/// `.claude/topology/logs/shadow.jsonl`.
pub fn emit_shadow(
    gate: &str,
    check: &str,
    configured: ShadowConfigured,
    artifact: Option<&str>,
    command: Option<&str>,
    result: ShadowResult,
    detail: &str,
) {
    fn json_str(s: &str) -> String {
        // Minimal JSON string escaping — only what's needed for paths/details.
        let mut out = String::with_capacity(s.len() + 2);
        out.push('"');
        for c in s.chars() {
            match c {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                c => out.push(c),
            }
        }
        out.push('"');
        out
    }
    let art_json = match artifact {
        Some(s) => json_str(s),
        None => "null".to_string(),
    };
    let cmd_json = match command {
        Some(s) => json_str(s),
        None => "null".to_string(),
    };
    // Build the inner field list once; reuse for stderr line and file line.
    let inner = format!(
        "\"gate\":{},\"check\":{},\"configured\":{},\"artifact\":{},\"command\":{},\"result\":{},\"detail\":{}",
        json_str(gate),
        json_str(check),
        json_str(configured.as_str()),
        art_json,
        cmd_json,
        json_str(result.as_str()),
        json_str(detail),
    );
    // Stderr line — field set is contractually frozen; integration tests assert it.
    eprintln!("SHADOW {{{inner}}}");
    // File line — same fields plus a leading `ts` (seconds since UNIX_EPOCH).
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let file_line = format!("{{\"ts\":{ts},{inner}}}");
    append_shadow_sink(&file_line);
}

/// Append `line` (without a trailing newline) to `<artifacts_root>/logs/shadow.jsonl`.
///
/// Fail-silent: any error at any step is ignored. Gates must never break because the
/// sink can't write.
fn append_shadow_sink(line: &str) {
    let sink_path = crate::artifacts_root().join("logs").join("shadow.jsonl");
    append_line_at(&sink_path, line);
}

/// Create `path`'s parent directory (if needed) and append `line + '\n'` to `path`.
///
/// This is the path-independent core factored out for unit-testability.
/// Any error is silently ignored.
pub(crate) fn append_line_at(path: &Path, line: &str) {
    let result = (|| -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut f = OpenOptions::new().append(true).create(true).open(path)?;
        f.write_all(line.as_bytes())?;
        f.write_all(b"\n")?;
        Ok(())
    })();
    let _ = result;
}

// ── evidence block parser ─────────────────────────────────────────────────────

/// A step parsed from an evidence block.
#[derive(Debug)]
pub struct EvidenceStep {
    pub raw_command: String,
    pub expect_literal: Vec<String>,
    pub expect_regex: Vec<String>,
}

/// Parse result for a single evidence block.
#[derive(Debug)]
pub enum BlockParseResult {
    /// Parsed zero or more steps successfully.
    Ok(Vec<EvidenceStep>),
    /// The block is malformed (detail message included).
    Malformed(String),
}

/// Widened malformed-directive pattern: `^#\s*[\w-]+\s*:`
fn looks_like_directive(line: &str) -> bool {
    // Must start with `#`, then optional whitespace, then word chars / hyphens,
    // then optional whitespace, then `:`.
    let s = line.trim_start_matches('#');
    let s = s.trim_start_matches([' ', '\t']);
    // Collect word-like chars (alphanumeric + hyphen + underscore)
    let word_end = s
        .find(|c: char| !c.is_alphanumeric() && c != '-' && c != '_')
        .unwrap_or(s.len());
    if word_end == 0 {
        return false;
    }
    let after_word = s[word_end..].trim_start_matches([' ', '\t']);
    after_word.starts_with(':')
}

/// Parse a single ```evidence block body (everything between the fences, exclusive).
pub fn parse_evidence_block(block_body: &str) -> BlockParseResult {
    let mut steps: Vec<EvidenceStep> = Vec::new();
    let mut has_step = false;

    for line in block_body.lines() {
        if let Some(cmd) = line.strip_prefix("$ ") {
            steps.push(EvidenceStep {
                raw_command: cmd.to_string(),
                expect_literal: Vec::new(),
                expect_regex: Vec::new(),
            });
            has_step = true;
        } else if let Some(rest) = line.strip_prefix("# expect: ") {
            if steps.is_empty() {
                return BlockParseResult::Malformed(
                    "directive '# expect:' with no preceding step".to_string(),
                );
            }
            let value = rest.trim();
            if value.is_empty() {
                // An empty expectation matches everything — a hollow assertion.
                return BlockParseResult::Malformed(
                    "directive '# expect:' with empty value".to_string(),
                );
            }
            steps
                .last_mut()
                .unwrap()
                .expect_literal
                .push(value.to_string());
        } else if let Some(rest) = line.strip_prefix("# expect-re: ") {
            if steps.is_empty() {
                return BlockParseResult::Malformed(
                    "directive '# expect-re:' with no preceding step".to_string(),
                );
            }
            let value = rest.trim();
            if value.is_empty() {
                return BlockParseResult::Malformed(
                    "directive '# expect-re:' with empty value".to_string(),
                );
            }
            steps
                .last_mut()
                .unwrap()
                .expect_regex
                .push(value.to_string());
        } else if line == "# expect:" || line == "# expect-re:" {
            // directive with no value — treat as malformed
            if steps.is_empty() {
                return BlockParseResult::Malformed("directive with no preceding step".to_string());
            }
            // bare directive keyword with no value — accept as empty expect (matches anything)
            // Actually spec says the widened shape catches near-misses; treat empty as malformed
            return BlockParseResult::Malformed(format!(
                "directive '{}' has no value",
                line.trim()
            ));
        } else if line.starts_with('#') && looks_like_directive(line) {
            // Widened malformed-directive: `^#\s*[\w-]+\s*:` that is NOT a recognized directive.
            return BlockParseResult::Malformed(format!(
                "unrecognized directive-shaped line: {line:?}"
            ));
        }
        // Lines not matching the shape are comments and ignored.
    }

    if !has_step {
        return BlockParseResult::Malformed("evidence block has no '$ ' step line".to_string());
    }

    BlockParseResult::Ok(steps)
}

/// Extract all ```evidence blocks from the artifact text.
/// Returns `(blocks, had_any_block)`.
pub fn extract_evidence_blocks(text: &str) -> Vec<BlockParseResult> {
    let mut results = Vec::new();
    let mut lines = text.lines().peekable();
    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        if trimmed == "```evidence" || trimmed == "~~~evidence" {
            let fence_char = if trimmed.starts_with('`') { '`' } else { '~' };
            let close_fence = if fence_char == '`' { "```" } else { "~~~" };
            let mut body = String::new();
            for inner in lines.by_ref() {
                let inner_trimmed = inner.trim();
                if inner_trimmed == close_fence {
                    break;
                }
                body.push_str(inner);
                body.push('\n');
            }
            results.push(parse_evidence_block(&body));
        }
    }
    results
}

// ── output capture with 1 MiB tail cap ───────────────────────────────────────

const OUTPUT_CAP_BYTES: usize = 1024 * 1024; // 1 MiB

/// Drain a child process's stdout and stderr into a merged line transcript.
/// Merges line-granular into one Vec<String>.  Spawns two reader threads.
/// Returns the merged lines (in approximate arrival order) and a flag for truncation.
fn drain_output(child: &mut Child) -> (Vec<String>, bool) {
    let stdout = child.stdout.take().map(BufReader::new);
    let stderr = child.stderr.take().map(BufReader::new);

    let (tx_a, rx_a) = mpsc::channel::<String>();
    let tx_b = tx_a.clone();

    let mut join_handles = Vec::new();

    if let Some(reader) = stdout {
        join_handles.push(thread::spawn(move || {
            for line in reader.lines().map_while(Result::ok) {
                let _ = tx_a.send(line);
            }
        }));
    } else {
        drop(tx_a);
    }

    if let Some(reader) = stderr {
        join_handles.push(thread::spawn(move || {
            for line in reader.lines().map_while(Result::ok) {
                let _ = tx_b.send(line);
            }
        }));
    } else {
        drop(tx_b);
    }

    let all_lines: Vec<String> = rx_a.into_iter().collect();

    for h in join_handles {
        let _ = h.join();
    }

    // Tail-cap: keep the last 1 MiB worth of output.
    let total_bytes: usize = all_lines.iter().map(|l| l.len() + 1).sum();
    let truncated = total_bytes > OUTPUT_CAP_BYTES;
    let result_lines = if truncated {
        // Drop from the front until under the cap.
        let mut lines = all_lines;
        let mut cur_bytes = total_bytes;
        while cur_bytes > OUTPUT_CAP_BYTES && !lines.is_empty() {
            let removed = lines.remove(0).len() + 1;
            cur_bytes -= removed;
        }
        lines
    } else {
        all_lines
    };
    (result_lines, truncated)
}

// ── command execution ─────────────────────────────────────────────────────────

/// Result of executing one evidence step.
#[derive(Debug)]
pub struct StepResult {
    pub command: String,
    pub passed: bool,
    pub detail: String,
}

/// Execute a single evidence step from `project_root`.
///
/// Returns `Ok(StepResult)` on clean execution (even if the step failed expectations),
/// or `Err(detail)` on a fail-closed rejection (metachar, env-assign, not-allowed, timeout).
pub fn execute_step(
    step: &EvidenceStep,
    project_root: &Path,
    cfg: &ProjectConfig,
    timeout: Duration,
) -> Result<StepResult, String> {
    let raw = &step.raw_command;

    // Fail-closed: metachars
    if has_metachar(raw) {
        return Err(format!("command rejected: metachar in {raw:?}"));
    }

    // Split into argv
    let argv: Vec<&str> = raw.split_whitespace().collect();
    if argv.is_empty() {
        return Err("empty command".to_string());
    }

    // Fail-closed: env-assignment prefix
    if is_env_assignment(argv[0]) {
        return Err(format!(
            "command rejected: env-assignment prefix in {raw:?}"
        ));
    }

    // Fail-closed: token-boundary allowlist
    if !is_command_allowed(&argv, &cfg.allowed_command_prefixes) {
        return Err(format!(
            "command rejected: not in allowed_command_prefixes: {raw:?}"
        ));
    }

    // Spawn child in process group 0 (Unix) for clean group kill on timeout.
    let child_result = spawn_child(argv[0], &argv[1..], project_root);
    let mut child = match child_result {
        Ok(c) => c,
        Err(e) => {
            return Ok(StepResult {
                command: raw.clone(),
                passed: false,
                detail: format!("failed to spawn: {e}"),
            });
        }
    };

    let child_pid = child.id();

    // Worker thread: drain output (blocking until pipes close) then wait().
    // Pipes close when the child exits, so this thread naturally finishes after the process does.
    // The main thread uses recv_timeout to enforce the wall-clock timeout.
    type WorkerResult = (Vec<String>, bool, std::io::Result<std::process::ExitStatus>);
    let (tx_worker, rx_worker) = mpsc::channel::<WorkerResult>();
    let worker_handle = thread::spawn(move || {
        let (lines, truncated) = drain_output(&mut child);
        let status = child.wait();
        let _ = tx_worker.send((lines, truncated, status));
    });

    let wait_result = rx_worker.recv_timeout(timeout);

    match wait_result {
        Err(_) => {
            // Timeout — kill the process group (Unix) or direct child (non-Unix).
            // After kill, the pipes close, the worker thread completes.
            kill_child_group(child_pid);
            let _ = worker_handle.join();
            Err(format!(
                "step timed out after {}s: {raw:?}",
                timeout.as_secs()
            ))
        }
        Ok((lines, truncated, status_result)) => {
            let _ = worker_handle.join();
            let status = match status_result {
                Ok(s) => s,
                Err(e) => {
                    return Ok(StepResult {
                        command: raw.clone(),
                        passed: false,
                        detail: format!("wait error: {e}"),
                    });
                }
            };

            if !status.success() {
                let truncation_note = if truncated {
                    " (output truncated to last 1 MiB)"
                } else {
                    ""
                };
                return Ok(StepResult {
                    command: raw.clone(),
                    passed: false,
                    detail: format!("exited {}{}", status.code().unwrap_or(-1), truncation_note),
                });
            }

            let transcript = lines.join("\n");

            // Check expect directives.
            for expect in &step.expect_literal {
                if !transcript.contains(expect.as_str()) {
                    let truncation_note = if truncated {
                        " (output truncated to last 1 MiB)"
                    } else {
                        ""
                    };
                    return Ok(StepResult {
                        command: raw.clone(),
                        passed: false,
                        detail: format!(
                            "expect literal {:?} not found in output{}",
                            expect, truncation_note
                        ),
                    });
                }
            }

            for pattern in &step.expect_regex {
                match Regex::new(&format!("(?m){pattern}")) {
                    Err(e) => {
                        return Ok(StepResult {
                            command: raw.clone(),
                            passed: false,
                            detail: format!("expect-re: bad regex {pattern:?}: {e}"),
                        });
                    }
                    Ok(re) => {
                        if !re.is_match(&transcript) {
                            let truncation_note = if truncated {
                                " (output truncated to last 1 MiB)"
                            } else {
                                ""
                            };
                            return Ok(StepResult {
                                command: raw.clone(),
                                passed: false,
                                detail: format!(
                                    "expect-re {:?} not matched{}",
                                    pattern, truncation_note
                                ),
                            });
                        }
                    }
                }
            }

            Ok(StepResult {
                command: raw.clone(),
                passed: true,
                detail: format!("ok ({} output lines)", lines.len()),
            })
        }
    }
}

// ── platform-specific child spawning and group kill ──────────────────────────

#[cfg(unix)]
fn spawn_child(program: &str, args: &[&str], cwd: &Path) -> std::io::Result<Child> {
    use std::os::unix::process::CommandExt;
    let mut cmd = Command::new(program);
    cmd.args(args)
        .current_dir(cwd)
        // D5: execution requires an *explicit* GATEKEEPER_SHADOW — a replayed command's
        // descendants (e.g. a `cargo test` whose integration tests invoke gatekeeper)
        // must not inherit the measurement trigger from this engine's own run.
        .env_remove("GATEKEEPER_SHADOW")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    cmd.spawn()
}

#[cfg(not(unix))]
fn spawn_child(program: &str, args: &[&str], cwd: &Path) -> std::io::Result<Child> {
    Command::new(program)
        .args(args)
        .current_dir(cwd)
        .env_remove("GATEKEEPER_SHADOW")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
}

/// Kill the process group on Unix by spawning `kill -9 -- -<pgid>`.
/// On non-Unix, this is a no-op (documented residual).
#[cfg(unix)]
fn kill_child_group(pid: u32) {
    // The child was spawned with process_group(0), so pgid == pid.
    let pgid_str = format!("-{pid}");
    let _ = Command::new("kill").args(["-9", "--", &pgid_str]).status();
}

#[cfg(not(unix))]
fn kill_child_group(_pid: u32) {
    // Non-Unix: no group kill — documented residual.
}

// ── static analysis (presence mode) ──────────────────────────────────────────

/// Count evidence blocks and commands for static analysis.
pub struct StaticAnalysis {
    pub block_count: usize,
    pub command_count: usize,
    pub all_allowlisted: bool,
    pub malformed: Option<String>,
}

pub fn static_analyse(text: &str, cfg: &ProjectConfig) -> StaticAnalysis {
    let blocks = extract_evidence_blocks(text);
    let block_count = blocks.len();
    let mut command_count = 0;
    let mut all_allowlisted = true;

    for block in &blocks {
        match block {
            BlockParseResult::Ok(steps) => {
                for step in steps {
                    command_count += 1;
                    let argv: Vec<&str> = step.raw_command.split_whitespace().collect();
                    if !is_command_allowed(&argv, &cfg.allowed_command_prefixes) {
                        all_allowlisted = false;
                    }
                }
            }
            BlockParseResult::Malformed(reason) => {
                return StaticAnalysis {
                    block_count,
                    command_count,
                    all_allowlisted: false,
                    malformed: Some(reason.clone()),
                };
            }
        }
    }

    StaticAnalysis {
        block_count,
        command_count,
        all_allowlisted,
        malformed: None,
    }
}

// ── legacy extraction (GATEKEEPER_SHADOW=replay, no evidence blocks) ─────────

/// Normalize a `$ `-prefixed command line from a legacy fenced block:
/// strip trailing annotation starting at ` →` or ` #`.
pub fn normalize_legacy_command(raw: &str) -> &str {
    // Strip ` → ...`
    let raw = if let Some(pos) = raw.find(" \u{2192}") {
        &raw[..pos]
    } else {
        raw
    };
    // Strip ` # ...`
    if let Some(pos) = raw.find(" #") {
        &raw[..pos]
    } else {
        raw
    }
}

/// Extract `$ `-prefixed lines from any fenced block (for legacy artifacts).
pub fn extract_legacy_commands(text: &str) -> Vec<String> {
    let mut cmds = Vec::new();
    let mut in_fence = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if !in_fence
            && (trimmed.starts_with("```") || trimmed.starts_with("~~~"))
            && !trimmed.contains("evidence")
        {
            in_fence = true;
            continue;
        }
        if in_fence && (trimmed == "```" || trimmed == "~~~") {
            in_fence = false;
            continue;
        }
        if in_fence {
            if let Some(cmd) = line.strip_prefix("$ ") {
                let normalized = normalize_legacy_command(cmd).trim().to_string();
                if !normalized.is_empty() {
                    cmds.push(normalized);
                }
            }
        }
    }
    cmds
}

// ── main gate entry points ────────────────────────────────────────────────────

/// Run `check verify` for the given artifact path.
///
/// `presence_exit_code` is the exit code from the original file-existence check
/// (0 = file exists, 1 = missing).  In replay mode this is replaced by the replay
/// result; in presence mode it is returned unchanged after emitting SHADOW lines.
pub fn run_verify_gate(artifact_path: &Path, project_root: &Path, cfg: &ProjectConfig) -> i32 {
    let shadow_env = std::env::var("GATEKEEPER_SHADOW").ok();
    let shadow_replay = shadow_env.as_deref() == Some("replay");
    let enforcing_replay = cfg.verify_mode == VerifyMode::Replay;

    let text = match std::fs::read_to_string(artifact_path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!(
                "gatekeeper: could not read {}: {e}",
                artifact_path.display()
            );
            return 1;
        }
    };

    let artifact_str = artifact_path.to_string_lossy();
    let configured = match cfg.verify_mode {
        VerifyMode::Presence => {
            if shadow_replay {
                ShadowConfigured::ShadowEnv
            } else {
                ShadowConfigured::Default
            }
        }
        VerifyMode::Replay => ShadowConfigured::On,
    };

    // Always do static analysis for the SHADOW line.
    let sa = static_analyse(&text, cfg);

    if !enforcing_replay && !shadow_replay {
        // Presence mode, no shadow env — just emit the static-analysis SHADOW line.
        let detail = if let Some(ref reason) = sa.malformed {
            format!(
                "{} evidence block(s), {} command(s); malformed: {reason}",
                sa.block_count, sa.command_count
            )
        } else {
            format!(
                "{} evidence block(s), {} command(s), all allowlisted: {}",
                sa.block_count, sa.command_count, sa.all_allowlisted
            )
        };
        emit_shadow(
            "verify",
            "replay",
            configured,
            Some(&artifact_str),
            None,
            ShadowResult::Static,
            &detail,
        );
        // Presence mode: pass as long as the file exists (we already know it does).
        println!("PASS verify gate: {}", artifact_path.display());
        return 0;
    }

    // Replay mode (enforcing or shadow-env).
    let blocks = extract_evidence_blocks(&text);

    if blocks.is_empty() {
        if enforcing_replay {
            // fail-closed: zero evidence blocks
            let detail = "zero evidence blocks — fail-closed";
            emit_shadow(
                "verify",
                "replay",
                configured,
                Some(&artifact_str),
                None,
                ShadowResult::Fail,
                detail,
            );
            println!("FAIL verify gate: {}: {}", artifact_path.display(), detail);
            return 1;
        } else {
            // shadow-env on a legacy artifact — use legacy extraction
            return run_legacy_shadow(artifact_path, &text, project_root, cfg, configured);
        }
    }

    let timeout = Duration::from_secs(cfg.replay_timeout_secs);
    let mut gate_passed = true;

    for block_result in &blocks {
        match block_result {
            BlockParseResult::Malformed(reason) => {
                let detail = format!("malformed evidence block: {reason}");
                emit_shadow(
                    "verify",
                    "replay",
                    configured,
                    Some(&artifact_str),
                    None,
                    ShadowResult::Fail,
                    &detail,
                );
                if enforcing_replay {
                    println!("FAIL verify gate: {}: {}", artifact_path.display(), detail);
                    return 1;
                }
                gate_passed = false;
            }
            BlockParseResult::Ok(steps) => {
                for step in steps {
                    match execute_step(step, project_root, cfg, timeout) {
                        Err(reject_reason) => {
                            emit_shadow(
                                "verify",
                                "replay",
                                configured,
                                Some(&artifact_str),
                                Some(&step.raw_command),
                                ShadowResult::Skip,
                                &reject_reason,
                            );
                            if enforcing_replay {
                                println!(
                                    "FAIL verify gate: {}: {}",
                                    artifact_path.display(),
                                    reject_reason
                                );
                                return 1;
                            }
                            gate_passed = false;
                        }
                        Ok(step_result) => {
                            let shadow_res = if step_result.passed {
                                ShadowResult::Pass
                            } else {
                                ShadowResult::Fail
                            };
                            emit_shadow(
                                "verify",
                                "replay",
                                configured,
                                Some(&artifact_str),
                                Some(&step_result.command),
                                shadow_res,
                                &step_result.detail,
                            );
                            if !step_result.passed {
                                if enforcing_replay {
                                    println!(
                                        "FAIL verify gate: {}: step {:?} failed: {}",
                                        artifact_path.display(),
                                        step_result.command,
                                        step_result.detail
                                    );
                                    return 1;
                                }
                                gate_passed = false;
                            }
                        }
                    }
                }
            }
        }
    }

    if enforcing_replay {
        if gate_passed {
            println!("PASS verify gate: {}", artifact_path.display());
            0
        } else {
            1
        }
    } else {
        // shadow-env mode — return presence-mode exit code (always 0 since file exists)
        println!("PASS verify gate: {}", artifact_path.display());
        0
    }
}

/// Run legacy shadow extraction for an artifact without evidence blocks.
fn run_legacy_shadow(
    artifact_path: &Path,
    text: &str,
    project_root: &Path,
    cfg: &ProjectConfig,
    configured: ShadowConfigured,
) -> i32 {
    let artifact_str = artifact_path.to_string_lossy();
    let cmds = extract_legacy_commands(text);

    if cmds.is_empty() {
        emit_shadow(
            "verify",
            "replay",
            configured,
            Some(&artifact_str),
            None,
            ShadowResult::Skip,
            "no legacy $ commands found",
        );
        println!("PASS verify gate: {}", artifact_path.display());
        return 0;
    }

    let timeout = Duration::from_secs(cfg.replay_timeout_secs);

    for cmd in &cmds {
        // Screen with metachar / allowlist checks — record skip if rejected.
        if has_metachar(cmd) {
            emit_shadow(
                "verify",
                "replay",
                configured,
                Some(&artifact_str),
                Some(cmd),
                ShadowResult::Skip,
                "legacy command has metachar",
            );
            continue;
        }

        let argv: Vec<&str> = cmd.split_whitespace().collect();
        if argv.is_empty() {
            continue;
        }

        if is_env_assignment(argv[0]) {
            emit_shadow(
                "verify",
                "replay",
                configured,
                Some(&artifact_str),
                Some(cmd),
                ShadowResult::Skip,
                "legacy command has env-assignment prefix",
            );
            continue;
        }

        if !is_command_allowed(&argv, &cfg.allowed_command_prefixes) {
            emit_shadow(
                "verify",
                "replay",
                configured,
                Some(&artifact_str),
                Some(cmd),
                ShadowResult::Skip,
                "legacy command not in allowed_command_prefixes",
            );
            continue;
        }

        // Build a pseudo-step with no expect directives and execute it.
        let step = EvidenceStep {
            raw_command: cmd.clone(),
            expect_literal: Vec::new(),
            expect_regex: Vec::new(),
        };

        match execute_step(&step, project_root, cfg, timeout) {
            Err(reject_reason) => {
                emit_shadow(
                    "verify",
                    "replay",
                    configured,
                    Some(&artifact_str),
                    Some(cmd),
                    ShadowResult::Skip,
                    &reject_reason,
                );
            }
            Ok(step_result) => {
                let shadow_res = if step_result.passed {
                    ShadowResult::Pass
                } else {
                    ShadowResult::Fail
                };
                emit_shadow(
                    "verify",
                    "replay",
                    configured,
                    Some(&artifact_str),
                    Some(cmd),
                    shadow_res,
                    &step_result.detail,
                );
            }
        }
    }

    println!("PASS verify gate: {}", artifact_path.display());
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── token-boundary allowlist matching ─────────────────────────────────────

    #[test]
    fn cargo_test_matches_longer_command() {
        let prefixes = vec!["cargo test".to_string()];
        let argv = vec!["cargo", "test", "--manifest-path", "gatekeeper/Cargo.toml"];
        assert!(is_command_allowed(&argv, &prefixes));
    }

    #[test]
    fn cargo_testfoo_rejected_token_boundary() {
        let prefixes = vec!["cargo test".to_string()];
        let argv = vec!["cargo", "testfoo"];
        assert!(
            !is_command_allowed(&argv, &prefixes),
            "cargo testfoo must not match cargo test (token boundary)"
        );
    }

    #[test]
    fn git_diff_matches() {
        let prefixes = vec!["git diff".to_string()];
        let argv = vec!["git", "diff", "--stat"];
        assert!(is_command_allowed(&argv, &prefixes));
    }

    #[test]
    fn git_push_not_allowed_by_git_diff_entry() {
        let prefixes = vec!["git diff".to_string()];
        let argv = vec!["git", "push"];
        assert!(!is_command_allowed(&argv, &prefixes));
    }

    #[test]
    fn allowlist_empty_prefix_skipped() {
        let prefixes = vec!["".to_string(), "cargo test".to_string()];
        let argv = vec!["cargo", "test"];
        assert!(is_command_allowed(&argv, &prefixes));
    }

    // ── metachar detection ────────────────────────────────────────────────────

    #[test]
    fn pipe_detected() {
        assert!(has_metachar("cargo test | grep ok"));
    }

    #[test]
    fn dollar_detected() {
        assert!(has_metachar("echo $HOME"));
    }

    #[test]
    fn clean_command_no_metachar() {
        assert!(!has_metachar(
            "cargo test --manifest-path gatekeeper/Cargo.toml"
        ));
    }

    // ── env-assignment detection ──────────────────────────────────────────────

    #[test]
    fn env_assign_detected() {
        assert!(is_env_assignment("FOO=bar"));
        assert!(is_env_assignment("GATEKEEPER_SHADOW=replay"));
    }

    #[test]
    fn non_env_assign() {
        assert!(!is_env_assignment("cargo"));
        assert!(!is_env_assignment("just"));
        assert!(!is_env_assignment("git"));
    }

    // ── evidence block parser ─────────────────────────────────────────────────

    #[test]
    fn parse_simple_step() {
        let body = "$ cargo test\n# expect: test result: ok\n";
        let result = parse_evidence_block(body);
        match result {
            BlockParseResult::Ok(steps) => {
                assert_eq!(steps.len(), 1);
                assert_eq!(steps[0].raw_command, "cargo test");
                assert_eq!(steps[0].expect_literal, vec!["test result: ok"]);
            }
            BlockParseResult::Malformed(r) => panic!("unexpected malformed: {r}"),
        }
    }

    #[test]
    fn parse_no_step_is_malformed() {
        let body = "# just a comment\n";
        let result = parse_evidence_block(body);
        assert!(matches!(result, BlockParseResult::Malformed(_)));
    }

    #[test]
    fn parse_unrecognized_directive_is_malformed() {
        let body = "$ cargo test\n# note: some note here\n";
        let result = parse_evidence_block(body);
        assert!(
            matches!(result, BlockParseResult::Malformed(_)),
            "# note: should be a malformed directive"
        );
    }

    #[test]
    fn parse_directive_before_step_is_malformed() {
        let body = "# expect: something\n$ cargo test\n";
        let result = parse_evidence_block(body);
        assert!(matches!(result, BlockParseResult::Malformed(_)));
    }

    #[test]
    fn parse_empty_expect_value_is_malformed() {
        // `# expect: ` with only whitespace would match everything — hollow assertion.
        let body = "$ cargo test\n# expect: \n";
        let result = parse_evidence_block(body);
        assert!(
            matches!(result, BlockParseResult::Malformed(_)),
            "empty '# expect:' value must be malformed"
        );
    }

    #[test]
    fn parse_empty_expect_re_value_is_malformed() {
        let body = "$ cargo test\n# expect-re:  \n";
        let result = parse_evidence_block(body);
        assert!(
            matches!(result, BlockParseResult::Malformed(_)),
            "empty '# expect-re:' value must be malformed"
        );
    }

    #[test]
    fn parse_multiple_steps() {
        let body = "$ cargo test\n$ git status\n";
        let result = parse_evidence_block(body);
        match result {
            BlockParseResult::Ok(steps) => {
                assert_eq!(steps.len(), 2);
            }
            _ => panic!("should be ok"),
        }
    }

    #[test]
    fn extract_blocks_from_text() {
        let text = "# Doc\n\n```evidence\n$ cargo test\n```\n\n```evidence\n$ git status\n```\n";
        let blocks = extract_evidence_blocks(text);
        assert_eq!(blocks.len(), 2);
    }

    #[test]
    fn extract_no_blocks() {
        let text = "# Just a doc\n\nNo evidence blocks here.\n";
        let blocks = extract_evidence_blocks(text);
        assert!(blocks.is_empty());
    }

    // ── legacy command normalization ──────────────────────────────────────────

    #[test]
    fn normalize_strips_arrow_annotation() {
        let cmd = "cargo test --manifest-path gatekeeper/Cargo.toml → exit 0";
        assert_eq!(
            normalize_legacy_command(cmd),
            "cargo test --manifest-path gatekeeper/Cargo.toml"
        );
    }

    #[test]
    fn normalize_strips_hash_annotation() {
        let cmd = "cargo test --manifest-path gatekeeper/Cargo.toml # from repo root";
        assert_eq!(
            normalize_legacy_command(cmd),
            "cargo test --manifest-path gatekeeper/Cargo.toml"
        );
    }

    #[test]
    fn normalize_no_annotation() {
        let cmd = "cargo test --manifest-path gatekeeper/Cargo.toml";
        assert_eq!(normalize_legacy_command(cmd), cmd);
    }

    // ── looks_like_directive ──────────────────────────────────────────────────

    #[test]
    fn directive_shape_detected() {
        assert!(looks_like_directive("# note: foo"));
        assert!(looks_like_directive("# expect: bar")); // known but tested
        assert!(looks_like_directive("#warn-msg: blah"));
    }

    #[test]
    fn non_directive_shape() {
        assert!(!looks_like_directive("# plain comment without colon"));
        assert!(!looks_like_directive("# "));
    }

    // ── append_line_at unit tests ─────────────────────────────────────────────

    /// append_line_at creates the parent directory and file on first call,
    /// then appends a second line in order.
    #[test]
    fn append_line_at_creates_parent_and_appends_in_order() {
        let dir = std::env::temp_dir().join(format!("topo_shadow_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let path = dir.join("sub").join("shadow.jsonl");
        append_line_at(&path, "line one");
        append_line_at(&path, "line two");

        let content = std::fs::read_to_string(&path).expect("file must exist after append");
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2, "expected 2 lines, got: {content:?}");
        assert_eq!(lines[0], "line one");
        assert_eq!(lines[1], "line two");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// append_line_at is fail-silent: calling it with a path whose parent is an
    /// existing regular file (not a directory) must not panic.
    #[test]
    fn append_line_at_impossible_path_does_not_panic() {
        let dir = std::env::temp_dir().join(format!("topo_shadow_imp_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        // Create a regular file at `dir/file` then try to use it as a directory
        // by appending to `dir/file/shadow.jsonl`.
        let blocker = dir.join("file");
        std::fs::write(&blocker, "I am a file, not a dir").unwrap();
        let impossible = blocker.join("shadow.jsonl");

        // Must not panic.
        append_line_at(&impossible, "should be silently dropped");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A file line in the shape emit_shadow assembles (leading `ts` + the same inner
    /// fields as the stderr line) round-trips through append_line_at as valid JSON with
    /// all 7 contract field names. (emit_shadow itself is exercised end-to-end by the
    /// cli_* shadow integration tests; this test pins the file-line shape only.)
    #[test]
    fn file_line_shape_has_ts_and_all_seven_fields() {
        use serde_json::Value;

        let dir = std::env::temp_dir().join(format!("topo_shadow_fields_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let path = dir.join("shadow_test.jsonl");

        // Build a file line the same way append_shadow_sink / emit_shadow would.
        // We call append_line_at directly here (no need for a full gate run).
        let sample_inner = "\"gate\":\"verify\",\"check\":\"replay\",\
            \"configured\":\"default\",\"artifact\":\"docs/verify/x.md\",\
            \"command\":null,\"result\":\"static\",\"detail\":\"0 evidence block(s)\"";
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let file_line = format!("{{\"ts\":{ts},{sample_inner}}}");

        append_line_at(&path, &file_line);

        let content = std::fs::read_to_string(&path).unwrap();
        let line = content.lines().next().expect("file must have one line");

        // Must start with `{"ts":`
        assert!(
            line.starts_with("{\"ts\":"),
            "file line must start with {{\"ts\":, got: {line:?}"
        );

        // Parse as JSON and verify all 7 SHADOW contract fields are present.
        let v: Value = serde_json::from_str(line).expect("file line must be valid JSON");
        let obj = v.as_object().expect("must be a JSON object");
        let required = &[
            "gate",
            "check",
            "configured",
            "artifact",
            "command",
            "result",
            "detail",
        ];
        for field in required {
            assert!(
                obj.contains_key(*field),
                "missing field {field:?} in file line: {line:?}"
            );
        }
        // ts field must be present and numeric.
        assert!(
            obj.get("ts").and_then(|v| v.as_u64()).is_some(),
            "ts must be a non-negative integer"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
