//! Cold storage interface for archiving memories to object storage.
//!
//! Provides a trait and types for archiving memories to S3-compatible
//! object storage. Memories can be archived from the primary database
//! and restored when needed.
//!
//! # Architecture
//!
//! The [`ColdStorage`] trait defines the contract for any cold storage backend.
//! An [`InMemoryColdStorage`] implementation is provided for testing without
//! requiring real S3 credentials or network access.
//!
//! S3 keys follow the format: `{prefix}/{agent_id}/{memory_id}.json`
//!
//! # Example
//!
//! ```rust
//! use mnemo_core::storage::cold::{ColdStorage, InMemoryColdStorage, ColdStorageConfig};
//!
//! # async fn example() -> mnemo_core::error::Result<()> {
//! let config = ColdStorageConfig {
//!     bucket: "my-bucket".to_string(),
//!     prefix: "memories".to_string(),
//!     endpoint: None,
//!     region: "us-east-1".to_string(),
//! };
//! let storage = InMemoryColdStorage::new(config);
//! # Ok(())
//! # }
//! ```

use std::collections::HashMap;
use std::sync::Mutex;
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::model::memory::MemoryRecord;

/// Configuration for S3-compatible cold storage.
#[derive(Debug, Clone)]
pub struct ColdStorageConfig {
    /// S3 bucket name.
    pub bucket: String,
    /// S3 key prefix for archived memories.
    pub prefix: String,
    /// S3 endpoint URL (for S3-compatible services like MinIO).
    pub endpoint: Option<String>,
    /// AWS region.
    pub region: String,
}

/// Result of an archive operation.
#[derive(Debug, Clone)]
pub struct ArchiveResult {
    /// The UUID of the archived memory.
    pub memory_id: Uuid,
    /// The S3 key where the memory was stored.
    pub s3_key: String,
    /// Size of the serialized payload in bytes.
    pub size_bytes: usize,
}

/// Result of a restore operation.
#[derive(Debug, Clone)]
pub struct RestoreResult {
    /// The UUID of the restored memory.
    pub memory_id: Uuid,
    /// The deserialized memory record.
    pub record: MemoryRecord,
}

/// Trait for cold storage backends.
///
/// Implementations handle archiving memory records to durable object storage
/// (e.g., S3, MinIO, GCS) and restoring them on demand.
#[async_trait::async_trait]
pub trait ColdStorage: Send + Sync {
    /// Archive a memory record to cold storage.
    ///
    /// Serializes the record to JSON and writes it to the configured bucket
    /// under the key `{prefix}/{agent_id}/{memory_id}.json`.
    async fn archive(&self, record: &MemoryRecord) -> Result<ArchiveResult>;

    /// Restore a memory from cold storage by ID.
    ///
    /// Returns an error if the memory is not found in cold storage.
    async fn restore(&self, memory_id: Uuid) -> Result<RestoreResult>;

    /// List archived memory IDs with optional agent filter.
    ///
    /// If `agent_id` is provided, only memories belonging to that agent are
    /// returned. Results are capped at `limit`.
    async fn list_archived(&self, agent_id: Option<&str>, limit: usize) -> Result<Vec<Uuid>>;

    /// Delete an archived memory permanently.
    ///
    /// Returns an error if the memory is not found in cold storage.
    async fn delete_archived(&self, memory_id: Uuid) -> Result<()>;

    /// Check if a memory is archived.
    async fn is_archived(&self, memory_id: Uuid) -> Result<bool>;
}

/// Entry stored in the in-memory cold storage backend.
///
/// Holds the serialized JSON bytes alongside the agent ID for filtering
/// during `list_archived` calls.
#[derive(Debug, Clone)]
struct ArchivedEntry {
    /// The serialized JSON bytes of the memory record.
    data: Vec<u8>,
    /// The S3 key that would be used in a real backend.
    /// Retained for parity with a real S3 backend; not read by the
    /// in-memory implementation itself.
    #[allow(dead_code)]
    s3_key: String,
    /// Agent ID cached for efficient filtering in `list_archived`.
    agent_id: String,
}

