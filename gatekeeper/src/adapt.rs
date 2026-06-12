//! Cross-harness adapters — generate native config for Codex, Cursor, OpenCode (and Claude) from the
//! one Markdown source (`AGENTS.md` + `skills/` + `instincts/`). Outputs are build artifacts, never
//! hand-edited (see docs/adr/0003 and docs/adr/0008). std + the existing `serde_json` only; no new
//! crates. Generation is pure (`root -> Vec<GenFile>`); all I/O is funneled through `apply_or_check`,
//! whose `--check` mode re-renders and diffs against disk (the idempotency gate).

use std::fs;
use std::path::{Path, PathBuf};

use crate::instinct;

/// The outcome of a partial-file edit (user-owned files).
#[derive(Debug)]
pub(crate) enum Edit {
    /// The file already contained the desired content — no write needed.
    Unchanged,
    /// The file was missing; the new content is provided.
    Created(String),
    /// The file existed and was updated; the new content is provided.
    Updated(String),
    /// The file is malformed in a way that makes safe editing impossible; the message names the problem.
    Failed(String),
}

/// Ensure `line` is present (exact match after trimming) in `existing`.
///
/// - `None` → `Created(line + "\n")`.
/// - `line` found in any row (trimmed) → `Unchanged`.
/// - Otherwise → `Updated` with `line` appended on its own line, single trailing newline.
fn ensure_import_line(existing: Option<&str>, line: &str) -> Edit {
    let line_trimmed = line.trim();
    match existing {
        None => Edit::Created(format!("{line}\n")),
        Some(text) => {
            if text.lines().any(|l| l.trim() == line_trimmed) {
                Edit::Unchanged
            } else {
                // Append on its own line, ensuring exactly one trailing newline.
                let mut out = text.to_owned();
                // Strip any trailing newlines, then add separator + line + newline.
                let trimmed_end = out.trim_end_matches('\n');
                let len = trimmed_end.len();
                out.truncate(len);
                out.push('\n');
                out.push_str(line);
                out.push('\n');
                Edit::Updated(out)
            }
        }
    }
}

const BLOCK_BEGIN: &str = "<!-- BEGIN TOPOLOGY MANAGED BLOCK -->";
const BLOCK_END: &str = "<!-- END TOPOLOGY MANAGED BLOCK -->";

/// Ensure the marker-delimited managed block contains `body`.
///
/// - No block → append wrapped body (create file if missing).
/// - Block present, identical body → `Unchanged`.
/// - Block present, different body → replace in place, outside content preserved.
/// - Malformed (begin without end, or duplicate begin) → `Failed` naming the problem.
fn ensure_managed_block(existing: Option<&str>, body: &str) -> Edit {
    let wrapped = format!("{BLOCK_BEGIN}\n{body}\n{BLOCK_END}\n");

    match existing {
        None => Edit::Created(wrapped),
        Some(text) => {
            let begin_count = text.matches(BLOCK_BEGIN).count();
            let end_count = text.matches(BLOCK_END).count();

            if begin_count > 1 {
                return Edit::Failed(format!(
                    "duplicate '{BLOCK_BEGIN}' markers — cannot safely update"
                ));
            }
            if begin_count == 1 && end_count == 0 {
                return Edit::Failed(format!(
                    "'{BLOCK_BEGIN}' without '{BLOCK_END}' — malformed block"
                ));
            }
            if begin_count == 0 {
                // No block; append to existing content.
                let mut out = text.to_owned();
                let trimmed_end = out.trim_end_matches('\n');
                let len = trimmed_end.len();
                out.truncate(len);
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(&wrapped);
                return Edit::Updated(out);
            }

            // Block is present and well-formed (1 begin, ≥1 end).
            let begin_pos = text.find(BLOCK_BEGIN).unwrap();
            // End marker search starts after begin marker.
            let end_search_start = begin_pos + BLOCK_BEGIN.len();
            let end_rel = text[end_search_start..].find(BLOCK_END).unwrap();
            let end_pos = end_search_start + end_rel;
            let end_end = end_pos + BLOCK_END.len();

            // Extract the current body (between begin+\n and \n+end).
            let inner_start = begin_pos + BLOCK_BEGIN.len();
            // Skip the leading newline after begin marker.
            let inner_start = if text[inner_start..].starts_with('\n') {
                inner_start + 1
            } else {
                inner_start
            };
            // Body ends before the end marker; trim one leading newline before end.
            let inner_end = if end_pos > 0 && text.as_bytes()[end_pos - 1] == b'\n' {
                end_pos - 1
            } else {
                end_pos
            };
            let current_body = &text[inner_start..inner_end];

            if current_body == body {
                return Edit::Unchanged;
            }

            // Replace the block in place.
            let before = &text[..begin_pos];
            // After the end marker, consume one trailing newline if present.
            let after_start = if end_end < text.len() && text.as_bytes()[end_end] == b'\n' {
                end_end + 1
            } else {
                end_end
            };
            let after = &text[after_start..];

            let out = format!("{before}{wrapped}{after}");
            Edit::Updated(out)
        }
    }
}

/// Merge hook wiring and `GATEKEEPER_BIN` into an existing (or absent) settings JSON.
///
/// - `existing = None` or `null` → start from `{}`.
/// - Non-object existing → `Err("not a JSON object")`.
/// - Sets `obj["hooks"] = hooks` (adapt-owned, replaced wholesale).
/// - Ensures `obj["env"]` is an object; sets `obj["env"]["GATEKEEPER_BIN"] = bin`.
/// - All other top-level keys and all other `env` keys are preserved.
pub(crate) fn merge_claude_settings(
    existing: Option<serde_json::Value>,
    hooks: serde_json::Value,
    bin: &str,
) -> Result<serde_json::Value, String> {
    let mut obj = match existing {
        None | Some(serde_json::Value::Null) => serde_json::Map::new(),
        Some(serde_json::Value::Object(m)) => m,
        Some(_) => return Err("not a JSON object".to_owned()),
    };

    obj.insert("hooks".to_owned(), hooks);

    let env = obj
        .entry("env")
        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
    if let serde_json::Value::Object(env_map) = env {
        env_map.insert(
            "GATEKEEPER_BIN".to_owned(),
            serde_json::Value::String(bin.to_owned()),
        );
    } else {
        // env exists but is not an object; replace it.
        let mut env_map = serde_json::Map::new();
        env_map.insert(
            "GATEKEEPER_BIN".to_owned(),
            serde_json::Value::String(bin.to_owned()),
        );
        obj.insert("env".to_owned(), serde_json::Value::Object(env_map));
    }

    Ok(serde_json::Value::Object(obj))
}

