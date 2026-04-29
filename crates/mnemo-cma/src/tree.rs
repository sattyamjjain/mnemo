//! CMA filesystem layout types (v0.4.1 P0-2).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncMode {
    /// Mnemo answers reads from CMA tree but does not persist into
    /// its own DuckDB — useful for migration discovery without
    /// committing to the bridge.
    ReadThrough,
    /// Mnemo writes to its own DuckDB AND to the CMA tree on every
    /// `remember`. The bridged audit row chains both.
    WriteThrough,
    /// Mnemo and CMA tree are kept in lock-step via a background
    /// reconciler. Conflict resolution is `engine wins`.
    Mirror,
}

impl SyncMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            SyncMode::ReadThrough => "read_through",
            SyncMode::WriteThrough => "write_through",
            SyncMode::Mirror => "mirror",
        }
    }
}

/// A pointer at one CMA `.memory/` tree on disk plus the namespace
/// it should map into in mnemo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CmaTreeRoot {
    pub root: PathBuf,
    pub namespace: String,
    pub sync: SyncMode,
}

impl CmaTreeRoot {
    pub fn new(root: PathBuf, namespace: impl Into<String>, sync: SyncMode) -> Self {
        Self {
            root,
            namespace: namespace.into(),
            sync,
        }
    }

    pub fn memory_dir(&self) -> PathBuf {
        self.root.join(".memory")
    }

    pub fn audit_log(&self) -> PathBuf {
        self.root.join("audit.jsonl")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_relative_to_root() {
        let r = CmaTreeRoot::new(
            PathBuf::from("/tmp/agent"),
            "primary",
            SyncMode::WriteThrough,
        );
        assert_eq!(r.memory_dir(), PathBuf::from("/tmp/agent/.memory"));
        assert_eq!(r.audit_log(), PathBuf::from("/tmp/agent/audit.jsonl"));
    }

    #[test]
    fn sync_mode_strings_are_stable() {
        for m in [
            SyncMode::ReadThrough,
            SyncMode::WriteThrough,
            SyncMode::Mirror,
        ] {
            assert!(!m.as_str().is_empty());
        }
    }
}
