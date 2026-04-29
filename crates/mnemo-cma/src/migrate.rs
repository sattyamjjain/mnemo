//! One-shot CMA tree import + export (v0.4.1 P0-2).

use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::Digest;

use crate::audit_bridge::{BridgedEvent, CmaSource, bridge_event};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportSummary {
    pub files: u32,
    pub memories: u32,
    pub audit_events_bridged: u32,
    pub hmac_chain_head: [u8; 32],
}

/// Walk a CMA `.memory/` tree and produce a deterministic
/// [`ImportSummary`]. The actual engine ingestion is wired in by the
/// caller (the binary in `mnemo-cli`); this fn is pure so the
/// summary is byte-identical between two runs over the same tree.
pub fn import_cma_tree(memory_dir: &Path) -> std::io::Result<(ImportSummary, Vec<BridgedEvent>)> {
    let mut files = 0u32;
    let mut memories = 0u32;
    let mut bridged = Vec::new();
    let mut head = [0u8; 32];

    if !memory_dir.exists() {
        return Ok((
            ImportSummary {
                files: 0,
                memories: 0,
                audit_events_bridged: 0,
                hmac_chain_head: head,
            },
            bridged,
        ));
    }

    // Sort entries so the output is stable (same tree → same head).
    let mut entries: Vec<_> = walk(memory_dir)?;
    entries.sort_by(|a, b| a.cmp(b));

    for path in entries {
        if !path.is_file() {
            continue;
        }
        files += 1;
        let bytes = std::fs::metadata(&path)?.len();
        let rel = path.strip_prefix(memory_dir).unwrap_or(&path);
        let event = bridge_event(
            CmaSource::CmaImport,
            &rel.to_string_lossy(),
            "import",
            bytes,
            head,
        );
        head = event.bridge_hash;
        bridged.push(event);
        if path.extension().and_then(|e| e.to_str()) == Some("md") {
            memories += 1;
        }
    }

    Ok((
        ImportSummary {
            files,
            memories,
            audit_events_bridged: bridged.len() as u32,
            hmac_chain_head: head,
        },
        bridged,
    ))
}

fn walk(dir: &Path) -> std::io::Result<Vec<std::path::PathBuf>> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(p) = stack.pop() {
        if p.is_dir() {
            for ent in std::fs::read_dir(&p)? {
                let ent = ent?;
                stack.push(ent.path());
            }
        } else if p.is_file() {
            out.push(p);
        }
    }
    Ok(out)
}

/// Export a synthesized CMA tree from a list of (path, body) pairs.
/// Used by tests + by `mnemo cma export` so users can leave mnemo
/// cleanly.
pub fn export_to_tree(memory_dir: &Path, files: &[(String, String)]) -> std::io::Result<()> {
    std::fs::create_dir_all(memory_dir)?;
    for (rel, body) in files {
        let path = memory_dir.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, body)?;
    }
    Ok(())
}

/// Hash a CMA tree's body content into a 32-byte SHA-256 so two
/// trees with the same files-and-bytes produce the same digest.
/// Used by the round-trip test.
pub fn tree_digest(memory_dir: &Path) -> std::io::Result<[u8; 32]> {
    let mut entries = walk(memory_dir)?;
    entries.sort();
    let mut h = sha2::Sha256::new();
    for p in entries {
        let rel = p
            .strip_prefix(memory_dir)
            .unwrap_or(&p)
            .to_string_lossy()
            .to_string();
        let body = std::fs::read(&p)?;
        h.update(rel.as_bytes());
        h.update(b"\n");
        h.update(&body);
        h.update(b"\n--\n");
    }
    Ok(h.finalize().into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_file(dir: &Path, rel: &str, body: &str) {
        let p = dir.join(rel);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(p, body).unwrap();
    }

    #[test]
    fn import_empty_dir_is_empty_summary() {
        let dir = tempfile::tempdir().unwrap();
        let (sum, ev) = import_cma_tree(dir.path()).unwrap();
        assert_eq!(sum.files, 0);
        assert_eq!(sum.memories, 0);
        assert_eq!(sum.audit_events_bridged, 0);
        assert!(ev.is_empty());
    }

    #[test]
    fn import_counts_md_files_and_chains_audit() {
        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path(), "a.md", "alpha");
        write_file(dir.path(), "b.md", "beta");
        write_file(dir.path(), "notes/c.md", "gamma");
        let (sum, ev) = import_cma_tree(dir.path()).unwrap();
        assert_eq!(sum.files, 3);
        assert_eq!(sum.memories, 3);
        assert_eq!(ev.len(), 3);
        assert_ne!(sum.hmac_chain_head, [0u8; 32]);
        // Chain links — each next event's prev_hash matches the
        // previous event's bridge_hash.
        for w in ev.windows(2) {
            assert_eq!(w[1].prev_hash, w[0].bridge_hash);
        }
    }

    #[test]
    fn import_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path(), "a.md", "alpha");
        write_file(dir.path(), "b.md", "beta");
        let (s1, _) = import_cma_tree(dir.path()).unwrap();
        let (s2, _) = import_cma_tree(dir.path()).unwrap();
        assert_eq!(
            s1, s2,
            "running import twice must produce identical summary"
        );
    }

    #[test]
    fn export_round_trip_preserves_byte_content() {
        let original = tempfile::tempdir().unwrap();
        write_file(original.path(), "a.md", "alpha");
        write_file(original.path(), "nested/b.md", "beta");
        let original_digest = tree_digest(original.path()).unwrap();

        // Read everything as (path, body) pairs.
        let mut pairs = Vec::new();
        for path in walk(original.path()).unwrap() {
            let rel = path.strip_prefix(original.path()).unwrap();
            let body = std::fs::read_to_string(&path).unwrap();
            pairs.push((rel.to_string_lossy().to_string(), body));
        }

        let exported = tempfile::tempdir().unwrap();
        export_to_tree(exported.path(), &pairs).unwrap();
        let exported_digest = tree_digest(exported.path()).unwrap();
        assert_eq!(original_digest, exported_digest);
    }
}
