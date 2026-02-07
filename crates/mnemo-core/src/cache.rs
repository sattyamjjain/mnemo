//! In-memory LRU cache for memory records.
//!
//! Wraps StorageBackend lookups with an in-process cache. Does not require
//! Redis — uses a simple `HashMap` with TTL-based eviction.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use uuid::Uuid;

use crate::model::memory::MemoryRecord;

/// A simple in-process cache for memory records with TTL-based eviction.
pub struct MemoryCache {
    entries: Mutex<HashMap<Uuid, CacheEntry>>,
    ttl: Duration,
    max_entries: usize,
}

struct CacheEntry {
    record: MemoryRecord,
    inserted_at: Instant,
}

impl MemoryCache {
    /// Create a new cache with the given TTL and max entry count.
    pub fn new(ttl_seconds: u64, max_entries: usize) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            ttl: Duration::from_secs(ttl_seconds),
            max_entries,
        }
    }

    /// Get a cached record by ID. Returns None if not cached or expired.
    pub fn get(&self, id: Uuid) -> Option<MemoryRecord> {
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(entry) = entries.get(&id) {
            if entry.inserted_at.elapsed() < self.ttl {
                return Some(entry.record.clone());
            }
            // Expired — remove it
            entries.remove(&id);
        }
        None
    }

    /// Insert or update a record in the cache.
    pub fn put(&self, record: MemoryRecord) {
        let mut entries = self.entries.lock().unwrap_or_else(|e| e.into_inner());

        // Evict expired entries if we're at capacity
        if entries.len() >= self.max_entries {
            let now = Instant::now();
            entries.retain(|_, e| now.duration_since(e.inserted_at) < self.ttl);
        }

        // If still at capacity, evict oldest
        if entries.len() >= self.max_entries
            && let Some(&oldest_id) = entries
                .iter()
                .min_by_key(|(_, e)| e.inserted_at)
                .map(|(id, _)| id)
        {
            entries.remove(&oldest_id);
        }

        // If still at capacity after eviction attempts, skip insert to prevent unbounded growth
        if entries.len() >= self.max_entries && !entries.contains_key(&record.id) {
            return;
        }

        entries.insert(
            record.id,
            CacheEntry {
                record,
                inserted_at: Instant::now(),
            },
        );
    }

    /// Invalidate (remove) a cached record.
    pub fn invalidate(&self, id: Uuid) {
        self.entries.lock().unwrap_or_else(|e| e.into_inner()).remove(&id);
    }

    /// Clear all cached entries.
    pub fn clear(&self) {
        self.entries.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }

    /// Number of entries currently in cache.
    pub fn len(&self) -> usize {
        self.entries.lock().unwrap_or_else(|e| e.into_inner()).len()
    }

    /// Whether the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.lock().unwrap_or_else(|e| e.into_inner()).is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_record(id: Uuid) -> MemoryRecord {
        MemoryRecord {
            id,
            agent_id: "test".to_string(),
            content: format!("content-{id}"),
            memory_type: crate::model::memory::MemoryType::Episodic,
            scope: crate::model::memory::Scope::Private,
            importance: 0.5,
            tags: vec![],
            embedding: None,
            metadata: serde_json::Value::Null,
            source_type: crate::model::memory::SourceType::Agent,
            source_id: None,
            consolidation_state: crate::model::memory::ConsolidationState::Raw,
            access_count: 0,
            org_id: None,
            thread_id: None,
            content_hash: vec![],
            prev_hash: None,
            created_at: String::new(),
            updated_at: String::new(),
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

    #[test]
    fn test_cache_put_and_get() {
        let cache = MemoryCache::new(60, 100);
        let id = Uuid::now_v7();
        let record = make_record(id);

        cache.put(record.clone());
        let cached = cache.get(id).unwrap();
        assert_eq!(cached.id, id);
        assert_eq!(cached.content, record.content);
    }

    #[test]
    fn test_cache_miss() {
        let cache = MemoryCache::new(60, 100);
        assert!(cache.get(Uuid::now_v7()).is_none());
    }

    #[test]
    fn test_cache_invalidate() {
        let cache = MemoryCache::new(60, 100);
        let id = Uuid::now_v7();
        cache.put(make_record(id));
        assert!(cache.get(id).is_some());

        cache.invalidate(id);
        assert!(cache.get(id).is_none());
    }

    #[test]
    fn test_cache_max_entries() {
        let cache = MemoryCache::new(60, 2);

        let id1 = Uuid::now_v7();
        let id2 = Uuid::now_v7();
        let id3 = Uuid::now_v7();

        cache.put(make_record(id1));
        cache.put(make_record(id2));
        assert_eq!(cache.len(), 2);

        cache.put(make_record(id3));
        // One should have been evicted
        assert_eq!(cache.len(), 2);
    }
}