/// Apply or check partial-file edits (for user-owned files).
///
/// Write mode: applies Created/Updated, prints what changed.
/// Check mode: exit 1 on any would-change, exit 2 on any Failed; never writes.
///
/// Returns exit code: 0 = up-to-date; 1 = drift; 2 = failed.
fn apply_edits(edits: &[(String, Edit)], check: bool) -> i32 {
    let mut drift = false;
    let mut failed = false;

    for (path, edit) in edits {
        match edit {
            Edit::Unchanged => {}
            Edit::Created(contents) => {
                if check {
                    println!("MISSING-IMPORT {path}");
                    drift = true;
                } else {
                    if let Some(parent) = std::path::Path::new(path).parent() {
                        if !parent.as_os_str().is_empty() {
                            if let Err(e) = fs::create_dir_all(parent) {
                                eprintln!(
                                    "gatekeeper adapt: cannot create {}: {e}",
                                    parent.display()
                                );
                                return 2;
                            }
                        }
                    }
                    if let Err(e) = fs::write(path, contents) {
                        eprintln!("gatekeeper adapt: cannot write {path}: {e}");
                        return 2;
                    }
                    println!("wrote {path}");
                }
            }
            Edit::Updated(contents) => {
                if check {
                    println!("DRIFT-BLOCK {path}");
                    drift = true;
                } else {
                    if let Some(parent) = std::path::Path::new(path).parent() {
                        if !parent.as_os_str().is_empty() {
                            if let Err(e) = fs::create_dir_all(parent) {
                                eprintln!(
                                    "gatekeeper adapt: cannot create {}: {e}",
                                    parent.display()
                                );
                                return 2;
                            }
                        }
                    }
                    if let Err(e) = fs::write(path, contents) {
                        eprintln!("gatekeeper adapt: cannot write {path}: {e}");
                        return 2;
                    }
                    println!("updated {path}");
                }
            }
            Edit::Failed(msg) => {
                eprintln!("gatekeeper adapt: {msg} in {path}");
                failed = true;
            }
        }
    }

    if failed {
        2
    } else if drift {
        1
    } else {
        0
    }
}

/// A single file an adapter will write, relative to the framework root.
struct GenFile {
    rel_path: PathBuf,
    contents: String,
}

impl GenFile {
    fn new(rel: impl Into<PathBuf>, contents: String) -> GenFile {
        GenFile {
            rel_path: rel.into(),
            contents,
        }
    }
}

/// The one project-safe `.codex/config.toml`: raise `project_doc_max_bytes` so the full `AGENTS.md`
/// contract is ingested; set nothing else (the user's `~/.codex/config.toml` wins). Validated against
/// `codex --strict-config` (see the verify note). The word "profile" appears only in this comment —
/// no denylisted key is set.
const CODEX_CONFIG_TOML: &str = "# Generated by `gatekeeper adapt --harness codex` — do not edit (regenerate to update).
# Topology's operating contract (the gate sequence + conduct rules) lives in AGENTS.md, which Codex
# auto-discovers as project instructions for this repository. project_doc_max_bytes is raised so the
# full contract is ingested as AGENTS.md grows. No model/sandbox/approval defaults are set here, so
# your ~/.codex/config.toml preferences win; project-local config may not carry credential, provider,
# or profile keys (Codex strips them).
project_doc_max_bytes = 1048576
";

/// Write every file (creating parent dirs), or with `check=true` diff against disk without writing.
/// Returns the exit code: 0 = wrote / up-to-date; 1 = drift in check mode; 2 = write error.
fn apply_or_check(files: &[GenFile], root: &Path, check: bool) -> i32 {
    if check {
        let mut drift = 0usize;
        for f in files {
            let path = root.join(&f.rel_path);
            match fs::read_to_string(&path) {
                Ok(disk) if disk == f.contents => {}
                Ok(_) => {
                    println!("DRIFT {}", f.rel_path.display());
                    drift += 1;
                }
                Err(_) => {
                    println!("MISSING {}", f.rel_path.display());
                    drift += 1;
                }
            }
        }
        if drift == 0 {
            println!("up to date ({} file(s))", files.len());
            0
        } else {
            println!("{drift} file(s) would change — run without --check to regenerate");
            1
        }
    } else {
        for f in files {
            let path = root.join(&f.rel_path);
            if let Some(parent) = path.parent() {
                if let Err(e) = fs::create_dir_all(parent) {
                    eprintln!("gatekeeper adapt: cannot create {}: {e}", parent.display());
                    return 2;
                }
            }
            if let Err(e) = fs::write(&path, &f.contents) {
                eprintln!("gatekeeper adapt: cannot write {}: {e}", path.display());
                return 2;
            }
            println!("wrote {}", f.rel_path.display());
        }
        0
    }
}

/// A skill parsed from `skills/<dir>/SKILL.md`. `name`/`description` come from the frontmatter (so the
/// canonical id is `name`, not the dir — `_getting-started` -> `getting-started`); `body` is the
/// Markdown after the frontmatter; `raw` is the whole file (for verbatim copy into Agent-Skills harnesses).
struct Skill {
    name: String,
    description: String,
    body: String,
    raw: String,
}

/// Parse a `SKILL.md`. Returns `None` if unreadable or missing a `name`.
fn read_skill(skill_md: &Path) -> Option<Skill> {
    let raw = fs::read_to_string(skill_md).ok()?;
    let text = raw.replace("\r\n", "\n");
    let after_open = text.strip_prefix("---\n")?;
    let mut name: Option<String> = None;
    let mut description = String::new();
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
        if let Some(v) = trimmed.strip_prefix("name:") {
            name = Some(v.trim().to_string());
        } else if let Some(v) = trimmed.strip_prefix("description:") {
            description = v.trim().to_string();
        }
    }
    let body_offset = body_offset?;
    let body = after_open[body_offset..].trim().to_string();
    Some(Skill {
        name: name?,
        description,
        body,
        raw,
    })
}