/// In-memory cold storage implementation for testing.
///
/// Stores serialized memory records in a `HashMap` protected by a `Mutex`.
/// This avoids any external dependencies while exercising the full
/// [`ColdStorage`] trait contract.
pub struct InMemoryColdStorage {
    config: ColdStorageConfig,
    store: Mutex<HashMap<Uuid, ArchivedEntry>>,
}

impl InMemoryColdStorage {
    /// Create a new in-memory cold storage backend with the given config.
    pub fn new(config: ColdStorageConfig) -> Self {
        Self {
            config,
            store: Mutex::new(HashMap::new()),
        }
    }

    /// Build the S3 key for a given record.
    fn s3_key(&self, agent_id: &str, memory_id: Uuid) -> String {
        format!("{}/{}/{}.json", self.config.prefix, agent_id, memory_id)
    }
}

#[async_trait::async_trait]
impl ColdStorage for InMemoryColdStorage {
    async fn archive(&self, record: &MemoryRecord) -> Result<ArchiveResult> {
        let data = serde_json::to_vec(record)?;
        let size_bytes = data.len();
        let s3_key = self.s3_key(&record.agent_id, record.id);

        let entry = ArchivedEntry {
            data,
            s3_key: s3_key.clone(),
            agent_id: record.agent_id.clone(),
        };

        self.store
            .lock()
            .map_err(|e| Error::Internal(format!("lock poisoned: {e}")))?
            .insert(record.id, entry);

        Ok(ArchiveResult {
            memory_id: record.id,
            s3_key,
            size_bytes,
        })
    }

    async fn restore(&self, memory_id: Uuid) -> Result<RestoreResult> {
        let guard = self
            .store
            .lock()
            .map_err(|e| Error::Internal(format!("lock poisoned: {e}")))?;

        let entry = guard
            .get(&memory_id)
            .ok_or_else(|| Error::NotFound(format!("archived memory {memory_id} not found")))?;

        let record: MemoryRecord = serde_json::from_slice(&entry.data)?;

        Ok(RestoreResult { memory_id, record })
    }

    async fn list_archived(&self, agent_id: Option<&str>, limit: usize) -> Result<Vec<Uuid>> {
        let guard = self
            .store
            .lock()
            .map_err(|e| Error::Internal(format!("lock poisoned: {e}")))?;

        let ids: Vec<Uuid> = guard
            .iter()
            .filter(|(_, entry)| {
                agent_id.is_none_or(|aid| entry.agent_id == aid)
            })
            .map(|(id, _)| *id)
            .take(limit)
            .collect();

        Ok(ids)
    }

    async fn delete_archived(&self, memory_id: Uuid) -> Result<()> {
        let mut guard = self
            .store
            .lock()
            .map_err(|e| Error::Internal(format!("lock poisoned: {e}")))?;

        guard
            .remove(&memory_id)
            .ok_or_else(|| Error::NotFound(format!("archived memory {memory_id} not found")))?;

        Ok(())
    }

    async fn is_archived(&self, memory_id: Uuid) -> Result<bool> {
        let guard = self
            .store
            .lock()
            .map_err(|e| Error::Internal(format!("lock poisoned: {e}")))?;

        Ok(guard.contains_key(&memory_id))
    }
}

// ---------------------------------------------------------------------------
// S3ColdStorage -- real AWS S3 / S3-compatible backend (feature-gated)
// ---------------------------------------------------------------------------

/// S3-compatible cold storage backend.
///
/// Archives memory records as JSON objects in an S3 bucket. Keys follow the
/// format `{prefix}/{agent_id}/{memory_id}.json`.
///
/// # Feature gate
///
/// This type is only available when the `s3` Cargo feature is enabled.
///
/// # Example
///
/// ```rust,ignore
/// use mnemo_core::storage::cold::{ColdStorageConfig, S3ColdStorage};
///
/// # async fn example() -> mnemo_core::error::Result<()> {
/// let config = ColdStorageConfig {
///     bucket: "my-bucket".to_string(),
///     prefix: "memories".to_string(),
///     endpoint: Some("http://localhost:9000".to_string()),
///     region: "us-east-1".to_string(),
/// };
/// let storage = S3ColdStorage::new(config).await;
/// # Ok(())
/// # }
/// ```
#[cfg(feature = "s3")]
pub struct S3ColdStorage {
    /// The underlying AWS S3 client.
    client: aws_sdk_s3::Client,
    /// Cold storage configuration (bucket, prefix, region, endpoint).
    config: ColdStorageConfig,
}

