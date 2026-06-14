//! Router eval harness (Phase 15 workstream B, Slice 3).
//!
//! Measures the LIVE keyword router (`gatekeeper activate`) against the shipped
//! `hooks/skill-rules.json` over an intent-labeled corpus
//! (`tests/fixtures/routing-eval.jsonl`). It is a deterministic regression guard,
//! not a semantic judge (a semantic layer is explicitly rejected — offline-first).
//!
//! Metrics (ROADMAP Phase 15): **recall** over `(prompt, expected require-skill)`
//! pairs, **precision** over all `(prompt, routed-skill)` outputs. The asserted floors
//! are the ROADMAP targets (recall >= 0.90 / precision >= 0.80); the shipped router
//! clears both with margin on this corpus (measured 0.956 / 0.921 — see
//! docs/verify/2026-06-14-path-routing-slice3.md). The floors guard against regression,
//! not aspiration.

use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

// Asserted regression floors = the ROADMAP targets; the router clears both with
// margin (measured recall 0.956, precision 0.921).
const RECALL_FLOOR: f64 = 0.90;
const PRECISION_FLOOR: f64 = 0.80;

/// A framework root carrying the repo's REAL `hooks/skill-rules.json`, so the eval
/// measures the shipped routing rules rather than a replica that can drift.
fn eval_root() -> PathBuf {
    let root = std::env::temp_dir().join(format!("topo_route_eval_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("skills")).unwrap();
    fs::write(root.join("AGENTS.md"), "").unwrap();
    fs::create_dir_all(root.join("hooks")).unwrap();
    let live_rules = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("hooks")
        .join("skill-rules.json");
    fs::copy(&live_rules, root.join("hooks").join("skill-rules.json")).unwrap();
    root
}

/// The set of skills whose enforcement is `require`, read from the shipped rules.
fn require_skills() -> BTreeSet<String> {
    let raw = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("hooks")
            .join("skill-rules.json"),
    )
    .unwrap();
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let mut out = BTreeSet::new();
    if let Some(skills) = v.get("skills").and_then(|s| s.as_object()) {
        for (name, cfg) in skills {
            if cfg.get("enforcement").and_then(|e| e.as_str()) == Some("require") {
                out.insert(name.clone());
            }
        }
    }
    out
}

/// Run `gatekeeper activate` with `prompt` on stdin; return the routed skill names.
fn routed_skills(root: &Path, prompt: &str) -> BTreeSet<String> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_gatekeeper"))
        .current_dir(root)
        .arg("activate")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to spawn gatekeeper");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(prompt.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    // Parse the "  - <skill> [<enforcement>]" lines.
    stdout
        .lines()
        .filter_map(|l| {
            let l = l.trim();
            let rest = l.strip_prefix("- ")?;
            let name = rest.split(" [").next()?;
            Some(name.to_string())
        })
        .collect()
}

#[derive(serde::Deserialize)]
struct Case {
    prompt: String,
    expect: Vec<String>,
}

#[test]
fn router_meets_recall_precision_floor() {
    let root = eval_root();
    let require = require_skills();
    let corpus = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("routing-eval.jsonl"),
    )
    .unwrap();
    let cases: Vec<Case> = corpus
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("corpus line is valid JSON"))
        .collect();
    assert!(cases.len() >= 50, "corpus must have >=50 prompts");

    // recall over (prompt, expected REQUIRE-skill); precision over all (prompt, routed-skill).
    let (mut recall_hit, mut recall_total) = (0u32, 0u32);
    let (mut prec_hit, mut prec_total) = (0u32, 0u32);
    for case in &cases {
        let routed = routed_skills(&root, &case.prompt);
        let expect: BTreeSet<&String> = case.expect.iter().collect();
        for want in &case.expect {
            if require.contains(want) {
                recall_total += 1;
                if routed.contains(want) {
                    recall_hit += 1;
                }
            }
        }
        for got in &routed {
            prec_total += 1;
            if expect.contains(got) {
                prec_hit += 1;
            }
        }
    }
    let _ = fs::remove_dir_all(&root);

    let recall = if recall_total == 0 {
        1.0
    } else {
        recall_hit as f64 / recall_total as f64
    };
    let precision = if prec_total == 0 {
        1.0
    } else {
        prec_hit as f64 / prec_total as f64
    };
    eprintln!(
        "router eval: recall={recall:.3} ({recall_hit}/{recall_total} require-skill expectations), \
         precision={precision:.3} ({prec_hit}/{prec_total} routed outputs); \
         floors recall>={RECALL_FLOOR} precision>={PRECISION_FLOOR} (target 0.90/0.80)"
    );
    assert!(
        recall >= RECALL_FLOOR,
        "recall {recall:.3} below floor {RECALL_FLOOR}"
    );
    assert!(
        precision >= PRECISION_FLOOR,
        "precision {precision:.3} below floor {PRECISION_FLOOR}"
    );
}