/// Every parseable skill under `root/skills/`, sorted by frontmatter `name`.
fn load_skills(root: &Path) -> Vec<Skill> {
    let dir = root.join("skills");
    let mut paths: Vec<PathBuf> = match fs::read_dir(&dir) {
        Ok(rd) => rd.filter_map(|e| e.ok().map(|e| e.path())).collect(),
        Err(_) => return Vec::new(),
    };
    paths.sort();
    let mut out: Vec<Skill> = paths
        .into_iter()
        .filter(|p| p.is_dir())
        .filter_map(|p| read_skill(&p.join("SKILL.md")))
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Read `AGENTS.md` — the contract every harness needs. Its absence is a hard error.
fn require_agents_md(root: &Path) -> Result<String, String> {
    let path = root.join("AGENTS.md");
    fs::read_to_string(&path).map_err(|_| {
        format!(
            "AGENTS.md not found at {} — it carries the contract every harness needs",
            path.display()
        )
    })
}

/// One Markdown bullet list of the always-on instincts, under `heading`.
fn instincts_markdown(heading: &str, instincts: &[(String, String)]) -> String {
    let mut s = format!("{heading}\n\nReasoning framing that applies to every change:\n\n");
    for (id, why) in instincts {
        s.push_str(&format!("- **{id}** — {why}\n"));
    }
    s
}

/// Quote a YAML scalar only when it could be misparsed unquoted (contains `:`, `"`, `#`, or a newline).
fn yaml_inline(s: &str) -> String {
    if s.contains(':') || s.contains('"') || s.contains('#') || s.contains('\n') {
        let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
        format!("\"{escaped}\"")
    } else {
        s.to_string()
    }
}

/// One Cursor `.mdc` rule: frontmatter (`description`, `alwaysApply`) then the body. `globs` is
/// deliberately omitted — Always rules don't need it and Agent-Requested rules must not set it.
fn mdc(description: &str, always: bool, body: &str) -> String {
    format!(
        "---\ndescription: {}\nalwaysApply: {}\n---\n{}\n",
        yaml_inline(description),
        always,
        body
    )
}

// ---- per-harness builders -------------------------------------------------------------------------

/// Codex: a project-safe `.codex/config.toml`; the contract rides on the auto-discovered `AGENTS.md`.
fn build_codex(root: &Path) -> Result<Vec<GenFile>, String> {
    require_agents_md(root)?;
    Ok(vec![GenFile::new(
        ".codex/config.toml",
        CODEX_CONFIG_TOML.to_string(),
    )])
}

/// Cursor: Always rules for the contract + the instincts; an Agent-Requested rule per skill (keyword
/// routing has no Cursor primitive, so skills select on their description).
fn build_cursor(root: &Path) -> Result<Vec<GenFile>, String> {
    let agents = require_agents_md(root)?;
    let instincts = instinct::instincts_for_adapt(root)?;
    let skills = load_skills(root);

    let mut files = Vec::new();
    files.push(GenFile::new(
        ".cursor/rules/agents-contract.mdc",
        mdc(
            "Topology operating contract — the gate sequence and moment-to-moment conduct rules.",
            true,
            agents.trim(),
        ),
    ));
    files.push(GenFile::new(
        ".cursor/rules/instincts.mdc",
        mdc(
            "Topology always-on instincts — reasoning guardrails that apply to every change.",
            true,
            instincts_markdown("# Always-on instincts", &instincts).trim(),
        ),
    ));
    for s in &skills {
        files.push(GenFile::new(
            format!(".cursor/rules/skill-{}.mdc", s.name),
            mdc(&s.description, false, &s.body),
        ));
    }
    Ok(files)
}

/// OpenCode: `opencode.json` pointing `instructions` at the contract + rendered instincts, plus the
/// skills copied verbatim into `.opencode/skills/` (Anthropic Agent Skills format).
fn build_opencode(root: &Path) -> Result<Vec<GenFile>, String> {
    require_agents_md(root)?;
    let instincts = instinct::instincts_for_adapt(root)?;
    let skills = load_skills(root);

    let mut files = Vec::new();

    let config = serde_json::json!({
        "$schema": "https://opencode.ai/config.json",
        "instructions": ["AGENTS.md", ".opencode/instincts.md"],
    });
    let mut json = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
    json.push('\n');
    files.push(GenFile::new("opencode.json", json));

    files.push(GenFile::new(
        ".opencode/instincts.md",
        instincts_markdown("# Topology always-on instincts", &instincts),
    ));

    for s in &skills {
        files.push(GenFile::new(
            format!(".opencode/skills/{}/SKILL.md", s.name),
            s.raw.clone(),
        ));
    }
    Ok(files)
}

/// Build the hooks JSON value for the Claude harness. Hook command paths are rooted at
/// `framework_root` (where the hooks actually live).
fn build_claude_hooks(framework_root: &Path) -> Result<serde_json::Value, String> {
    require_agents_md(framework_root)?;
    let root = framework_root;
    let skill_activation = root.join("hooks/skill-activation.sh").display().to_string();
    let security_scan = root.join("hooks/security-scan.sh").display().to_string();
    Ok(serde_json::json!({
        "UserPromptSubmit": [
            { "hooks": [ { "type": "command", "command": skill_activation, "timeout": 30 } ] }
        ],
        "PreToolUse": [
            {
                "matcher": "Bash|Write|Edit|MultiEdit",
                "hooks": [ { "type": "command", "command": security_scan, "timeout": 30 } ]
            }
        ]
    }))
}

/// Claude: no adapt-owned whole files to write (settings.json is merged in cmd_adapt).
/// Returns an empty list; the AGENTS.md check is done by build_claude_hooks.
fn build_claude(framework_root: &Path) -> Result<Vec<GenFile>, String> {
    build_claude_hooks(framework_root)?;
    Ok(Vec::new())
}

/// Detect the likely default integration branch from git at `repo_root`, used when generating
/// `config.toml` during `gatekeeper adapt`.
///
/// Strategy (mirrors the review gate):
///
///   1. `git symbolic-ref refs/remotes/origin/HEAD` → strip the prefix.
///   2. Whichever of "main" / "master" exists as a local branch (skip if both exist).
///
/// Falls back to `"main"` when inconclusive.
fn detect_base_branch_for_config(repo_root: &Path) -> String {
    // Strategy 1: origin/HEAD.
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["symbolic-ref", "refs/remotes/origin/HEAD"])
        .output();
    if let Ok(o) = out {
        if o.status.success() {
            let sym = String::from_utf8_lossy(&o.stdout).trim().to_string();
            let prefix = "refs/remotes/origin/";
            if let Some(branch) = sym.strip_prefix(prefix) {
                if !branch.is_empty() {
                    return branch.to_owned();
                }
            }
        }
    }

    // Strategy 2: local branch existence.
    let has = |branch: &str| -> bool {
        std::process::Command::new("git")
            .arg("-C")
            .arg(repo_root)
            .args(["rev-parse", "--verify", branch])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    };
    let has_main = has("main");
    let has_master = has("master");
    match (has_main, has_master) {
        (true, false) => "main".to_owned(),
        (false, true) => "master".to_owned(),
        _ => "main".to_owned(), // both or neither: default to main
    }
}