#[cfg(feature = "s3")]
impl S3ColdStorage {
    /// Create a new S3 cold storage backend.
    ///
    /// Loads AWS credentials from the default provider chain. When
    /// `config.endpoint` is set the client will target that URL instead of
    /// the real AWS endpoint, which is useful for S3-compatible services
    /// such as MinIO or LocalStack.
    pub async fn new(config: ColdStorageConfig) -> Self {
        let mut aws_cfg_loader = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .region(aws_config::Region::new(config.region.clone()));

        if let Some(ref endpoint) = config.endpoint {
            aws_cfg_loader = aws_cfg_loader.endpoint_url(endpoint);
        }

        let aws_cfg = aws_cfg_loader.load().await;

        let client = aws_sdk_s3::Client::new(&aws_cfg);

        Self { client, config }
    }

    /// Build the S3 key for a given agent and memory.
    fn s3_key(&self, agent_id: &str, memory_id: Uuid) -> String {
        format!("{}/{}/{}.json", self.config.prefix, agent_id, memory_id)
    }

    /// Build the S3 prefix for listing objects belonging to an agent.
    fn agent_prefix(&self, agent_id: &str) -> String {
        format!("{}/{}/", self.config.prefix, agent_id)
    }

    /// Build the bare prefix for listing all archived objects.
    fn bare_prefix(&self) -> String {
        format!("{}/", self.config.prefix)
    }
}

#[cfg(feature = "s3")]
#[async_trait::async_trait]
impl ColdStorage for S3ColdStorage {
    async fn archive(&self, record: &MemoryRecord) -> Result<ArchiveResult> {
        let data = serde_json::to_vec(record)?;
        let size_bytes = data.len();
        let s3_key = self.s3_key(&record.agent_id, record.id);

        self.client
            .put_object()
            .bucket(&self.config.bucket)
            .key(&s3_key)
            .body(aws_sdk_s3::primitives::ByteStream::from(data))
            .content_type("application/json")
            .send()
            .await
            .map_err(|e| Error::Storage(format!("S3 put_object failed: {e}")))?;

        Ok(ArchiveResult {
            memory_id: record.id,
            s3_key,
            size_bytes,
        })
    }

    async fn restore(&self, memory_id: Uuid) -> Result<RestoreResult> {
        // We do not know the agent_id up front, so we search for the key by
        // listing objects with the bare prefix and filtering by the memory_id
        // suffix. This costs one LIST call but avoids requiring the caller to
        // pass the agent_id for restores.
        let prefix = self.bare_prefix();

        let list_resp = self
            .client
            .list_objects_v2()
            .bucket(&self.config.bucket)
            .prefix(&prefix)
            .send()
            .await
            .map_err(|e| Error::Storage(format!("S3 list_objects_v2 failed: {e}")))?;

        let target_suffix = format!("{memory_id}.json");

        let key = list_resp
            .contents()
            .iter()
            .filter_map(|obj| obj.key())
            .find(|k| k.ends_with(&target_suffix))
            .ok_or_else(|| {
                Error::NotFound(format!("archived memory {memory_id} not found in S3"))
            })?
            .to_string();

        let get_resp = self
            .client
            .get_object()
            .bucket(&self.config.bucket)
            .key(&key)
            .send()
            .await
            .map_err(|e| Error::Storage(format!("S3 get_object failed: {e}")))?;

        let body = get_resp
            .body
            .collect()
            .await
            .map_err(|e| Error::Storage(format!("S3 body collect failed: {e}")))?;

        let record: MemoryRecord = serde_json::from_slice(&body.into_bytes())?;

        Ok(RestoreResult { memory_id, record })
    }

