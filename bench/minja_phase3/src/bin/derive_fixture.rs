//! Derive the pre-registered Phase-3 fixture from the corpus, once.
//!
//! Run this only when the corpus or the derivation rule changes deliberately.
//! The bench reads the **committed** fixture and never calls this, so a change
//! to the attack text shows up as a diff in `fixtures/phase3_records.json`
//! rather than silently moving the published number.
//!
//! ```text
//! cargo run -p mnemo-minja-phase3-bench --bin derive_fixture
//! ```

use std::path::PathBuf;

use mnemo_minja_phase3_bench::fixture::{CorpusRow, Phase3Fixture, derive_pairs};
use sha2::{Digest, Sha256};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("repo root")
        .to_path_buf();
    let corpus_path = repo.join("crates/mnemo-core/benches/data/longmemeval_m.jsonl");
    let raw = std::fs::read(&corpus_path)?;
    let corpus_sha256 = hex::encode(Sha256::digest(&raw));

    let rows: Vec<CorpusRow> = String::from_utf8(raw)?
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()?;

    let fixture = Phase3Fixture {
        corpus_sha256: corpus_sha256.clone(),
        derivation: "Structurally derived end-state of MINJA Phase-2 shortening: an ordinary-register \
             note asserting a COMPETING answer to the corpus record's own victim query, carrying no \
             self-referential bridging markers. Each poisoned record has a benign twin sharing its \
             opening clause, tags and topic vocabulary and asserting no competing answer. These are \
             NOT verbatim examples from arXiv:2503.03704 and were NOT produced by an attacker model \
             — Phases 1 and 2 are generative and are held out of scope in issue #37 pending an LLM \
             budget."
            .to_string(),
        pairs: derive_pairs(&rows),
    };

    let out = repo.join("bench/minja_phase3/fixtures/phase3_records.json");
    std::fs::create_dir_all(out.parent().unwrap())?;
    std::fs::write(&out, serde_json::to_string_pretty(&fixture)? + "\n")?;

    println!("corpus  {} ({} rows)", corpus_path.display(), rows.len());
    println!("sha256  {corpus_sha256}");
    println!("wrote   {}", out.display());
    println!("\nThis file is COMMITTED. Re-deriving it changes the published number;");
    println!("the diff is the audit trail for that.");
    Ok(())
}