/// Generate `<artifacts_root>/config.toml` with `base_branch` detected from git, and
/// `test_command` commented out. Never overwrites an existing file.
///
/// Returns a `GenFile` with a path relative to `write_root`.
fn build_project_config(write_root: &Path) -> Option<GenFile> {
    let arts_rel = PathBuf::from(".claude").join("topology");
    let config_path = arts_rel.join("config.toml");

    // Never overwrite.
    if write_root.join(&config_path).exists() {
        return None;
    }

    let base = detect_base_branch_for_config(write_root);
    let contents = format!(
        "# Topology per-project config — generated by `gatekeeper adapt`.\n\
         # All keys are optional. Re-run adapt to regenerate (will not overwrite).\n\
         base_branch = \"{base}\"\n\
         # test_command = \"npm test\"   \
         # uncomment and set; `gatekeeper check finish` runs this when no -- <cmd> is given\n"
    );
    Some(GenFile::new(config_path, contents))
}

// ── contract render ────────────────────────────────────────────────────────────

/// Context for rendering `CONTRACT.template.md` into a world-specific contract.
pub(crate) struct ContractCtx {
    /// The artifact-path prefix: `docs` (framework) or `.claude/topology` (governed project).
    pub(crate) artifacts_root: String,
    /// Wiring note appended at the end: empty for the framework, one sentence for governed
    /// projects (explaining that `gatekeeper` is wired via `GATEKEEPER_BIN` in `.claude/settings.json`).
    pub(crate) binary_note: String,
}

/// The three known template placeholders (order matters for fail-closed check: substituted before
/// scanning for residual `{{`).
const KNOWN_PLACEHOLDERS: &[&str] = &[
    "{{ARTIFACTS_ROOT}}",
    "{{GATEKEEPER_CMD}}",
    "{{BINARY_NOTE}}",
];

/// The gatekeeper invocation word used in both contexts (the binary is always on PATH in the
/// framework; governed projects will wire it via GATEKEEPER_BIN in Phase 9).
const GATEKEEPER_CMD: &str = "gatekeeper";

/// Trailer appended to the framework render (after the template body).
/// Contains the pointer to the dev doc that is NOT part of the portable contract.
const FRAMEWORK_TRAILER: &str = "\
## Framework development

Stack conventions and the skill house format live in `docs/DEVELOPMENT.md` — read it before changing this repo.
";

/// Pure render function — no I/O, fully unit-testable.
///
/// Substitutes the three known placeholders into `template`. After substitution any remaining
/// `{{` is a hard error (fail-closed): returns `Err` naming the first offending placeholder.
///
/// Unknown placeholders in the template (i.e. `{{FOO}}` where FOO is not in `KNOWN_PLACEHOLDERS`)
/// are caught via the residual-`{{` check — after the known substitutions any `{{...}}` left is
/// necessarily unknown, and the error message names it.
pub(crate) fn render_contract(template: &str, ctx: &ContractCtx) -> Result<String, String> {
    let mut out = template.to_owned();
    out = out.replace("{{ARTIFACTS_ROOT}}", &ctx.artifacts_root);
    out = out.replace("{{GATEKEEPER_CMD}}", GATEKEEPER_CMD);
    out = out.replace("{{BINARY_NOTE}}", &ctx.binary_note);

    // Fail-closed: any remaining `{{` is an unresolved (unknown) placeholder.
    if let Some(pos) = out.find("{{") {
        // Extract the placeholder token for a useful error message.
        let rest = &out[pos..];
        let end = rest.find("}}").unwrap_or(rest.len().min(40));
        let token = &rest[..end + if end < rest.len() { 2 } else { 0 }];
        return Err(format!(
            "unresolved placeholder '{token}' — only {:?} are substituted",
            KNOWN_PLACEHOLDERS
        ));
    }

    Ok(out)
}

/// Build a `ContractCtx` for the framework world: artifacts live at `docs/`, no binary note.
fn framework_ctx() -> ContractCtx {
    ContractCtx {
        artifacts_root: "docs".to_owned(),
        binary_note: String::new(),
    }
}

/// Wiring sentence for the governed-project render (spec §1): the binary resolves through
/// `GATEKEEPER_BIN`, never an absolute path baked into the contract. The wiring itself
/// (`.claude/settings.json` env block) is created by Phase 9's integration.
const PROJECT_BINARY_NOTE: &str = "In governed projects, `gatekeeper` resolves through the \
`GATEKEEPER_BIN` environment variable wired in `.claude/settings.json` — no PATH \
installation is needed.\n";

/// Build a `ContractCtx` for a governed-project world: artifacts at `.claude/topology/`.
fn project_ctx() -> ContractCtx {
    ContractCtx {
        artifacts_root: ".claude/topology".to_owned(),
        binary_note: PROJECT_BINARY_NOTE.to_owned(),
    }
}

/// Render the contract for the given world, printing to stdout (exit 0) or error to stderr (exit 2).
/// `read_root` is the framework root where `templates/CONTRACT.template.md` lives.
fn cmd_adapt_contract(world: &str, read_root: &Path) -> i32 {
    let template_path = read_root.join("templates").join("CONTRACT.template.md");
    let template = match fs::read_to_string(&template_path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!(
                "gatekeeper adapt --contract: cannot read {}: {e}",
                template_path.display()
            );
            return 2;
        }
    };

    let ctx = match world {
        "framework" => framework_ctx(),
        "project" => project_ctx(),
        other => {
            eprintln!(
                "gatekeeper adapt --contract: unknown world '{other}' (expected framework|project)"
            );
            return 2;
        }
    };

    let rendered = match render_contract(&template, &ctx) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("gatekeeper adapt --contract {world}: render error: {e}");
            return 2;
        }
    };

    // For framework world, append the dev-doc trailer.
    let output = if world == "framework" {
        format!("{rendered}{FRAMEWORK_TRAILER}")
    } else {
        rendered
    };

    print!("{output}");
    0
}