    async fn list_archived(&self, agent_id: Option<&str>, limit: usize) -> Result<Vec<Uuid>> {
        let prefix = match agent_id {
            Some(aid) => self.agent_prefix(aid),
            None => self.bare_prefix(),
        };

        let mut ids: Vec<Uuid> = Vec::new();
        let mut continuation_token: Option<String> = None;

        loop {
            let mut req = self
                .client
                .list_objects_v2()
                .bucket(&self.config.bucket)
                .prefix(&prefix)
                .max_keys(limit.min(1000) as i32);

            if let Some(ref token) = continuation_token {
                req = req.continuation_token(token);
            }

            let resp = req
                .send()
                .await
                .map_err(|e| Error::Storage(format!("S3 list_objects_v2 failed: {e}")))?;

            for obj in resp.contents() {
                if ids.len() >= limit {
                    return Ok(ids);
                }

                if let Some(key) = obj.key() {
                    // Extract UUID from key: {prefix}/{agent_id}/{uuid}.json
                    if let Some(filename) = key.rsplit('/').next() {
                        if let Some(uuid_str) = filename.strip_suffix(".json") {
                            if let Ok(uuid) = Uuid::parse_str(uuid_str) {
                                ids.push(uuid);
                            }
                        }
                    }
                }
            }

            if ids.len() >= limit {
                return Ok(ids);
            }

            match resp.next_continuation_token() {
                Some(token) if resp.is_truncated() == Some(true) => {
                    continuation_token = Some(token.to_string());
                }
                _ => break,
            }
        }

        Ok(ids)
    }

    async fn delete_archived(&self, memory_id: Uuid) -> Result<()> {
        // Find the key first (we need the full path including agent_id).
        let prefix = self.bare_prefix();
        let target_suffix = format!("{memory_id}.json");

        let list_resp = self
            .client
            .list_objects_v2()
            .bucket(&self.config.bucket)
            .prefix(&prefix)
            .send()
            .await
            .map_err(|e| Error::Storage(format!("S3 list_objects_v2 failed: {e}")))?;

        let key = list_resp
            .contents()
            .iter()
            .filter_map(|obj| obj.key())
            .find(|k| k.ends_with(&target_suffix))
            .ok_or_else(|| {
                Error::NotFound(format!("archived memory {memory_id} not found in S3"))
            })?
            .to_string();

        self.client
            .delete_object()
            .bucket(&self.config.bucket)
            .key(&key)
            .send()
            .await
            .map_err(|e| Error::Storage(format!("S3 delete_object failed: {e}")))?;

        Ok(())
    }

