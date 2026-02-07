use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

/// Constant-time comparison for hash values to prevent timing side-channels.
fn hashes_equal(a: &[u8], b: &[u8]) -> bool {
    a.ct_eq(b).into()
}

pub fn compute_content_hash(content: &str, agent_id: &str, timestamp: &str) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    hasher.update(agent_id.as_bytes());
    hasher.update(timestamp.as_bytes());
    hasher.finalize().to_vec()
}

pub fn compute_chain_hash(content_hash: &[u8], prev_hash: Option<&[u8]>) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(content_hash);
    if let Some(prev) = prev_hash {
        hasher.update(prev);
    }
    hasher.finalize().to_vec()
}

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::model::event::AgentEvent;
use crate::model::memory::MemoryRecord;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainVerificationResult {
    pub valid: bool,
    pub total_records: usize,
    pub verified_records: usize,
    pub first_broken_at: Option<Uuid>,
    pub error_message: Option<String>,
}

pub fn verify_chain(records: &[MemoryRecord]) -> ChainVerificationResult {
    if records.is_empty() {
        return ChainVerificationResult {
            valid: true,
            total_records: 0,
            verified_records: 0,
            first_broken_at: None,
            error_message: None,
        };
    }

    let mut verified = 0;

    for (i, record) in records.iter().enumerate() {
        // Verify content hash (constant-time comparison)
        let expected_hash = compute_content_hash(&record.content, &record.agent_id, &record.created_at);
        if !hashes_equal(&expected_hash, &record.content_hash) {
            return ChainVerificationResult {
                valid: false,
                total_records: records.len(),
                verified_records: verified,
                first_broken_at: Some(record.id),
                error_message: Some(format!("content hash mismatch at record {}", record.id)),
            };
        }

        // Verify chain linking (prev_hash)
        if i > 0 {
            let prev_record = &records[i - 1];
            let expected_chain = compute_chain_hash(&record.content_hash, Some(&prev_record.content_hash));
            if let Some(ref prev_hash) = record.prev_hash {
                if !hashes_equal(prev_hash, &expected_chain) {
                    return ChainVerificationResult {
                        valid: false,
                        total_records: records.len(),
                        verified_records: verified,
                        first_broken_at: Some(record.id),
                        error_message: Some(format!("chain hash mismatch at record {}", record.id)),
                    };
                }
            }
        }

        verified += 1;
    }

    ChainVerificationResult {
        valid: true,
        total_records: records.len(),
        verified_records: verified,
        first_broken_at: None,
        error_message: None,
    }
}

