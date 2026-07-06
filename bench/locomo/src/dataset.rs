//! Shared LoCoMo-/LongMemEval-style fixture loader.
//!
//! Centralised so benches that need the bundled dialogue slice load it the same
//! way instead of each bin re-deriving the record shape and path. The sibling
//! `semantic_recall_bench` / `grep_vs_vector_replay` bins predate this module
//! and keep their own private copies; new benches (`reproduction_bench`) use
//! this one.

use std::path::{Path, PathBuf};

use serde::Deserialize;
use sha2::{Digest, Sha256};

/// One self-contained record: its `query` is answerable from its own `content`,
/// so the record is its own gold document (matched downstream by `id`).
#[derive(Debug, Deserialize, Clone)]
pub struct LongMemRecord {
    pub id: String,
    pub conversation_id: String,
    pub turn: u32,
    pub content: String,
    pub tags: Vec<String>,
    pub query: String,
    pub expected: String,
}

/// The bundled 45-record LongMemEval_M slice under `mnemo-core/benches/data`.
/// Resolved relative to this crate so the path is stable regardless of cwd.
pub fn default_dataset_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("crates")
        .join("mnemo-core")
        .join("benches")
        .join("data")
        .join("longmemeval_m.jsonl")
}

/// Parse the JSONL fixture. Panics with a clear message on a missing/invalid
/// file — a bench must fail loud, never silently score an empty corpus.
pub fn load_dataset(path: &Path) -> Vec<LongMemRecord> {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("failed to read dataset at {path:?}: {e}"));
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str::<LongMemRecord>(l).expect("invalid record"))
        .collect()
}

/// SHA-256 of the raw fixture bytes — pinned in reports so a reader can confirm
/// they scored the identical dataset.
pub fn dataset_sha(path: &Path) -> String {
    let mut h = Sha256::new();
    h.update(std::fs::read(path).unwrap_or_default());
    hex::encode(h.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_fixture_loads_and_is_nonempty() {
        let path = default_dataset_path();
        let ds = load_dataset(&path);
        assert!(
            !ds.is_empty(),
            "bundled LongMemEval_M slice must be present"
        );
        // Every record must carry the fields the recall gold-match relies on.
        for r in &ds {
            assert!(!r.id.is_empty());
            assert!(!r.query.is_empty());
            assert!(!r.content.is_empty());
        }
        // SHA is a 64-hex-char digest.
        assert_eq!(dataset_sha(&path).len(), 64);
    }
}