    async fn is_archived(&self, memory_id: Uuid) -> Result<bool> {
        // We search for the key by listing with the bare prefix and checking
        // for a matching suffix. An alternative approach using head_object
        // would require knowing the full key (including agent_id).
        let prefix = self.bare_prefix();
        let target_suffix = format!("{memory_id}.json");

        let list_resp = self
            .client
            .list_objects_v2()
            .bucket(&self.config.bucket)
            .prefix(&prefix)
            .send()
            .await
            .map_err(|e| Error::Storage(format!("S3 list_objects_v2 failed: {e}")))?;

        Ok(list_resp
            .contents()
            .iter()
            .filter_map(|obj| obj.key())
            .any(|k| k.ends_with(&target_suffix)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::memory::{
        ConsolidationState, MemoryType, Scope, SourceType,
    };

    fn sample_config() -> ColdStorageConfig {
        ColdStorageConfig {
            bucket: "test-bucket".to_string(),
            prefix: "memories".to_string(),
            endpoint: None,
            region: "us-east-1".to_string(),
        }
    }

    fn sample_record(agent_id: &str) -> MemoryRecord {
        MemoryRecord {
            id: Uuid::now_v7(),
            agent_id: agent_id.to_string(),
            content: "The user prefers dark mode".to_string(),
            memory_type: MemoryType::Semantic,
            scope: Scope::Private,
            importance: 0.8,
            tags: vec!["preference".to_string(), "ui".to_string()],
            metadata: serde_json::json!({"source": "conversation"}),
            embedding: None,
            content_hash: vec![1, 2, 3],
            prev_hash: None,
            source_type: SourceType::Agent,
            source_id: None,
            consolidation_state: ConsolidationState::Raw,
            access_count: 0,
            org_id: None,
            thread_id: None,
            created_at: "2025-01-01T00:00:00Z".to_string(),
            updated_at: "2025-01-01T00:00:00Z".to_string(),
            last_accessed_at: None,
            expires_at: None,
            deleted_at: None,
            decay_rate: None,
            created_by: None,
            version: 1,
            prev_version_id: None,
            quarantined: false,
            quarantine_reason: None,
            decay_function: None,
        }
    }

    #[tokio::test]
    async fn test_archive_and_restore() {
        let storage = InMemoryColdStorage::new(sample_config());
        let record = sample_record("agent-1");
        let id = record.id;

        // Archive
        let result = storage.archive(&record).await.unwrap();
        assert_eq!(result.memory_id, id);
        assert!(result.size_bytes > 0);
        assert_eq!(
            result.s3_key,
            format!("memories/agent-1/{id}.json")
        );

        // Restore and verify round-trip fidelity
        let restored = storage.restore(id).await.unwrap();
        assert_eq!(restored.memory_id, id);
        assert_eq!(restored.record, record);
    }

    #[tokio::test]
    async fn test_list_archived() {
        let storage = InMemoryColdStorage::new(sample_config());

        let r1 = sample_record("agent-1");
        let r2 = sample_record("agent-1");
        let r3 = sample_record("agent-2");

        let id1 = r1.id;
        let id2 = r2.id;
        let id3 = r3.id;

        storage.archive(&r1).await.unwrap();
        storage.archive(&r2).await.unwrap();
        storage.archive(&r3).await.unwrap();

        // List all
        let all = storage.list_archived(None, 100).await.unwrap();
        assert_eq!(all.len(), 3);
        assert!(all.contains(&id1));
        assert!(all.contains(&id2));
        assert!(all.contains(&id3));

        // Filter by agent-1
        let agent1_ids = storage.list_archived(Some("agent-1"), 100).await.unwrap();
        assert_eq!(agent1_ids.len(), 2);
        assert!(agent1_ids.contains(&id1));
        assert!(agent1_ids.contains(&id2));

        // Filter by agent-2
        let agent2_ids = storage.list_archived(Some("agent-2"), 100).await.unwrap();
        assert_eq!(agent2_ids.len(), 1);
        assert!(agent2_ids.contains(&id3));

        // Limit
        let limited = storage.list_archived(None, 2).await.unwrap();
        assert_eq!(limited.len(), 2);
    }

    #[tokio::test]
    async fn test_delete_archived() {
        let storage = InMemoryColdStorage::new(sample_config());
        let record = sample_record("agent-1");
        let id = record.id;

        storage.archive(&record).await.unwrap();
        assert!(storage.is_archived(id).await.unwrap());

        // Delete
        storage.delete_archived(id).await.unwrap();
        assert!(!storage.is_archived(id).await.unwrap());

        // Restore should fail
        let err = storage.restore(id).await.unwrap_err();
        assert!(
            err.to_string().contains("not found"),
            "expected not-found error, got: {err}"
        );

        // Double-delete should fail
        let err = storage.delete_archived(id).await.unwrap_err();
        assert!(
            err.to_string().contains("not found"),
            "expected not-found error, got: {err}"
        );
    }

    #[tokio::test]
    async fn test_is_archived() {
        let storage = InMemoryColdStorage::new(sample_config());
        let record = sample_record("agent-1");
        let id = record.id;

        // Not archived yet
        assert!(!storage.is_archived(id).await.unwrap());

        // Archive
        storage.archive(&record).await.unwrap();
        assert!(storage.is_archived(id).await.unwrap());
    }
}
