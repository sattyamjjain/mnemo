//! Sync configuration (v0.4.0 P2-6).

use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MdSyncSpec {
    /// Root of the git repo we sync against. Watcher polls every
    /// file under `glob`.
    pub repo: PathBuf,
    /// Glob pattern (relative to `repo`) for files to sync.
    /// Default `**/*.md`.
    pub glob: String,
    /// Author signature for commit-on-flush.
    pub commit_author: String,
    /// Buffer edits this long before issuing a single git commit.
    /// Smaller values mean more commits but smaller windows for
    /// data loss; the default 250ms matches Wuphf's published flush
    /// cadence.
    pub flush_every: Duration,
}

impl Default for MdSyncSpec {
    fn default() -> Self {
        Self {
            repo: PathBuf::from("."),
            glob: "**/*.md".to_string(),
            commit_author: "mnemo-md-sync <mnemo@localhost>".to_string(),
            flush_every: Duration::from_millis(250),
        }
    }
}

/// How aggressive the flush-to-disk side should be when an in-engine
/// remember would overwrite a file the human is editing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SyncFlushPolicy {
    /// Always overwrite the on-disk file with the engine's version.
    /// Used during a fresh import.
    PreferEngine,
    /// Always preserve the on-disk file; engine writes that would
    /// overwrite are written to a sibling `.conflict.md`. Default.
    PreferDisk,
    /// Pick whichever has the more recent timestamp; tie goes to disk.
    NewerWins,
}

impl Default for SyncFlushPolicy {
    fn default() -> Self {
        Self::PreferDisk
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_glob_matches_all_md() {
        let s = MdSyncSpec::default();
        assert_eq!(s.glob, "**/*.md");
    }

    #[test]
    fn default_flush_is_under_one_second() {
        let s = MdSyncSpec::default();
        assert!(s.flush_every < Duration::from_secs(1));
    }

    #[test]
    fn default_policy_prefers_disk() {
        let p = SyncFlushPolicy::default();
        assert_eq!(p, SyncFlushPolicy::PreferDisk);
    }
}
