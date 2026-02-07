use std::sync::Arc;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::Result;
use crate::storage::StorageBackend;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncResult {
    pub pushed: usize,
    pub pulled: usize,
    pub conflicts: Vec<SyncConflict>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConflict {
    pub memory_id: Uuid,
    pub local_updated_at: String,
    pub remote_updated_at: String,
}

pub struct SyncEngine {
    local: Arc<dyn StorageBackend>,
    remote: Arc<dyn StorageBackend>,
}

impl SyncEngine {
    pub fn new(local: Arc<dyn StorageBackend>, remote: Arc<dyn StorageBackend>) -> Self {
        Self { local, remote }
    }

    /// Push local changes to remote. Returns number of memories pushed.
    /// Uses watermark persistence to resume from last sync point.
    pub async fn push(&self, since: &str) -> Result<usize> {
        let watermark_key = "push_watermark";
        let effective_since = self.local.get_sync_watermark(watermark_key).await?
            .unwrap_or_else(|| since.to_string());
        let local_memories = self.local.list_memories_since(&effective_since, 10000).await?;
        let mut pushed = 0;
        for record in &local_memories {
            self.remote.upsert_memory(record).await?;
            pushed += 1;
        }
        if pushed > 0 {
            let now = Utc::now().to_rfc3339();
            self.local.set_sync_watermark(watermark_key, &now).await?;
        }
        Ok(pushed)
    }

    /// Pull remote changes to local. Returns number of memories pulled.
    /// Uses watermark persistence to resume from last sync point.
    pub async fn pull(&self, since: &str) -> Result<usize> {
        let watermark_key = "pull_watermark";
        let effective_since = self.local.get_sync_watermark(watermark_key).await?
            .unwrap_or_else(|| since.to_string());
        let remote_memories = self.remote.list_memories_since(&effective_since, 10000).await?;
        let mut pulled = 0;
        for record in &remote_memories {
            self.local.upsert_memory(record).await?;
            pulled += 1;
        }
        if pulled > 0 {
            let now = Utc::now().to_rfc3339();
            self.local.set_sync_watermark(watermark_key, &now).await?;
        }
        Ok(pulled)
    }

    /// Full bidirectional sync. Pushes local changes, then pulls remote changes.
    /// Detects conflicts where both sides have been modified since `since`.
    pub async fn full_sync(&self, since: &str) -> Result<SyncResult> {
        let local_memories = self.local.list_memories_since(since, 10000).await?;
        let remote_memories = self.remote.list_memories_since(since, 10000).await?;

        // Build a map of remote memory IDs → updated_at for conflict detection
        let remote_map: std::collections::HashMap<Uuid, String> = remote_memories
            .iter()
            .map(|m| (m.id, m.updated_at.clone()))
            .collect();

        let mut conflicts = Vec::new();
        let mut pushed = 0;

        // Push local → remote, detecting conflicts
        for record in &local_memories {
            if let Some(remote_updated) = remote_map.get(&record.id) {
                // Both sides modified — conflict (last-writer-wins: push local anyway)
                if *remote_updated != record.updated_at {
                    conflicts.push(SyncConflict {
                        memory_id: record.id,
                        local_updated_at: record.updated_at.clone(),
                        remote_updated_at: remote_updated.clone(),
                    });
                }
            }
            self.remote.upsert_memory(record).await?;
            pushed += 1;
        }

        // Pull remote → local (skip items we just pushed)
        let local_ids: std::collections::HashSet<Uuid> =
            local_memories.iter().map(|m| m.id).collect();
        let mut pulled = 0;
        for record in &remote_memories {
            if !local_ids.contains(&record.id) {
                self.local.upsert_memory(record).await?;
                pulled += 1;
            }
        }

        Ok(SyncResult {
            pushed,
            pulled,
            conflicts,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_result_serde() {
        let result = SyncResult {
            pushed: 5,
            pulled: 3,
            conflicts: vec![SyncConflict {
                memory_id: Uuid::now_v7(),
                local_updated_at: "2025-01-01T00:00:00Z".to_string(),
                remote_updated_at: "2025-01-01T01:00:00Z".to_string(),
            }],
        };
        let json = serde_json::to_string(&result).unwrap();
        let deserialized: SyncResult = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.pushed, 5);
        assert_eq!(deserialized.pulled, 3);
        assert_eq!(deserialized.conflicts.len(), 1);
    }
}