/// Entry point for `gatekeeper adapt ...`. Returns the process exit code (0 / 1 / 2).
///
/// - `read_root`: the framework root — skills, instincts, and AGENTS.md are read from here;
///   hook command paths in the generated config point here.
/// - `write_root`: the project root — generated files are written relative to this directory.
///   When `read_root == write_root` (in-framework use) the behavior is identical to v1.
pub fn cmd_adapt(args: &[String], read_root: &Path, write_root: &Path) -> i32 {
    if let Some(code) = crate::check_help_or_unknown(
        "adapt",
        args,
        &["--harness", "--check", "--contract"],
        crate::lookup_usage("adapt"),
    ) {
        return code;
    }
    let mut harness: Option<String> = None;
    let mut contract_world: Option<String> = None;
    let mut check = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--harness" => match args.get(i + 1) {
                Some(h) => {
                    harness = Some(h.clone());
                    i += 2;
                }
                None => {
                    eprintln!("gatekeeper adapt: --harness needs a value");
                    return 2;
                }
            },
            "--contract" => match args.get(i + 1) {
                Some(w) => {
                    contract_world = Some(w.clone());
                    i += 2;
                }
                None => {
                    eprintln!("gatekeeper adapt: --contract needs a value (framework|project)");
                    return 2;
                }
            },
            "--check" => {
                check = true;
                i += 1;
            }
            // Unknown flags already rejected by check_help_or_unknown above.
            _ => {
                i += 1;
            }
        }
    }

    // --contract path: print render to stdout, exit 0 / 2 only.
    if let Some(world) = contract_world {
        return cmd_adapt_contract(&world, read_root);
    }

    let Some(harness) = harness else {
        eprintln!("gatekeeper adapt: --harness <codex|cursor|opencode|claude> or --contract <framework|project> is required");
        return 2;
    };
    // Build generates paths relative to write_root; hook paths embedded in config point at
    // read_root (the framework, where hooks/skill-activation.sh actually lives).
    let built = match harness.as_str() {
        "codex" => build_codex(read_root),
        "cursor" => build_cursor(read_root),
        "opencode" => build_opencode(read_root),
        "claude" => build_claude(read_root),
        other => {
            eprintln!(
                "gatekeeper adapt: unknown harness '{other}' (expected codex|cursor|opencode|claude)"
            );
            return 2;
        }
    };
    match built {
        Ok(mut files) => {
            // Canonicalize for comparison when both paths are on disk; fall back to plain equality.
            let roots_differ = match (
                std::fs::canonicalize(read_root),
                std::fs::canonicalize(write_root),
            ) {
                (Ok(r), Ok(w)) => r != w,
                _ => read_root != write_root,
            };

            // For the claude harness: merge settings.json rather than emitting whole-file.
            if harness == "claude" {
                let hooks = match build_claude_hooks(read_root) {
                    Ok(h) => h,
                    Err(e) => {
                        eprintln!("gatekeeper adapt claude: {e}");
                        return 2;
                    }
                };
                let bin = read_root
                    .join("bin")
                    .join("gatekeeper")
                    .display()
                    .to_string();
                let settings_path = write_root.join(".claude").join("settings.json");

                // Read existing settings (if any).
                let existing: Option<serde_json::Value> = if settings_path.exists() {
                    match fs::read_to_string(&settings_path) {
                        Ok(s) => match serde_json::from_str(&s) {
                            Ok(v) => Some(v),
                            Err(e) => {
                                eprintln!(
                                    "gatekeeper adapt: cannot parse {}: {e}",
                                    settings_path.display()
                                );
                                return 2;
                            }
                        },
                        Err(e) => {
                            eprintln!(
                                "gatekeeper adapt: cannot read {}: {e}",
                                settings_path.display()
                            );
                            return 2;
                        }
                    }
                } else {
                    None
                };

                let merged = match merge_claude_settings(existing.clone(), hooks.clone(), &bin) {
                    Ok(v) => v,
                    Err(e) => {
                        eprintln!("gatekeeper adapt: {e}");
                        return 2;
                    }
                };
                let mut merged_str =
                    serde_json::to_string_pretty(&merged).expect("serialization cannot fail");
                merged_str.push('\n');

                if check {
                    // --check: compare only hooks and env.GATEKEEPER_BIN (user keys not drift).
                    let disk_ok = existing
                        .as_ref()
                        .and_then(|v| v.as_object())
                        .map(|obj| {
                            obj.get("hooks") == Some(&hooks)
                                && obj
                                    .get("env")
                                    .and_then(|e| e.get("GATEKEEPER_BIN"))
                                    .and_then(|b| b.as_str())
                                    == Some(bin.as_str())
                        })
                        .unwrap_or(false);
                    if !disk_ok {
                        println!("DRIFT .claude/settings.json");
                        // continue to collect other drift — but we'll propagate the code below
                    }
                    // Apply remaining whole-file checks.
                    if roots_differ {
                        if let Some(cfg_file) = build_project_config(write_root) {
                            files.push(cfg_file);
                        }
                    }
                    let other_code = apply_or_check(&files, write_root, true);
                    if !disk_ok {
                        return if other_code == 2 { 2 } else { 1 };
                    }
                    return other_code;
                } else {
                    // Write mode: apply merge.
                    if let Some(parent) = settings_path.parent() {
                        if let Err(e) = fs::create_dir_all(parent) {
                            eprintln!("gatekeeper adapt: cannot create {}: {e}", parent.display());
                            return 2;
                        }
                    }
                    if let Err(e) = fs::write(&settings_path, &merged_str) {
                        eprintln!(
                            "gatekeeper adapt: cannot write {}: {e}",
                            settings_path.display()
                        );
                        return 2;
                    }
                    println!("wrote .claude/settings.json");
                }
            }

            // For project installs (read_root != write_root), also generate config.toml
            // at <artifacts_root>/config.toml if it doesn't already exist.
            if roots_differ {
                if let Some(cfg_file) = build_project_config(write_root) {
                    files.push(cfg_file);
                }

                // Scaffold the five artifact subdirectories with .gitkeep files.
                for subdir in &["research", "specs", "plans", "verify", "reviews"] {
                    files.push(GenFile::new(
                        format!(".claude/topology/{subdir}/.gitkeep"),
                        String::new(),
                    ));
                }

                // Render and deliver the project contract (whole-file GenFile).
                let template_path = read_root.join("templates").join("CONTRACT.template.md");
                match fs::read_to_string(&template_path) {
                    Ok(template) => match render_contract(&template, &project_ctx()) {
                        Ok(rendered) => {
                            files.push(GenFile::new(".topology/CONTRACT.md", rendered));
                        }
                        Err(e) => {
                            eprintln!("gatekeeper adapt: contract render error: {e}");
                            return 2;
                        }
                    },
                    Err(e) => {
                        eprintln!(
                            "gatekeeper adapt: cannot read {}: {e}",
                            template_path.display()
                        );
                        return 2;
                    }
                }

                // Deliver the contract pointer into the harness-native surface.
                // claude: append @.topology/CONTRACT.md import to CLAUDE.md.
                // codex: upsert managed block in AGENTS.md.
                // cursor/opencode: unchanged from v1 (Phase 9.1).
                match harness.as_str() {
                    "claude" => {
                        let claude_md_path = write_root.join("CLAUDE.md");
                        let existing = fs::read_to_string(&claude_md_path).ok();
                        let edit =
                            ensure_import_line(existing.as_deref(), "@.topology/CONTRACT.md");
                        let edits = vec![(claude_md_path.to_string_lossy().into_owned(), edit)];
                        let code = apply_edits(&edits, check);
                        if code != 0 {
                            // Exit early on error; on drift (code=1) still report and return.
                            let files_code = apply_or_check(&files, write_root, check);
                            return if code == 2 || files_code == 2 { 2 } else { 1 };
                        }
                    }
                    "codex" => {
                        let agents_md_path = write_root.join("AGENTS.md");
                        let existing = fs::read_to_string(&agents_md_path).ok();
                        const CODEX_BLOCK_BODY: &str =
                            "See `.topology/CONTRACT.md` for the Topology operating contract (gate sequence, conduct rules).";
                        let edit = ensure_managed_block(existing.as_deref(), CODEX_BLOCK_BODY);
                        let edits = vec![(agents_md_path.to_string_lossy().into_owned(), edit)];
                        let code = apply_edits(&edits, check);
                        if code != 0 {
                            let files_code = apply_or_check(&files, write_root, check);
                            return if code == 2 || files_code == 2 { 2 } else { 1 };
                        }
                    }
                    _ => {}
                }
            }
            apply_or_check(&files, write_root, check)
        }
        Err(e) => {
            eprintln!("gatekeeper adapt {harness}: {e}");
            2
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("topo_adapt_{tag}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("skills/brainstorm-design")).unwrap();
        fs::create_dir_all(root.join("skills/_getting-started")).unwrap();
        fs::create_dir_all(root.join("instincts")).unwrap();
        fs::write(
            root.join("AGENTS.md"),
            "# Topology Agent\n\nGate sequence: design then plan then tdd.\n",
        )
        .unwrap();
        fs::write(
            root.join("skills/brainstorm-design/SKILL.md"),
            "---\nname: brainstorm-design\ndescription: Turn an idea into a design. Use when starting a feature.\n---\n# Brainstorm\n\nNo code before a design doc.\n",
        )
        .unwrap();
        fs::write(
            root.join("skills/_getting-started/SKILL.md"),
            "---\nname: getting-started\ndescription: Bootstrap the methodology. Use at the start of a task.\n---\n# Getting started\n\nPick the gate.\n",
        )
        .unwrap();
        fs::write(
            root.join("instincts/gates-not-rules.md"),
            "---\nid: gates-not-rules\npriority: high\n---\nPhrase a commitment as trigger then check then act.\n",
        )
        .unwrap();
        fs::write(
            root.join("instincts/surgical-changes-only.md"),
            "---\nid: surgical-changes-only\npriority: medium\n---\nChange only what the task needs.\n",
        )
        .unwrap();
        root
    }

    fn find<'a>(files: &'a [GenFile], rel: &str) -> &'a GenFile {
        files
            .iter()
            .find(|f| f.rel_path.to_string_lossy() == rel)
            .unwrap_or_else(|| panic!("missing generated file {rel}"))
    }

    #[test]
    fn codex_sets_only_the_validated_key() {
        let root = fixture("codex");
        let files = build_codex(&root).unwrap();
        let cfg = find(&files, ".codex/config.toml");
        // The only non-comment assignment line is the validated key — no denylisted key is set.
        let assignments: Vec<&str> = cfg
            .contents
            .lines()
            .filter(|l| !l.trim_start().starts_with('#') && l.contains('='))
            .collect();
        assert_eq!(assignments, vec!["project_doc_max_bytes = 1048576"]);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn cursor_instincts_always_skills_agent_requested() {
        let root = fixture("cursor");
        let files = build_cursor(&root).unwrap();

        let inst = find(&files, ".cursor/rules/instincts.mdc");
        assert!(inst.contents.contains("alwaysApply: true"));
        assert!(inst.contents.contains("gates-not-rules"));

        let contract = find(&files, ".cursor/rules/agents-contract.mdc");
        assert!(contract.contents.contains("alwaysApply: true"));
        assert!(contract.contents.contains("Gate sequence"));

        // skill rule keyed by frontmatter name (getting-started, not _getting-started)
        let skill = find(&files, ".cursor/rules/skill-getting-started.mdc");
        assert!(skill.contents.contains("alwaysApply: false"));
        assert!(skill.contents.contains("description:"));
        assert!(
            !skill.contents.contains("globs:"),
            "an Agent-Requested rule must omit globs"
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn opencode_schema_instructions_and_verbatim_skill() {
        let root = fixture("opencode");
        let files = build_opencode(&root).unwrap();

        let cfg = find(&files, "opencode.json");
        assert!(cfg.contents.contains("https://opencode.ai/config.json"));
        assert!(cfg.contents.contains("AGENTS.md"));

        let src = fs::read_to_string(root.join("skills/brainstorm-design/SKILL.md")).unwrap();
        let copy = find(&files, ".opencode/skills/brainstorm-design/SKILL.md");
        assert_eq!(copy.contents, src, "opencode skill is a verbatim copy");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn claude_wires_both_hooks() {
        // build_claude_hooks returns the hooks JSON; merge_claude_settings embeds them.
        let root = fixture("claude");
        let hooks = build_claude_hooks(&root).unwrap();
        let merged = merge_claude_settings(None, hooks, "/fw/bin/gatekeeper").unwrap();
        let s = serde_json::to_string_pretty(&merged).unwrap();
        assert!(s.contains("UserPromptSubmit"));
        assert!(s.contains("PreToolUse"));
        assert!(s.contains("security-scan.sh"));
        assert!(s.contains("skill-activation.sh"));
        assert!(s.contains("Bash|Write|Edit|MultiEdit"));
        // Framework root is referenced in hook paths.
        assert!(s.contains(root.to_str().unwrap()));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn apply_then_check_is_idempotent_and_detects_drift() {
        let root = fixture("apply");
        let files = build_cursor(&root).unwrap();
        assert_eq!(apply_or_check(&files, &root, false), 0, "first write");
        assert_eq!(apply_or_check(&files, &root, true), 0, "re-check is clean");
        fs::write(root.join(".cursor/rules/instincts.mdc"), "tampered\n").unwrap();
        assert_eq!(apply_or_check(&files, &root, true), 1, "drift is detected");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn missing_agents_md_is_an_error() {
        let root = std::env::temp_dir().join(format!("topo_adapt_noagents_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("instincts")).unwrap();
        assert!(build_codex(&root).is_err());
        assert!(build_cursor(&root).is_err());
        assert!(build_opencode(&root).is_err());
        assert!(build_claude(&root).is_err());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn skill_name_comes_from_frontmatter_not_dir() {
        let root = fixture("skillname");
        let names: Vec<String> = load_skills(&root).into_iter().map(|s| s.name).collect();
        assert_eq!(names, vec!["brainstorm-design", "getting-started"]);
        let _ = fs::remove_dir_all(&root);
    }

    // ── render_contract unit tests (AC-2, AC-3) ───────────────────────────────

    const MINI_TEMPLATE: &str =
        "Root: {{ARTIFACTS_ROOT}}\nCmd: {{GATEKEEPER_CMD}}\nNote: {{BINARY_NOTE}}\n";

    #[test]
    fn framework_render_contains_docs_and_no_topology() {
        let ctx = framework_ctx();
        let rendered = render_contract(MINI_TEMPLATE, &ctx).unwrap();
        assert!(
            rendered.contains("docs"),
            "framework render must contain 'docs': {rendered}"
        );
        assert!(
            !rendered.contains(".claude/topology"),
            "framework render must not contain '.claude/topology': {rendered}"
        );
        assert!(
            rendered.contains("gatekeeper"),
            "must contain gatekeeper cmd"
        );
    }

    #[test]
    fn project_render_contains_topology_and_no_docs_root() {
        let ctx = project_ctx();
        let rendered = render_contract(MINI_TEMPLATE, &ctx).unwrap();
        assert!(
            rendered.contains(".claude/topology"),
            "project render must contain '.claude/topology': {rendered}"
        );
        // The `docs` literal must NOT appear as an artifact-path prefix in the rendered output.
        // (MINI_TEMPLATE has no docs/ path, but check the rendered text doesn't re-introduce it.)
        assert!(
            !rendered.contains("docs"),
            "project render must not contain 'docs': {rendered}"
        );
        assert!(
            rendered.contains("GATEKEEPER_BIN"),
            "project render carries the wiring note (spec §1): {rendered}"
        );
    }

    #[test]
    fn framework_render_has_no_wiring_note() {
        let rendered = render_contract(MINI_TEMPLATE, &framework_ctx()).unwrap();
        assert!(
            !rendered.contains("GATEKEEPER_BIN"),
            "framework render must not carry the governed wiring note: {rendered}"
        );
    }

    #[test]
    fn render_fail_closed_unknown_placeholder() {
        let bad = "Valid: {{ARTIFACTS_ROOT}}\nBad: {{UNKNOWN_TOKEN}}\n";
        let ctx = framework_ctx();
        let err = render_contract(bad, &ctx).unwrap_err();
        assert!(
            err.contains("UNKNOWN_TOKEN"),
            "error must name the offending placeholder: {err}"
        );
    }

    #[test]
    fn render_fail_closed_typo_placeholder() {
        // A common typo: wrong casing / extra chars.
        let bad = "Root: {{ARTIFACT_ROOT}}\n"; // missing 'S'
        let ctx = framework_ctx();
        let err = render_contract(bad, &ctx).unwrap_err();
        assert!(
            err.contains("ARTIFACT_ROOT"),
            "error must name the typo'd placeholder: {err}"
        );
    }

    #[test]
    fn render_binary_note_substituted() {
        let ctx = ContractCtx {
            artifacts_root: "docs".to_owned(),
            binary_note: "The binary is wired via GATEKEEPER_BIN.\n".to_owned(),
        };
        let rendered = render_contract(MINI_TEMPLATE, &ctx).unwrap();
        assert!(
            rendered.contains("The binary is wired via GATEKEEPER_BIN."),
            "binary_note must appear in output: {rendered}"
        );
    }

    #[test]
    fn render_empty_binary_note_produces_no_residual() {
        let ctx = framework_ctx(); // binary_note is empty
        let rendered = render_contract(MINI_TEMPLATE, &ctx).unwrap();
        // The {{BINARY_NOTE}} token must be gone (replaced with empty string).
        assert!(
            !rendered.contains("{{"),
            "no residual {{ after render: {rendered}"
        );
    }

    // ── Task 1: red fixtures for partial-file primitives ──────────────────────

    // ensure_import_line tests

    #[test]
    fn import_line_none_creates_file_with_line() {
        let result = ensure_import_line(None, "@.topology/CONTRACT.md");
        match result {
            Edit::Created(contents) => {
                assert_eq!(contents, "@.topology/CONTRACT.md\n");
            }
            other => panic!("expected Created, got {other:?}"),
        }
    }

    #[test]
    fn import_line_already_present_is_unchanged() {
        let existing = "# My file\n\n@.topology/CONTRACT.md\n\nSome content.\n";
        let result = ensure_import_line(Some(existing), "@.topology/CONTRACT.md");
        assert!(
            matches!(result, Edit::Unchanged),
            "expected Unchanged, got {result:?}"
        );
    }

    #[test]
    fn import_line_absent_appended_preserving_content() {
        let existing = "# My project\n\nSome user content.\n";
        let result = ensure_import_line(Some(existing), "@.topology/CONTRACT.md");
        match result {
            Edit::Updated(contents) => {
                assert!(
                    contents.starts_with("# My project\n"),
                    "prior content preserved"
                );
                assert!(
                    contents.contains("Some user content."),
                    "prior content preserved"
                );
                assert!(
                    contents.ends_with("@.topology/CONTRACT.md\n"),
                    "import appended at end"
                );
            }
            other => panic!("expected Updated, got {other:?}"),
        }
    }

    #[test]
    fn import_line_trimmed_match_counts_as_present() {
        // Line present but with trailing whitespace in the existing file.
        let existing = "# My file\n@.topology/CONTRACT.md   \nOther content.\n";
        let result = ensure_import_line(Some(existing), "@.topology/CONTRACT.md");
        assert!(
            matches!(result, Edit::Unchanged),
            "trimmed match → Unchanged, got {result:?}"
        );
    }

    // ensure_managed_block tests

    const BEGIN_MARKER: &str = "<!-- BEGIN TOPOLOGY MANAGED BLOCK -->";
    const END_MARKER: &str = "<!-- END TOPOLOGY MANAGED BLOCK -->";

    #[test]
    fn managed_block_none_creates_wrapped_body() {
        let body = "See `.topology/CONTRACT.md` for the operating contract.";
        let result = ensure_managed_block(None, body);
        match result {
            Edit::Created(contents) => {
                assert!(contents.contains(BEGIN_MARKER));
                assert!(contents.contains(END_MARKER));
                assert!(contents.contains(body));
            }
            other => panic!("expected Created, got {other:?}"),
        }
    }

    #[test]
    fn managed_block_absent_appended_to_existing_content() {
        let existing = "# Prior content\n\nSome user text.\n";
        let body = "See `.topology/CONTRACT.md`.";
        let result = ensure_managed_block(Some(existing), body);
        match result {
            Edit::Updated(contents) => {
                assert!(
                    contents.starts_with("# Prior content\n"),
                    "prior content preserved"
                );
                assert!(
                    contents.contains("Some user text."),
                    "prior content preserved"
                );
                assert!(contents.contains(BEGIN_MARKER));
                assert!(contents.contains(END_MARKER));
                assert!(contents.contains(body));
            }
            other => panic!("expected Updated, got {other:?}"),
        }
    }

    #[test]
    fn managed_block_identical_body_is_unchanged() {
        let body = "See `.topology/CONTRACT.md`.";
        let existing = format!("# Prior\n\n{BEGIN_MARKER}\n{body}\n{END_MARKER}\n");
        let result = ensure_managed_block(Some(&existing), body);
        assert!(
            matches!(result, Edit::Unchanged),
            "identical body → Unchanged, got {result:?}"
        );
    }

    #[test]
    fn managed_block_differing_body_replaces_in_place() {
        let old_body = "Old content.";
        let new_body = "New content.";
        let outside_before = "# Prior\n\nUser content above.\n";
        let outside_after = "\nUser content below.\n";
        let existing =
            format!("{outside_before}{BEGIN_MARKER}\n{old_body}\n{END_MARKER}{outside_after}");
        let result = ensure_managed_block(Some(&existing), new_body);
        match result {
            Edit::Updated(contents) => {
                assert!(
                    contents.contains("User content above."),
                    "content before preserved"
                );
                assert!(
                    contents.contains("User content below."),
                    "content after preserved"
                );
                assert!(contents.contains(new_body), "new body present");
                assert!(!contents.contains(old_body), "old body removed");
            }
            other => panic!("expected Updated, got {other:?}"),
        }
    }

    #[test]
    fn managed_block_malformed_begin_without_end_is_failed() {
        let existing = format!("# Something\n{BEGIN_MARKER}\nNo end marker here.\n");
        let result = ensure_managed_block(Some(&existing), "body");
        assert!(
            matches!(result, Edit::Failed(_)),
            "malformed → Failed, got {result:?}"
        );
    }

    #[test]
    fn managed_block_duplicate_begin_is_failed() {
        let existing = format!(
            "# Something\n{BEGIN_MARKER}\nsome content\n{BEGIN_MARKER}\nmore\n{END_MARKER}\n"
        );
        let result = ensure_managed_block(Some(&existing), "body");
        assert!(
            matches!(result, Edit::Failed(_)),
            "duplicate begin → Failed, got {result:?}"
        );
    }

    // merge_claude_settings tests

    #[test]
    fn merge_settings_none_existing_sets_hooks_and_env() {
        let hooks = serde_json::json!({"UserPromptSubmit": []});
        let result = merge_claude_settings(None, hooks.clone(), "/fw/bin/gatekeeper").unwrap();
        assert_eq!(result["hooks"], hooks);
        assert_eq!(result["env"]["GATEKEEPER_BIN"], "/fw/bin/gatekeeper");
    }

    #[test]
    fn merge_settings_preserves_user_model_key() {
        let existing = serde_json::json!({"model": "claude-opus-4-5", "other": "value"});
        let hooks = serde_json::json!({"PreToolUse": []});
        let result =
            merge_claude_settings(Some(existing), hooks.clone(), "/fw/bin/gatekeeper").unwrap();
        assert_eq!(
            result["model"], "claude-opus-4-5",
            "user model key preserved"
        );
        assert_eq!(result["other"], "value", "other key preserved");
        assert_eq!(result["hooks"], hooks);
        assert_eq!(result["env"]["GATEKEEPER_BIN"], "/fw/bin/gatekeeper");
    }

    #[test]
    fn merge_settings_preserves_other_env_keys() {
        let existing = serde_json::json!({
            "env": {"MY_VAR": "hello", "GATEKEEPER_BIN": "old_path"}
        });
        let hooks = serde_json::json!({});
        let result = merge_claude_settings(Some(existing), hooks, "/fw/bin/gatekeeper").unwrap();
        assert_eq!(result["env"]["MY_VAR"], "hello", "other env key preserved");
        assert_eq!(
            result["env"]["GATEKEEPER_BIN"], "/fw/bin/gatekeeper",
            "BIN updated"
        );
    }

    #[test]
    fn merge_settings_non_object_existing_is_err() {
        let existing = serde_json::json!([1, 2, 3]);
        let result = merge_claude_settings(Some(existing), serde_json::json!({}), "/bin/gk");
        assert!(result.is_err(), "non-object existing → Err");
        let msg = result.unwrap_err();
        assert!(msg.contains("not a JSON object"), "error message: {msg}");
    }
}