/// Verify the integrity of an ordered list of agent events.
/// Verifies that content_hash fields are non-empty and that
/// prev_hash chain linkage between consecutive events is valid.
/// Note: event content_hash is computed from the operation's source data
/// (memory content or query string), not from the event payload JSON,
/// so we verify it is present but do not recompute it.
pub fn verify_event_chain(events: &[AgentEvent]) -> ChainVerificationResult {
    if events.is_empty() {
        return ChainVerificationResult {
            valid: true,
            total_records: 0,
            verified_records: 0,
            first_broken_at: None,
            error_message: None,
        };
    }

    let mut verified = 0;

    for (i, event) in events.iter().enumerate() {
        // Verify content hash is present (non-empty)
        if event.content_hash.is_empty() {
            return ChainVerificationResult {
                valid: false,
                total_records: events.len(),
                verified_records: verified,
                first_broken_at: Some(event.id),
                error_message: Some(format!("event content hash is empty at {}", event.id)),
            };
        }

        // Verify chain linking (prev_hash)
        if i > 0 {
            let prev_event = &events[i - 1];
            let expected_chain = compute_chain_hash(&event.content_hash, Some(&prev_event.content_hash));
            if let Some(ref prev_hash) = event.prev_hash {
                if !hashes_equal(prev_hash, &expected_chain) {
                    return ChainVerificationResult {
                        valid: false,
                        total_records: events.len(),
                        verified_records: verified,
                        first_broken_at: Some(event.id),
                        error_message: Some(format!("event chain hash mismatch at {}", event.id)),
                    };
                }
            }
        }

        verified += 1;
    }

    ChainVerificationResult {
        valid: true,
        total_records: events.len(),
        verified_records: verified,
        first_broken_at: None,
        error_message: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_hash_deterministic() {
        let h1 = compute_content_hash("hello", "agent-1", "2025-01-01T00:00:00Z");
        let h2 = compute_content_hash("hello", "agent-1", "2025-01-01T00:00:00Z");
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 32); // SHA-256 = 32 bytes
    }

    #[test]
    fn test_content_hash_differs_with_different_input() {
        let h1 = compute_content_hash("hello", "agent-1", "2025-01-01T00:00:00Z");
        let h2 = compute_content_hash("world", "agent-1", "2025-01-01T00:00:00Z");
        let h3 = compute_content_hash("hello", "agent-2", "2025-01-01T00:00:00Z");
        let h4 = compute_content_hash("hello", "agent-1", "2025-01-02T00:00:00Z");
        assert_ne!(h1, h2);
        assert_ne!(h1, h3);
        assert_ne!(h1, h4);
    }

    #[test]
    fn test_chain_hash_without_prev() {
        let content_hash = compute_content_hash("test", "a", "t");
        let chain = compute_chain_hash(&content_hash, None);
        assert_eq!(chain.len(), 32);
    }

    #[test]
    fn test_chain_hash_with_prev() {
        let h1 = compute_content_hash("first", "a", "t1");
        let h2 = compute_content_hash("second", "a", "t2");
        let chain1 = compute_chain_hash(&h1, None);
        let chain2 = compute_chain_hash(&h2, Some(&chain1));
        assert_ne!(chain1, chain2);
    }

    #[test]
    fn test_verify_chain_valid() {
        use crate::model::memory::*;

        let mut records: Vec<MemoryRecord> = Vec::new();
        let agent_id = "agent-1";

        for i in 0..5 {
            let content = format!("memory content {i}");
            let timestamp = format!("2025-01-0{:01}T00:00:00Z", i + 1);
            let content_hash = compute_content_hash(&content, agent_id, &timestamp);
            let prev_hash = if i == 0 {
                Some(compute_chain_hash(&content_hash, None))
            } else {
                let prev_record = &records[i - 1];
                Some(compute_chain_hash(&content_hash, Some(&prev_record.content_hash)))
            };

            records.push(MemoryRecord {
                id: uuid::Uuid::now_v7(),
                agent_id: agent_id.to_string(),
                content,
                memory_type: MemoryType::Episodic,
                scope: Scope::Private,
                importance: 0.5,
                tags: vec![],
                metadata: serde_json::json!({}),
                embedding: None,
                content_hash,
                prev_hash,
                source_type: SourceType::Agent,
                source_id: None,
                consolidation_state: ConsolidationState::Raw,
                access_count: 0,
                org_id: None,
                thread_id: None,
                created_at: timestamp,
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
            });
        }

        let result = verify_chain(&records);
        assert!(result.valid);
        assert_eq!(result.total_records, 5);
        assert_eq!(result.verified_records, 5);
        assert!(result.first_broken_at.is_none());
    }

    #[test]
    fn test_verify_chain_tampered() {
        use crate::model::memory::*;

        let mut records: Vec<MemoryRecord> = Vec::new();
        let agent_id = "agent-1";

        for i in 0..3 {
            let content = format!("memory content {i}");
            let timestamp = format!("2025-01-0{:01}T00:00:00Z", i + 1);
            let content_hash = compute_content_hash(&content, agent_id, &timestamp);
            let prev_hash = if i == 0 {
                Some(compute_chain_hash(&content_hash, None))
            } else {
                let prev_record = &records[i - 1];
                Some(compute_chain_hash(&content_hash, Some(&prev_record.content_hash)))
            };

            records.push(MemoryRecord {
                id: uuid::Uuid::now_v7(),
                agent_id: agent_id.to_string(),
                content,
                memory_type: MemoryType::Episodic,
                scope: Scope::Private,
                importance: 0.5,
                tags: vec![],
                metadata: serde_json::json!({}),
                embedding: None,
                content_hash,
                prev_hash,
                source_type: SourceType::Agent,
                source_id: None,
                consolidation_state: ConsolidationState::Raw,
                access_count: 0,
                org_id: None,
                thread_id: None,
                created_at: timestamp,
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
            });
        }

        // Tamper with the second record's content (but not its hash)
        records[1].content = "TAMPERED CONTENT".to_string();

        let result = verify_chain(&records);
        assert!(!result.valid);
        assert_eq!(result.first_broken_at, Some(records[1].id));
        assert!(result.error_message.unwrap().contains("content hash mismatch"));
    }
}
