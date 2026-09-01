//! The exported chain must verify under the STANDALONE verifier, and must stop
//! verifying the moment a record is edited.
//!
//! # Why this test and not a Rust-side assertion
//!
//! `mnemo_core::hash::verify_chain` already has unit tests. They prove mnemo
//! agrees with itself, which is exactly the property an auditor cannot use. The
//! claim being defended here is narrower and harder: that
//! `tools/verify_mnemo_chain.py` — a file with no mnemo dependency — reaches the
//! same verdict on a real export.
//!
//! So this test shells out to Python. If the export format drifts, or the
//! verifier's hash construction drifts from the engine's, the two stop agreeing
//! and this fails. That is the only way to catch a divergence between two
//! implementations that are deliberately independent.
//!
//! Skips (green) when `python3` is absent, and says so rather than passing
//! silently — a skipped check that looks like a pass is the failure mode this
//! repository has been bitten by more than once.

use std::path::PathBuf;
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("repo root")
        .to_path_buf()
}

fn python() -> Option<&'static str> {
    ["python3", "python"]
        .into_iter()
        .find(|c| Command::new(c).arg("--version").output().is_ok())
}

/// A hand-built chain in exactly the shape `audit export` emits, so the test
/// does not need a database. The hashes come from `mnemo_core`, so a change to
/// the engine's construction shows up here as a disagreement with the Python.
fn export_lines() -> Vec<String> {
    use mnemo_core::hash::{compute_chain_hash, compute_content_hash};

    let agent = "auditor-demo";
    let ts = "2026-09-01T00:00:00+00:00";
    let contents = [
        "Consent obtained from data subject 4471.",
        "Retention period set to 24 months per policy R-12.",
        "Access request fulfilled.",
    ];

    let mut lines = Vec::new();
    let mut prev_ch: Option<Vec<u8>> = None;
    for (i, c) in contents.iter().enumerate() {
        let ch = compute_content_hash(c, agent, ts);
        let prev = compute_chain_hash(&ch, prev_ch.as_deref());
        lines.push(
            serde_json::json!({
                "index": i,
                "id": format!("00000000-0000-0000-0000-{:012}", i),
                "agent_id": agent,
                "content": c,
                "created_at": ts,
                "content_hash": hex::encode(&ch),
                "prev_hash": hex::encode(&prev),
                "deleted_at": serde_json::Value::Null,
            })
            .to_string(),
        );
        prev_ch = Some(ch);
    }
    lines
}

fn run_verifier(py: &str, chain: &std::path::Path) -> (i32, String) {
    let out = Command::new(py)
        .arg(repo_root().join("tools/verify_mnemo_chain.py"))
        .arg(chain)
        .output()
        .expect("run the standalone verifier");
    let mut text = String::from_utf8_lossy(&out.stdout).to_string();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.code().unwrap_or(-1), text)
}

#[test]
fn standalone_verifier_accepts_a_well_formed_export_and_rejects_a_tampered_one() {
    let Some(py) = python() else {
        eprintln!("SKIP: no python3 on PATH; the standalone verifier could not be exercised");
        return;
    };

    let dir = tempfile::tempdir().expect("tempdir");
    let chain = dir.path().join("chain.jsonl");
    let lines = export_lines();
    std::fs::write(&chain, lines.join("\n") + "\n").expect("write chain");

    // --- clean ---
    let (code, out) = run_verifier(py, &chain);
    assert_eq!(
        code, 0,
        "the standalone verifier rejected a chain that mnemo_core built. The export \
         format and the verifier's hash construction have diverged.\n{out}"
    );
    assert!(out.contains("chain intact"), "{out}");

    // --- tampered: edit the content, leave every hash alone ---
    let tampered = lines.join("\n").replace("24 months", "6 months") + "\n";
    assert!(
        tampered.contains("6 months"),
        "the edit must actually apply"
    );
    std::fs::write(&chain, tampered).expect("write tampered chain");

    let (code, out) = run_verifier(py, &chain);
    assert_eq!(
        code, 1,
        "a record's content was edited and the verifier did not object. This is the \
         one property the whole artefact exists for.\n{out}"
    );
    assert!(
        out.contains("BROKEN at record index 1"),
        "the verifier must name the offending record, not just fail:\n{out}"
    );
}

/// The strict/compat divergence is a documented claim in
/// `docs/verify-my-log.md`. If mnemo ever starts checking a null `prev_hash`,
/// or the verifier stops, the doc becomes wrong — so the divergence is pinned.
#[test]
fn a_null_prev_hash_is_a_break_in_strict_mode_and_accepted_in_compat_mode() {
    let Some(py) = python() else {
        eprintln!("SKIP: no python3 on PATH; the strict/compat divergence was not exercised");
        return;
    };

    let dir = tempfile::tempdir().expect("tempdir");
    let chain = dir.path().join("chain.jsonl");
    // Null out the second record's link — one deleted field.
    let lines: Vec<String> = export_lines()
        .into_iter()
        .enumerate()
        .map(|(i, l)| {
            if i == 1 {
                let mut v: serde_json::Value = serde_json::from_str(&l).unwrap();
                v["prev_hash"] = serde_json::Value::Null;
                v.to_string()
            } else {
                l
            }
        })
        .collect();
    std::fs::write(&chain, lines.join("\n") + "\n").expect("write chain");

    let (strict, out) = run_verifier(py, &chain);
    assert_eq!(
        strict, 1,
        "a null prev_hash silences the link check; strict mode must treat that as a \
         break rather than trusting the file's own metadata.\n{out}"
    );

    let out = Command::new(py)
        .arg(repo_root().join("tools/verify_mnemo_chain.py"))
        .arg(&chain)
        .arg("--mnemo-compat")
        .output()
        .expect("run verifier in compat mode");
    assert_eq!(
        out.status.code().unwrap_or(-1),
        0,
        "--mnemo-compat must reproduce mnemo's own behaviour, which accepts a null \
         prev_hash. If this now fails, mnemo's verifier changed and docs/verify-my-log.md \
         needs re-deriving."
    );
}
