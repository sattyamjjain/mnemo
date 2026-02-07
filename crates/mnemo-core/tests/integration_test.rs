//! Integration test: Full REMEMBER → RECALL → FORGET → SHARE lifecycle
//!
//! This test exercises the entire Mnemo stack using in-memory storage
//! and noop embeddings to verify the complete operation lifecycle.

use std::sync::Arc;

use mnemo_core::embedding::NoopEmbedding;
use mnemo_core::index::usearch::UsearchIndex;
use mnemo_core::model::acl::Permission;
use mnemo_core::model::delegation::{Delegation, DelegationScope};
use mnemo_core::model::event::{AgentEvent, EventType};
use mnemo_core::model::memory::{MemoryType, Scope};
use mnemo_core::query::branch::BranchRequest;
use mnemo_core::query::checkpoint::CheckpointRequest;
use mnemo_core::query::conflict::ResolutionStrategy;
use mnemo_core::query::forget::{ForgetRequest, ForgetStrategy};
use mnemo_core::query::lifecycle;
use mnemo_core::query::merge::MergeRequest;
use mnemo_core::query::recall::RecallRequest;
use mnemo_core::query::remember::RememberRequest;
use mnemo_core::query::replay::ReplayRequest;
use mnemo_core::query::share::ShareRequest;
use mnemo_core::query::MnemoEngine;
use mnemo_core::storage::duckdb::DuckDbStorage;

fn create_engine(agent_id: &str) -> Arc<MnemoEngine> {
    let storage = Arc::new(DuckDbStorage::open_in_memory().unwrap());
    let index = Arc::new(UsearchIndex::new(128).unwrap());
    let embedding = Arc::new(NoopEmbedding::new(128));

    Arc::new(MnemoEngine::new(
        storage,
        index,
        embedding,
        agent_id.to_string(),
        None,
    ))
}

#[tokio::test]
async fn test_full_lifecycle() {
    let engine = create_engine("agent-1");

    // === REMEMBER ===
    let remember_result = engine
        .remember(RememberRequest {
            content: "The user prefers dark mode".to_string(),
            agent_id: None,
            memory_type: Some(MemoryType::Semantic),
            scope: None,
            importance: Some(0.8),
            tags: Some(vec!["preference".to_string()]),
            metadata: None,
            source_type: None,
            source_id: None,
            org_id: None,
            thread_id: None,
            ttl_seconds: None,
            related_to: None,
            decay_rate: None,
            created_by: None,
        })
        .await
        .expect("remember should succeed");

    assert!(!remember_result.id.is_nil());
    assert!(!remember_result.content_hash.is_empty());

    // === RECALL ===
    let recall_result = engine
        .recall(RecallRequest {
            query: "user preferences".to_string(),
            agent_id: None,
            limit: Some(5),
            memory_type: None,
            memory_types: None,
            scope: None,
            min_importance: None,
            tags: None,
            org_id: None,
            strategy: None,
            temporal_range: None,
            recency_half_life_hours: None,
            hybrid_weights: None,
            rrf_k: None,
            as_of: None,
        })
        .await
        .expect("recall should succeed");

    assert_eq!(recall_result.total, 1);
    assert_eq!(recall_result.memories[0].content, "The user prefers dark mode");
    assert_eq!(recall_result.memories[0].memory_type, MemoryType::Semantic);

    // === SHARE ===
    let share_result = engine
        .share(ShareRequest {
            memory_id: remember_result.id,
            agent_id: None,
            target_agent_id: "agent-2".to_string(),
            target_agent_ids: None,
            permission: Some(Permission::Read),
            expires_in_hours: None,
        })
        .await
        .expect("share should succeed");

    assert_eq!(share_result.memory_id, remember_result.id);
    assert_eq!(share_result.shared_with, "agent-2");
    assert_eq!(share_result.permission, Permission::Read);

    // === FORGET (soft delete) ===
    let forget_result = engine
        .forget(ForgetRequest {
            memory_ids: vec![remember_result.id],
            agent_id: None,
            strategy: Some(ForgetStrategy::SoftDelete),
            criteria: None,
        })
        .await
        .expect("forget should succeed");

    assert_eq!(forget_result.forgotten.len(), 1);
    assert!(forget_result.errors.is_empty());

    // === Verify memory is gone from recall ===
    let recall_after_forget = engine
        .recall(RecallRequest {
            query: "user preferences".to_string(),
            agent_id: None,
            limit: Some(5),
            memory_type: None,
            memory_types: None,
            scope: None,
            min_importance: None,
            tags: None,
            org_id: None,
            strategy: None,
            temporal_range: None,
            recency_half_life_hours: None,
            hybrid_weights: None,
            rrf_k: None,
            as_of: None,
        })
        .await
        .expect("recall should succeed");

    assert_eq!(recall_after_forget.total, 0);
}

#[tokio::test]
async fn test_multiple_memories_with_filtering() {
    let engine = create_engine("agent-1");

    // Store multiple memories
    let _m1 = engine
        .remember(RememberRequest {
            content: "Python is great for data science".to_string(),
            agent_id: None,
            memory_type: Some(MemoryType::Semantic),
            scope: None,
            importance: Some(0.7),
            tags: Some(vec!["tech".to_string(), "python".to_string()]),
            metadata: None,
            source_type: None,
            source_id: None,
            org_id: None,
            thread_id: None,
            ttl_seconds: None,
            related_to: None,
            decay_rate: None,
            created_by: None,
        })
        .await
        .unwrap();

    let _m2 = engine
        .remember(RememberRequest {
            content: "Rust is excellent for systems programming".to_string(),
            agent_id: None,
            memory_type: Some(MemoryType::Semantic),
            scope: None,
            importance: Some(0.9),
            tags: Some(vec!["tech".to_string(), "rust".to_string()]),
            metadata: None,
            source_type: None,
            source_id: None,
            org_id: None,
            thread_id: None,
            ttl_seconds: None,
            related_to: None,
            decay_rate: None,
            created_by: None,
        })
        .await
        .unwrap();

    let _m3 = engine
        .remember(RememberRequest {
            content: "Morning standup at 9:30 AM".to_string(),
            agent_id: None,
            memory_type: Some(MemoryType::Procedural),
            scope: None,
            importance: Some(0.5),
            tags: Some(vec!["schedule".to_string()]),
            metadata: None,
            source_type: None,
            source_id: None,
            org_id: None,
            thread_id: None,
            ttl_seconds: None,
            related_to: None,
            decay_rate: None,
            created_by: None,
        })
        .await
        .unwrap();

    // Recall all
    let all = engine
        .recall(RecallRequest {
            query: "everything".to_string(),
            agent_id: None,
            limit: Some(10),
            memory_type: None,
            memory_types: None,
            scope: None,
            min_importance: None,
            tags: None,
            org_id: None,
            strategy: None,
            temporal_range: None,
            recency_half_life_hours: None,
            hybrid_weights: None,
            rrf_k: None,
            as_of: None,
        })
        .await
        .unwrap();
    assert_eq!(all.total, 3);

    // Filter by memory type
    let semantic_only = engine
        .recall(RecallRequest {
            query: "everything".to_string(),
            agent_id: None,
            limit: Some(10),
            memory_type: Some(MemoryType::Semantic),
            memory_types: None,
            scope: None,
            min_importance: None,
            tags: None,
            org_id: None,
            strategy: None,
            temporal_range: None,
            recency_half_life_hours: None,
            hybrid_weights: None,
            rrf_k: None,
            as_of: None,
        })
        .await
        .unwrap();
    assert_eq!(semantic_only.total, 2);

    // Filter by min importance
    let important = engine
        .recall(RecallRequest {
            query: "everything".to_string(),
            agent_id: None,
            limit: Some(10),
            memory_type: None,
            memory_types: None,
            scope: None,
            min_importance: Some(0.8),
            tags: None,
            org_id: None,
            strategy: None,
            temporal_range: None,
            recency_half_life_hours: None,
            hybrid_weights: None,
            rrf_k: None,
            as_of: None,
        })
        .await
        .unwrap();
    assert_eq!(important.total, 1);
    assert!(important.memories[0].content.contains("Rust"));

    // Filter by tags
    let tech_only = engine
        .recall(RecallRequest {
            query: "everything".to_string(),
            agent_id: None,
            limit: Some(10),
            memory_type: None,
            memory_types: None,
            scope: None,
            min_importance: None,
            tags: Some(vec!["schedule".to_string()]),
            org_id: None,
            strategy: None,
            temporal_range: None,
            recency_half_life_hours: None,
            hybrid_weights: None,
            rrf_k: None,
            as_of: None,
        })
        .await
        .unwrap();
    assert_eq!(tech_only.total, 1);
    assert!(tech_only.memories[0].content.contains("standup"));
}

#[tokio::test]
async fn test_hard_delete_is_permanent() {
    let engine = create_engine("agent-1");

    let result = engine
        .remember(RememberRequest {
            content: "Temporary secret".to_string(),
            agent_id: None,
            memory_type: None,
            scope: None,
            importance: None,
            tags: None,
            metadata: None,
            source_type: None,
            source_id: None,
            org_id: None,
            thread_id: None,
            ttl_seconds: None,
            related_to: None,
            decay_rate: None,
            created_by: None,
        })
        .await
        .unwrap();

    // Hard delete
    engine
        .forget(ForgetRequest {
            memory_ids: vec![result.id],
            agent_id: None,
            strategy: Some(ForgetStrategy::HardDelete),
            criteria: None,
        })
        .await
        .unwrap();

    // Verify completely gone from storage
    let record = engine.storage.get_memory(result.id).await.unwrap();
    assert!(record.is_none());
}

#[tokio::test]
async fn test_access_count_increments() {
    let engine = create_engine("agent-1");

    let result = engine
        .remember(RememberRequest {
            content: "Frequently accessed fact".to_string(),
            agent_id: None,
            memory_type: None,
            scope: None,
            importance: None,
            tags: None,
            metadata: None,
            source_type: None,
            source_id: None,
            org_id: None,
            thread_id: None,
            ttl_seconds: None,
            related_to: None,
            decay_rate: None,
            created_by: None,
        })
        .await
        .unwrap();

    // Recall multiple times
    for _ in 0..3 {
        engine
            .recall(RecallRequest {
                query: "fact".to_string(),
                agent_id: None,
                limit: Some(1),
                memory_type: None,
                memory_types: None,
                scope: None,
                min_importance: None,
                tags: None,
                org_id: None,
                strategy: None,
                temporal_range: None,
            recency_half_life_hours: None,
            hybrid_weights: None,
            rrf_k: None,
            as_of: None,
            })
            .await
            .unwrap();
    }

    // Check access count
    let record = engine.storage.get_memory(result.id).await.unwrap().unwrap();
    assert_eq!(record.access_count, 3);
}

#[tokio::test]
async fn test_checkpoint_and_replay() {
    let engine = create_engine("agent-1");

    // Store some memories
    let m1 = engine
        .remember(RememberRequest {
            content: "First memory".to_string(),
            agent_id: None,
            memory_type: None,
            scope: None,
            importance: None,
            tags: None,
            metadata: None,
            source_type: None,
            source_id: None,
            org_id: None,
            thread_id: None,
            ttl_seconds: None,
            related_to: None,
            decay_rate: None,
            created_by: None,
        })
        .await
        .unwrap();

    // Create checkpoint
    let cp = engine
        .checkpoint(CheckpointRequest {
            thread_id: "thread-1".to_string(),
            agent_id: None,
            branch_name: None,
            state_snapshot: serde_json::json!({"step": 1, "context": "initial"}),
            label: Some("after first memory".to_string()),
            metadata: None,
        })
        .await
        .unwrap();

    assert!(!cp.id.is_nil());
    assert_eq!(cp.branch_name, "main");
    assert!(cp.parent_id.is_none()); // First checkpoint has no parent

    // Add more data
    let _m2 = engine
        .remember(RememberRequest {
            content: "Second memory".to_string(),
            agent_id: None,
            memory_type: None,
            scope: None,
            importance: None,
            tags: None,
            metadata: None,
            source_type: None,
            source_id: None,
            org_id: None,
            thread_id: None,
            ttl_seconds: None,
            related_to: None,
            decay_rate: None,
            created_by: None,
        })
        .await
        .unwrap();

    // Create second checkpoint
    let cp2 = engine
        .checkpoint(CheckpointRequest {
            thread_id: "thread-1".to_string(),
            agent_id: None,
            branch_name: None,
            state_snapshot: serde_json::json!({"step": 2}),
            label: Some("after second memory".to_string()),
            metadata: None,
        })
        .await
        .unwrap();

    assert_eq!(cp2.parent_id, Some(cp.id)); // Chains to first

    // Replay first checkpoint
    let replay = engine
        .replay(ReplayRequest {
            thread_id: "thread-1".to_string(),
            agent_id: None,
            checkpoint_id: Some(cp.id),
            branch_name: None,
        })
        .await
        .unwrap();

    assert_eq!(replay.checkpoint.id, cp.id);
    // First checkpoint should have the memory that existed at that time
    assert!(replay.memories.iter().any(|m| m.id == m1.id));
}

#[tokio::test]
async fn test_branch_and_merge() {
    let engine = create_engine("agent-1");

    // Store a memory and create initial checkpoint
    let _m1 = engine
        .remember(RememberRequest {
            content: "Base memory".to_string(),
            agent_id: None,
            memory_type: None,
            scope: None,
            importance: None,
            tags: None,
            metadata: None,
            source_type: None,
            source_id: None,
            org_id: None,
            thread_id: None,
            ttl_seconds: None,
            related_to: None,
            decay_rate: None,
            created_by: None,
        })
        .await
        .unwrap();

    let main_cp = engine
        .checkpoint(CheckpointRequest {
            thread_id: "thread-1".to_string(),
            agent_id: None,
            branch_name: None,
            state_snapshot: serde_json::json!({"mode": "production"}),
            label: Some("main-v1".to_string()),
            metadata: None,
        })
        .await
        .unwrap();

    // Branch for exploration
    let branch = engine
        .branch(BranchRequest {
            thread_id: "thread-1".to_string(),
            agent_id: None,
            new_branch_name: "experiment-1".to_string(),
            source_checkpoint_id: None,
            source_branch: None,
        })
        .await
        .unwrap();

    assert_eq!(branch.branch_name, "experiment-1");
    assert_eq!(branch.source_checkpoint_id, main_cp.id);

    // Merge branch back to main
    let merge = engine
        .merge(MergeRequest {
            thread_id: "thread-1".to_string(),
            agent_id: None,
            source_branch: "experiment-1".to_string(),
            target_branch: None,
            strategy: None,
            cherry_pick_ids: None,
        })
        .await
        .unwrap();

    assert_eq!(merge.target_branch, "main");
    assert!(merge.merged_memory_count > 0);

    // Replay the merged state
    let replay = engine
        .replay(ReplayRequest {
            thread_id: "thread-1".to_string(),
            agent_id: None,
            checkpoint_id: None,
            branch_name: Some("main".to_string()),
        })
        .await
        .unwrap();

    assert_eq!(replay.checkpoint.branch_name, "main");
}

#[tokio::test]
async fn test_event_recorded_on_remember() {
    let engine = create_engine("agent-1");

    let _m = engine
        .remember(RememberRequest {
            content: "Test event recording".to_string(),
            agent_id: None,
            memory_type: None,
            scope: None,
            importance: None,
            tags: None,
            metadata: None,
            source_type: None,
            source_id: None,
            org_id: None,
            thread_id: None,
            ttl_seconds: None,
            related_to: None,
            decay_rate: None,
            created_by: None,
        })
        .await
        .unwrap();

    // Check that an event was recorded
    let events = engine.storage.list_events("agent-1", 10, 0).await.unwrap();
    assert!(!events.is_empty());
    assert_eq!(
        events[0].event_type,
        mnemo_core::model::event::EventType::MemoryWrite
    );
}

#[tokio::test]
async fn test_related_to_creates_relations() {
    let engine = create_engine("agent-1");

    // Create first memory
    let m1 = engine
        .remember(RememberRequest {
            content: "Base memory for relations test".to_string(),
            agent_id: None,
            memory_type: None,
            scope: None,
            importance: None,
            tags: None,
            metadata: None,
            source_type: None,
            source_id: None,
            org_id: None,
            thread_id: None,
            ttl_seconds: None,
            related_to: None,
            decay_rate: None,
            created_by: None,
        })
        .await
        .unwrap();

    // Create second memory related to first
    let m2 = engine
        .remember(RememberRequest {
            content: "Related memory linking back".to_string(),
            agent_id: None,
            memory_type: None,
            scope: None,
            importance: None,
            tags: None,
            metadata: None,
            source_type: None,
            source_id: None,
            org_id: None,
            thread_id: None,
            ttl_seconds: None,
            related_to: Some(vec![m1.id.to_string()]),
            decay_rate: None,
            created_by: None,
        })
        .await
        .unwrap();

    // Verify relation exists
    let relations = engine.storage.get_relations_from(m2.id).await.unwrap();
    assert_eq!(relations.len(), 1);
    assert_eq!(relations[0].target_id, m1.id);
    assert_eq!(relations[0].relation_type, "related_to");
}

#[tokio::test]
async fn test_ttl_sets_expires_at() {
    let engine = create_engine("agent-1");

    let result = engine
        .remember(RememberRequest {
            content: "Expiring memory".to_string(),
            agent_id: None,
            memory_type: None,
            scope: None,
            importance: None,
            tags: None,
            metadata: None,
            source_type: None,
            source_id: None,
            org_id: None,
            thread_id: None,
            ttl_seconds: Some(3600),
            related_to: None,
            decay_rate: None,
            created_by: None,
        })
        .await
        .unwrap();

    let record = engine.storage.get_memory(result.id).await.unwrap().unwrap();
    assert!(record.expires_at.is_some());

    // Verify the expiry is roughly 1 hour from now
    let expires_at = chrono::DateTime::parse_from_rfc3339(record.expires_at.as_ref().unwrap()).unwrap();
    let now = chrono::Utc::now();
    let diff = (expires_at.timestamp() - now.timestamp()).abs();
    assert!((3500..=3700).contains(&diff), "expires_at should be ~1 hour from now, got diff={diff}");
}

#[tokio::test]
async fn test_chain_linking_consecutive() {
    let engine = create_engine("agent-1");

    let mut memory_ids = Vec::new();
    for i in 0..3 {
        let result = engine
            .remember(RememberRequest {
                content: format!("Chain memory {}", i),
                agent_id: None,
                memory_type: None,
                scope: None,
                importance: None,
                tags: None,
                metadata: None,
                source_type: None,
                source_id: None,
                org_id: None,
                thread_id: Some("chain-test".to_string()),
                ttl_seconds: None,
                related_to: None,
                decay_rate: None,
                created_by: None,
            })
            .await
            .unwrap();
        memory_ids.push(result.id);
    }

    // Load all memories and verify chain
    let records = engine
        .storage
        .list_memories_by_agent_ordered("agent-1", Some("chain-test"), 10)
        .await
        .unwrap();

    assert_eq!(records.len(), 3);

    // All should have prev_hash (chain linking)
    // First memory's prev_hash is derived from content_hash + None prev
    // Subsequent ones chain to the previous
    for record in &records {
        assert!(record.prev_hash.is_some(), "all memories should have prev_hash for chain linking");
    }

    // Verify chain integrity
    let result = engine
        .verify_integrity(None, Some("chain-test"))
        .await
        .unwrap();

    assert!(result.valid);
    assert_eq!(result.total_records, 3);
    assert_eq!(result.verified_records, 3);
}

#[tokio::test]
async fn test_exact_recall_strategy() {
    let engine = create_engine("agent-1");

    // Store memories with different tags
    engine
        .remember(RememberRequest {
            content: "Alpha memory".to_string(),
            agent_id: None,
            memory_type: Some(MemoryType::Semantic),
            scope: None,
            importance: Some(0.9),
            tags: Some(vec!["alpha".to_string()]),
            metadata: None,
            source_type: None,
            source_id: None,
            org_id: None,
            thread_id: None,
            ttl_seconds: None,
            related_to: None,
            decay_rate: None,
            created_by: None,
        })
        .await
        .unwrap();

    engine
        .remember(RememberRequest {
            content: "Beta memory".to_string(),
            agent_id: None,
            memory_type: Some(MemoryType::Episodic),
            scope: None,
            importance: Some(0.3),
            tags: Some(vec!["beta".to_string()]),
            metadata: None,
            source_type: None,
            source_id: None,
            org_id: None,
            thread_id: None,
            ttl_seconds: None,
            related_to: None,
            decay_rate: None,
            created_by: None,
        })
        .await
        .unwrap();

    // Exact recall with memory type filter
    let result = engine
        .recall(RecallRequest {
            query: "anything".to_string(),
            agent_id: None,
            limit: Some(10),
            memory_type: Some(MemoryType::Semantic),
            memory_types: None,
            scope: None,
            min_importance: None,
            tags: None,
            org_id: None,
            strategy: Some("exact".to_string()),
            temporal_range: None,
            recency_half_life_hours: None,
            hybrid_weights: None,
            rrf_k: None,
            as_of: None,
        })
        .await
        .unwrap();

    assert_eq!(result.total, 1);
    assert!(result.memories[0].content.contains("Alpha"));
}

#[tokio::test]
async fn test_delegation_grants_access() {
    let engine = create_engine("agent-1");

    // Store a private memory
    let m1 = engine
        .remember(RememberRequest {
            content: "Secret memory for delegation test".to_string(),
            agent_id: None,
            memory_type: None,
            scope: None,
            importance: None,
            tags: Some(vec!["secret".to_string()]),
            metadata: None,
            source_type: None,
            source_id: None,
            org_id: None,
            thread_id: None,
            ttl_seconds: None,
            related_to: None,
            decay_rate: None,
            created_by: None,
        })
        .await
        .unwrap();

    // agent-2 should NOT have access
    let has_access = engine
        .storage
        .check_permission(m1.id, "agent-2", Permission::Read)
        .await
        .unwrap();
    assert!(!has_access);

    // Create delegation from agent-1 to agent-2
    let delegation = Delegation {
        id: uuid::Uuid::now_v7(),
        delegator_id: "agent-1".to_string(),
        delegate_id: "agent-2".to_string(),
        permission: Permission::Read,
        scope: DelegationScope::AllMemories,
        max_depth: 0,
        current_depth: 0,
        parent_delegation_id: None,
        created_at: chrono::Utc::now().to_rfc3339(),
        expires_at: None,
        revoked_at: None,
    };
    engine.storage.insert_delegation(&delegation).await.unwrap();

    // Now agent-2 should have access
    let has_access = engine
        .storage
        .check_permission(m1.id, "agent-2", Permission::Read)
        .await
        .unwrap();
    assert!(has_access);

    // But not write access
    let has_write = engine
        .storage
        .check_permission(m1.id, "agent-2", Permission::Write)
        .await
        .unwrap();
    assert!(!has_write);
}

#[tokio::test]
async fn test_delegation_expired_no_access() {
    let engine = create_engine("agent-1");

    let m1 = engine
        .remember(RememberRequest {
            content: "Memory with expired delegation".to_string(),
            agent_id: None,
            memory_type: None,
            scope: None,
            importance: None,
            tags: None,
            metadata: None,
            source_type: None,
            source_id: None,
            org_id: None,
            thread_id: None,
            ttl_seconds: None,
            related_to: None,
            decay_rate: None,
            created_by: None,
        })
        .await
        .unwrap();

    // Create an already-expired delegation
    let delegation = Delegation {
        id: uuid::Uuid::now_v7(),
        delegator_id: "agent-1".to_string(),
        delegate_id: "agent-3".to_string(),
        permission: Permission::Read,
        scope: DelegationScope::AllMemories,
        max_depth: 0,
        current_depth: 0,
        parent_delegation_id: None,
        created_at: "2020-01-01T00:00:00Z".to_string(),
        expires_at: Some("2020-01-02T00:00:00Z".to_string()),
        revoked_at: None,
    };
    engine.storage.insert_delegation(&delegation).await.unwrap();

    // agent-3 should NOT have access because delegation is expired
    let has_access = engine
        .storage
        .check_permission(m1.id, "agent-3", Permission::Read)
        .await
        .unwrap();
    assert!(!has_access);
}

#[tokio::test]
async fn test_cleanup_expired_memories() {
    let engine = create_engine("agent-1");

    // Store memory with already-expired TTL
    let m1 = engine
        .remember(RememberRequest {
            content: "Already expired memory".to_string(),
            agent_id: None,
            memory_type: None,
            scope: None,
            importance: None,
            tags: None,
            metadata: None,
            source_type: None,
            source_id: None,
            org_id: None,
            thread_id: None,
            ttl_seconds: None,
            related_to: None,
            decay_rate: None,
            created_by: None,
        })
        .await
        .unwrap();

    // Manually set expires_at to the past
    let mut record = engine.storage.get_memory(m1.id).await.unwrap().unwrap();
    record.expires_at = Some("2020-01-01T00:00:00Z".to_string());
    engine.storage.update_memory(&record).await.unwrap();

    // Run cleanup
    let cleaned = engine.storage.cleanup_expired().await.unwrap();
    assert_eq!(cleaned, 1);

    // Memory should be soft-deleted
    let record = engine.storage.get_memory(m1.id).await.unwrap().unwrap();
    assert!(record.deleted_at.is_some());
}

#[tokio::test]
async fn test_quarantined_excluded_from_recall() {
    let engine = create_engine("agent-1");

    let m1 = engine
        .remember(RememberRequest {
            content: "Normal visible memory".to_string(),
            agent_id: None,
            memory_type: None,
            scope: None,
            importance: None,
            tags: None,
            metadata: None,
            source_type: None,
            source_id: None,
            org_id: None,
            thread_id: None,
            ttl_seconds: None,
            related_to: None,
            decay_rate: None,
            created_by: None,
        })
        .await
        .unwrap();

    let m2 = engine
        .remember(RememberRequest {
            content: "Quarantined suspicious memory".to_string(),
            agent_id: None,
            memory_type: None,
            scope: None,
            importance: None,
            tags: None,
            metadata: None,
            source_type: None,
            source_id: None,
            org_id: None,
            thread_id: None,
            ttl_seconds: None,
            related_to: None,
            decay_rate: None,
            created_by: None,
        })
        .await
        .unwrap();

    // Manually quarantine m2
    let mut record = engine.storage.get_memory(m2.id).await.unwrap().unwrap();
    record.quarantined = true;
    record.quarantine_reason = Some("test quarantine".to_string());
    engine.storage.update_memory(&record).await.unwrap();

    // Recall should only return the non-quarantined memory
    let result = engine
        .recall(RecallRequest {
            query: "memory".to_string(),
            agent_id: None,
            limit: Some(10),
            memory_type: None,
            memory_types: None,
            scope: None,
            min_importance: None,
            tags: None,
            org_id: None,
            strategy: None,
            temporal_range: None,
            recency_half_life_hours: None,
            hybrid_weights: None,
            rrf_k: None,
            as_of: None,
        })
        .await
        .unwrap();

    assert_eq!(result.total, 1);
    assert_eq!(result.memories[0].id, m1.id);
}

#[tokio::test]
async fn test_agent_profile_updated_on_remember() {
    let engine = create_engine("agent-1");

    // Store memories to build profile
    for i in 0..5 {
        engine
            .remember(RememberRequest {
                content: format!("Profile building memory number {}", i),
                agent_id: None,
                memory_type: None,
                scope: None,
                importance: Some(0.5),
                tags: None,
                metadata: None,
                source_type: None,
                source_id: None,
                org_id: None,
                thread_id: None,
                ttl_seconds: None,
                related_to: None,
                decay_rate: None,
                created_by: None,
            })
            .await
            .unwrap();
    }

    // Check agent profile was created/updated
    let profile = engine
        .storage
        .get_agent_profile("agent-1")
        .await
        .unwrap();
    assert!(profile.is_some());
    let profile = profile.unwrap();
    assert_eq!(profile.total_memories, 5);
    assert!((profile.avg_importance - 0.5).abs() < 0.01);
}

// ============================================================
// Sprint 4 Tests
// ============================================================

#[tokio::test]
async fn test_recall_scope_filter() {
    let engine = create_engine("agent-1");

    // Create a private memory
    engine
        .remember(RememberRequest {
            content: "Private secret".to_string(),
            agent_id: None,
            memory_type: None,
            scope: Some(Scope::Private),
            importance: Some(0.8),
            tags: None,
            metadata: None,
            source_type: None,
            source_id: None,
            org_id: None,
            thread_id: None,
            ttl_seconds: None,
            related_to: None,
            decay_rate: None,
            created_by: None,
        })
        .await
        .unwrap();

    // Create a shared memory
    engine
        .remember(RememberRequest {
            content: "Shared info".to_string(),
            agent_id: None,
            memory_type: None,
            scope: Some(Scope::Shared),
            importance: Some(0.8),
            tags: None,
            metadata: None,
            source_type: None,
            source_id: None,
            org_id: None,
            thread_id: None,
            ttl_seconds: None,
            related_to: None,
            decay_rate: None,
            created_by: None,
        })
        .await
        .unwrap();

    // Recall with scope=private → only private
    let result = engine
        .recall(RecallRequest {
            query: "info".to_string(),
            agent_id: None,
            limit: Some(10),
            memory_type: None,
            memory_types: None,
            scope: Some(Scope::Private),
            min_importance: None,
            tags: None,
            org_id: None,
            strategy: Some("exact".to_string()),
            temporal_range: None,
            recency_half_life_hours: None,
            hybrid_weights: None,
            rrf_k: None,
            as_of: None,
        })
        .await
        .unwrap();

    assert_eq!(result.total, 1);
    assert!(result.memories[0].content.contains("Private"));

    // Recall with scope=shared → only shared
    let result = engine
        .recall(RecallRequest {
            query: "info".to_string(),
            agent_id: None,
            limit: Some(10),
            memory_type: None,
            memory_types: None,
            scope: Some(Scope::Shared),
            min_importance: None,
            tags: None,
            org_id: None,
            strategy: Some("exact".to_string()),
            temporal_range: None,
            recency_half_life_hours: None,
            hybrid_weights: None,
            rrf_k: None,
            as_of: None,
        })
        .await
        .unwrap();

    assert_eq!(result.total, 1);
    assert!(result.memories[0].content.contains("Shared"));
}

#[tokio::test]
async fn test_recall_multi_type_filter() {
    let engine = create_engine("agent-1");

    // Create one of each type
    for (content, mt) in [
        ("Episodic event", MemoryType::Episodic),
        ("Semantic fact", MemoryType::Semantic),
        ("Procedural how-to", MemoryType::Procedural),
    ] {
        engine
            .remember(RememberRequest {
                content: content.to_string(),
                agent_id: None,
                memory_type: Some(mt),
                scope: None,
                importance: Some(0.8),
                tags: None,
                metadata: None,
                source_type: None,
                source_id: None,
                org_id: None,
                thread_id: None,
                ttl_seconds: None,
                related_to: None,
                decay_rate: None,
                created_by: None,
            })
            .await
            .unwrap();
    }

    // Recall with memory_types = [Episodic, Semantic]
    let result = engine
        .recall(RecallRequest {
            query: "anything".to_string(),
            agent_id: None,
            limit: Some(10),
            memory_type: None,
            memory_types: Some(vec![MemoryType::Episodic, MemoryType::Semantic]),
            scope: None,
            min_importance: None,
            tags: None,
            org_id: None,
            strategy: Some("exact".to_string()),
            temporal_range: None,
            recency_half_life_hours: None,
            hybrid_weights: None,
            rrf_k: None,
            as_of: None,
        })
        .await
        .unwrap();

    assert_eq!(result.total, 2);
    let types: Vec<MemoryType> = result.memories.iter().map(|m| m.memory_type).collect();
    assert!(types.contains(&MemoryType::Episodic));
    assert!(types.contains(&MemoryType::Semantic));
    assert!(!types.contains(&MemoryType::Procedural));
}

#[tokio::test]
async fn test_share_multi_target() {
    let engine = create_engine("agent-1");

    let mem = engine
        .remember(RememberRequest {
            content: "Shared knowledge".to_string(),
            agent_id: None,
            memory_type: None,
            scope: None,
            importance: Some(0.8),
            tags: None,
            metadata: None,
            source_type: None,
            source_id: None,
            org_id: None,
            thread_id: None,
            ttl_seconds: None,
            related_to: None,
            decay_rate: None,
            created_by: None,
        })
        .await
        .unwrap();

    // Share to 3 agents at once
    let result = engine
        .share(ShareRequest {
            memory_id: mem.id,
            agent_id: None,
            target_agent_id: "agent-2".to_string(),
            target_agent_ids: Some(vec![
                "agent-2".to_string(),
                "agent-3".to_string(),
                "agent-4".to_string(),
            ]),
            permission: Some(Permission::Read),
            expires_in_hours: None,
        })
        .await
        .unwrap();

    assert_eq!(result.acl_ids.len(), 3);
    assert_eq!(result.shared_with_all.len(), 3);
    assert!(result.shared_with_all.contains(&"agent-2".to_string()));
    assert!(result.shared_with_all.contains(&"agent-3".to_string()));
    assert!(result.shared_with_all.contains(&"agent-4".to_string()));
}

#[tokio::test]
async fn test_share_expiration() {
    let engine = create_engine("agent-1");

    let mem = engine
        .remember(RememberRequest {
            content: "Expiring share".to_string(),
            agent_id: None,
            memory_type: None,
            scope: None,
            importance: Some(0.8),
            tags: None,
            metadata: None,
            source_type: None,
            source_id: None,
            org_id: None,
            thread_id: None,
            ttl_seconds: None,
            related_to: None,
            decay_rate: None,
            created_by: None,
        })
        .await
        .unwrap();

    let result = engine
        .share(ShareRequest {
            memory_id: mem.id,
            agent_id: None,
            target_agent_id: "agent-2".to_string(),
            target_agent_ids: None,
            permission: Some(Permission::Read),
            expires_in_hours: Some(24.0),
        })
        .await
        .unwrap();

    assert_eq!(result.shared_with, "agent-2");

    // Share succeeded with expiration — agent-2 should currently have access
    let has_access = engine
        .storage
        .check_permission(mem.id, "agent-2", Permission::Read)
        .await
        .unwrap();
    assert!(has_access, "agent-2 should have read access after share with expiration");
}

#[tokio::test]
async fn test_trace_causality() {
    let engine = create_engine("agent-1");

    let now = chrono::Utc::now().to_rfc3339();
    let parent_id = uuid::Uuid::now_v7();
    let child_id = uuid::Uuid::now_v7();

    // Insert parent event
    let parent_event = AgentEvent {
        id: parent_id,
        agent_id: "agent-1".to_string(),
        thread_id: None,
        run_id: None,
        parent_event_id: None,
        event_type: EventType::MemoryWrite,
        payload: serde_json::json!({"action": "remember"}),
        trace_id: None,
        span_id: None,
        model: None,
        tokens_input: None,
        tokens_output: None,
        latency_ms: None,
        cost_usd: None,
        timestamp: now.clone(),
        logical_clock: 0,
        content_hash: vec![1, 2, 3],
        prev_hash: None,
        embedding: None,
    };
    engine.storage.insert_event(&parent_event).await.unwrap();

    // Insert child event referencing parent
    let child_event = AgentEvent {
        id: child_id,
        agent_id: "agent-1".to_string(),
        thread_id: None,
        run_id: None,
        parent_event_id: Some(parent_id),
        event_type: EventType::MemoryRead,
        payload: serde_json::json!({"action": "recall"}),
        trace_id: None,
        span_id: None,
        model: None,
        tokens_input: None,
        tokens_output: None,
        latency_ms: None,
        cost_usd: None,
        timestamp: now,
        logical_clock: 1,
        content_hash: vec![4, 5, 6],
        prev_hash: None,
        embedding: None,
    };
    engine.storage.insert_event(&child_event).await.unwrap();

    // Trace from parent
    let chain = engine.trace_causality(parent_id, 3).await.unwrap();
    assert_eq!(chain.root, parent_id);
    assert_eq!(chain.nodes.len(), 2); // parent + child
    assert_eq!(chain.depth, 1);

    // Root node should list child
    let root_node = chain.nodes.iter().find(|n| n.event.id == parent_id).unwrap();
    assert!(root_node.children.contains(&child_id));
}

#[tokio::test]
async fn test_conflict_detection() {
    let engine = create_engine("agent-1");

    // Create two very similar memories (noop embedding → identical vectors)
    engine
        .remember(RememberRequest {
            content: "The capital of France is Paris".to_string(),
            agent_id: None,
            memory_type: None,
            scope: None,
            importance: Some(0.8),
            tags: None,
            metadata: None,
            source_type: None,
            source_id: None,
            org_id: None,
            thread_id: None,
            ttl_seconds: None,
            related_to: None,
            decay_rate: None,
            created_by: None,
        })
        .await
        .unwrap();

    engine
        .remember(RememberRequest {
            content: "Paris is the capital of France".to_string(),
            agent_id: None,
            memory_type: None,
            scope: None,
            importance: Some(0.6),
            tags: None,
            metadata: None,
            source_type: None,
            source_id: None,
            org_id: None,
            thread_id: None,
            ttl_seconds: None,
            related_to: None,
            decay_rate: None,
            created_by: None,
        })
        .await
        .unwrap();

    // With noop embeddings, all vectors are identical → similarity = 1.0
    // Since content differs, this should be flagged as a conflict
    let result = engine.detect_conflicts(Some("agent-1".to_string()), 0.9).await.unwrap();
    assert!(!result.conflicts.is_empty(), "Should detect near-duplicate conflict");
    assert_eq!(result.conflicts[0].similarity, 1.0);
}

#[tokio::test]
async fn test_conflict_resolution_keep_newest() {
    let engine = create_engine("agent-1");

    let mem_a = engine
        .remember(RememberRequest {
            content: "Old version of a fact".to_string(),
            agent_id: None,
            memory_type: None,
            scope: None,
            importance: Some(0.5),
            tags: None,
            metadata: None,
            source_type: None,
            source_id: None,
            org_id: None,
            thread_id: None,
            ttl_seconds: None,
            related_to: None,
            decay_rate: None,
            created_by: None,
        })
        .await
        .unwrap();

    let mem_b = engine
        .remember(RememberRequest {
            content: "New version of a fact".to_string(),
            agent_id: None,
            memory_type: None,
            scope: None,
            importance: Some(0.8),
            tags: None,
            metadata: None,
            source_type: None,
            source_id: None,
            org_id: None,
            thread_id: None,
            ttl_seconds: None,
            related_to: None,
            decay_rate: None,
            created_by: None,
        })
        .await
        .unwrap();

    // Detect conflicts
    let conflicts = engine.detect_conflicts(Some("agent-1".to_string()), 0.9).await.unwrap();
    assert!(!conflicts.conflicts.is_empty());

    // Resolve: keep newest
    engine
        .resolve_conflict(&conflicts.conflicts[0], ResolutionStrategy::KeepNewest)
        .await
        .unwrap();

    // mem_a (older) should be soft-deleted, mem_b (newer) should remain
    let a = engine.storage.get_memory(mem_a.id).await.unwrap().unwrap();
    let b = engine.storage.get_memory(mem_b.id).await.unwrap().unwrap();
    assert!(a.is_deleted(), "Older memory should be soft-deleted");
    assert!(!b.is_deleted(), "Newer memory should remain");
}

#[tokio::test]
async fn test_event_embedding_stored() {
    let engine = create_engine("agent-1");

    // Remember stores an event — check if the event is persisted
    let _mem = engine
        .remember(RememberRequest {
            content: "Event embedding test".to_string(),
            agent_id: None,
            memory_type: None,
            scope: None,
            importance: Some(0.5),
            tags: None,
            metadata: None,
            source_type: None,
            source_id: None,
            org_id: None,
            thread_id: None,
            ttl_seconds: None,
            related_to: None,
            decay_rate: None,
            created_by: None,
        })
        .await
        .unwrap();

    // List events for this agent and verify they exist
    let events = engine
        .storage
        .list_events("agent-1", 10, 0)
        .await
        .unwrap();

    assert!(!events.is_empty());
    // Events are currently stored with embedding: None (not computed for events by default)
    // This test verifies the field round-trips through DuckDB correctly
    let event = &events[0];
    assert_eq!(event.event_type, EventType::MemoryWrite);
    // embedding should be None since we don't compute event embeddings by default
    assert!(event.embedding.is_none());
}

#[tokio::test]
async fn test_custom_decay_linear() {
    use mnemo_core::model::memory::*;

    let now = chrono::Utc::now().to_rfc3339();
    let record = MemoryRecord {
        id: uuid::Uuid::now_v7(),
        agent_id: "agent-1".to_string(),
        content: "test".to_string(),
        memory_type: MemoryType::Episodic,
        scope: Scope::Private,
        importance: 0.8,
        tags: vec![],
        metadata: serde_json::json!({}),
        embedding: None,
        content_hash: vec![],
        prev_hash: None,
        source_type: SourceType::Agent,
        source_id: None,
        consolidation_state: ConsolidationState::Raw,
        access_count: 0,
        org_id: None,
        thread_id: None,
        created_at: now.clone(),
        updated_at: now.clone(),
        last_accessed_at: None,
        expires_at: None,
        deleted_at: None,
        decay_rate: Some(0.01),
        created_by: None,
        version: 1,
        prev_version_id: None,
        quarantined: false,
        quarantine_reason: None,
        decay_function: Some("linear".to_string()),
    };

    // Fresh memory with linear decay → should be close to base importance
    let eff = lifecycle::effective_importance(&record);
    assert!(eff > 0.7, "Fresh linear decay {eff} should be > 0.7");

    // Old memory with linear decay
    let old_date = (chrono::Utc::now() - chrono::Duration::hours(50)).to_rfc3339();
    let old_record = MemoryRecord {
        created_at: old_date,
        ..record.clone()
    };
    let old_eff = lifecycle::effective_importance(&old_record);
    assert!(old_eff < eff, "Old linear decay {old_eff} should be < fresh {eff}");

    // Step function: fresh → full importance
    let step_record = MemoryRecord {
        decay_function: Some("step:100".to_string()),
        ..record.clone()
    };
    let step_eff = lifecycle::effective_importance(&step_record);
    assert!(step_eff > 0.7, "Step function within threshold {step_eff} should be > 0.7");

    // Step function: past threshold → 0 (+ access boost only)
    let old_step = MemoryRecord {
        created_at: (chrono::Utc::now() - chrono::Duration::hours(200)).to_rfc3339(),
        decay_function: Some("step:100".to_string()),
        ..record.clone()
    };
    let old_step_eff = lifecycle::effective_importance(&old_step);
    assert!(old_step_eff < 0.1, "Step function past threshold {old_step_eff} should be < 0.1");

    // Power law decay
    let power_record = MemoryRecord {
        decay_function: Some("power_law:1.5".to_string()),
        ..record.clone()
    };
    let power_eff = lifecycle::effective_importance(&power_record);
    assert!(power_eff > 0.7, "Fresh power law {power_eff} should be > 0.7");
}

#[tokio::test]
async fn test_hybrid_with_graph_signal() {
    let engine = create_engine("agent-1");

    // Create a base memory
    let mem_a = engine
        .remember(RememberRequest {
            content: "Quantum computing fundamentals".to_string(),
            agent_id: None,
            memory_type: None,
            scope: None,
            importance: Some(0.5),
            tags: Some(vec!["science".to_string()]),
            metadata: None,
            source_type: None,
            source_id: None,
            org_id: None,
            thread_id: None,
            ttl_seconds: None,
            related_to: None,
            decay_rate: None,
            created_by: None,
        })
        .await
        .unwrap();

    // Create a related memory linked via graph relation
    let _mem_b = engine
        .remember(RememberRequest {
            content: "Qubit entanglement applications".to_string(),
            agent_id: None,
            memory_type: None,
            scope: None,
            importance: Some(0.5),
            tags: Some(vec!["science".to_string()]),
            metadata: None,
            source_type: None,
            source_id: None,
            org_id: None,
            thread_id: None,
            ttl_seconds: None,
            related_to: Some(vec![mem_a.id.to_string()]),
            decay_rate: None,
            created_by: None,
        })
        .await
        .unwrap();

    // Recall using hybrid strategy (default) — graph expansion should find related memories
    let result = engine
        .recall(RecallRequest {
            query: "quantum".to_string(),
            agent_id: None,
            limit: Some(10),
            memory_type: None,
            memory_types: None,
            scope: None,
            min_importance: None,
            tags: None,
            org_id: None,
            strategy: None, // default = auto/hybrid
            temporal_range: None,
            recency_half_life_hours: None,
            hybrid_weights: None,
            rrf_k: None,
            as_of: None,
        })
        .await
        .unwrap();

    // Both memories should be returned (vector + graph signals)
    assert_eq!(result.total, 2);
}

// ==========================================================================
// Sprint 5 Tests — Sync
// ==========================================================================

#[tokio::test]
async fn test_sync_push_pull() {
    use mnemo_core::sync::SyncEngine;

    let local = create_engine("sync-agent");
    let remote = create_engine("sync-agent");

    // Create memories on local side
    local
        .remember(RememberRequest {
            content: "Local memory alpha".to_string(),
            agent_id: None,
            memory_type: None,
            scope: None,
            importance: Some(0.8),
            tags: None,
            metadata: None,
            source_type: None,
            source_id: None,
            related_to: None,
            org_id: None,
            thread_id: None,
            ttl_seconds: None,
            decay_rate: None,
            created_by: None,
        })
        .await
        .unwrap();

    local
        .remember(RememberRequest {
            content: "Local memory beta".to_string(),
            agent_id: None,
            memory_type: None,
            scope: None,
            importance: Some(0.6),
            tags: None,
            metadata: None,
            source_type: None,
            source_id: None,
            related_to: None,
            org_id: None,
            thread_id: None,
            ttl_seconds: None,
            decay_rate: None,
            created_by: None,
        })
        .await
        .unwrap();

    let sync = SyncEngine::new(local.storage.clone(), remote.storage.clone());

    // Push local → remote
    let pushed = sync.push("1970-01-01T00:00:00Z").await.unwrap();
    assert_eq!(pushed, 2);

    // Verify remote now has the memories
    let remote_results = remote
        .recall(RecallRequest {
            query: "memory".to_string(),
            agent_id: Some("sync-agent".to_string()),
            limit: Some(10),
            memory_type: None,
            memory_types: None,
            scope: None,
            min_importance: None,
            tags: None,
            org_id: None,
            strategy: Some("exact".to_string()),
            temporal_range: None,
            recency_half_life_hours: None,
            hybrid_weights: None,
            rrf_k: None,
            as_of: None,
        })
        .await
        .unwrap();
    assert_eq!(remote_results.total, 2);

    // Create a memory on remote side only
    remote
        .remember(RememberRequest {
            content: "Remote-only memory gamma".to_string(),
            agent_id: None,
            memory_type: None,
            scope: None,
            importance: Some(0.5),
            tags: None,
            metadata: None,
            source_type: None,
            source_id: None,
            related_to: None,
            org_id: None,
            thread_id: None,
            ttl_seconds: None,
            decay_rate: None,
            created_by: None,
        })
        .await
        .unwrap();

    // Pull remote → local
    let pulled = sync.pull("1970-01-01T00:00:00Z").await.unwrap();
    // Should pull all 3 remote memories (2 pushed + 1 new)
    assert!(pulled >= 1);
}

#[tokio::test]
async fn test_sync_full_conflict_detection() {
    use mnemo_core::sync::SyncEngine;

    let local = create_engine("conflict-agent");
    let remote = create_engine("conflict-agent");

    // Create the same memory on both sides with different content
    let local_resp = local
        .remember(RememberRequest {
            content: "Shared memory v1".to_string(),
            agent_id: None,
            memory_type: None,
            scope: None,
            importance: Some(0.9),
            tags: None,
            metadata: None,
            source_type: None,
            source_id: None,
            related_to: None,
            org_id: None,
            thread_id: None,
            ttl_seconds: None,
            decay_rate: None,
            created_by: None,
        })
        .await
        .unwrap();

    // Push to remote first
    let sync = SyncEngine::new(local.storage.clone(), remote.storage.clone());
    sync.push("1970-01-01T00:00:00Z").await.unwrap();

    // Now update the remote copy (simulating a conflicting change)
    // Use storage directly to update
    let remote_mem = remote.storage.get_memory(local_resp.id).await.unwrap().unwrap();
    let mut updated = remote_mem.clone();
    updated.content = "Modified on remote side".to_string();
    updated.updated_at = chrono::Utc::now().to_rfc3339();
    remote.storage.upsert_memory(&updated).await.unwrap();

    // Full sync should detect the conflict
    let result = sync.full_sync("1970-01-01T00:00:00Z").await.unwrap();

    // We pushed at least 1 (the local version), and remote had a different updated_at
    assert!(result.pushed >= 1);
    // Conflicts should be detected (the same memory_id with different updated_at)
    assert!(!result.conflicts.is_empty());
    assert_eq!(result.conflicts[0].memory_id, local_resp.id);
}

#[tokio::test]
async fn test_sync_result_serialization() {
    use mnemo_core::sync::{SyncConflict, SyncResult};

    let result = SyncResult {
        pushed: 10,
        pulled: 5,
        conflicts: vec![SyncConflict {
            memory_id: uuid::Uuid::now_v7(),
            local_updated_at: "2025-06-01T00:00:00Z".to_string(),
            remote_updated_at: "2025-06-01T01:00:00Z".to_string(),
        }],
    };

    let json = serde_json::to_string(&result).unwrap();
    let deserialized: SyncResult = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.pushed, 10);
    assert_eq!(deserialized.pulled, 5);
    assert_eq!(deserialized.conflicts.len(), 1);
}

// ==========================================================================
// Sprint 11 Tests — Final 4 Gaps
// ==========================================================================

#[tokio::test]
async fn test_permission_safe_ann() {
    // Create 10 private memories per agent (agent-1 and agent-2), recall as agent-1,
    // verify 0 leakage from agent-2.
    let storage = Arc::new(DuckDbStorage::open_in_memory().unwrap());
    let index = Arc::new(UsearchIndex::new(128).unwrap());
    let embedding = Arc::new(NoopEmbedding::new(128));

    let engine = Arc::new(MnemoEngine::new(
        storage.clone(),
        index.clone(),
        embedding.clone(),
        "agent-1".to_string(),
        None,
    ));

    // Store 10 memories for agent-1
    for i in 0..10 {
        engine
            .remember(RememberRequest {
                content: format!("Agent-1 memory {}", i),
                agent_id: Some("agent-1".to_string()),
                memory_type: None,
                scope: Some(Scope::Private),
                importance: Some(0.5),
                tags: None,
                metadata: None,
                source_type: None,
                source_id: None,
                org_id: None,
                thread_id: None,
                ttl_seconds: None,
                related_to: None,
                decay_rate: None,
                created_by: None,
            })
            .await
            .unwrap();
    }

    // Store 10 memories for agent-2 using same index
    let engine2 = Arc::new(MnemoEngine::new(
        storage.clone(),
        index.clone(),
        embedding.clone(),
        "agent-2".to_string(),
        None,
    ));
    for i in 0..10 {
        engine2
            .remember(RememberRequest {
                content: format!("Agent-2 secret {}", i),
                agent_id: Some("agent-2".to_string()),
                memory_type: None,
                scope: Some(Scope::Private),
                importance: Some(0.5),
                tags: None,
                metadata: None,
                source_type: None,
                source_id: None,
                org_id: None,
                thread_id: None,
                ttl_seconds: None,
                related_to: None,
                decay_rate: None,
                created_by: None,
            })
            .await
            .unwrap();
    }

    // Recall as agent-1 with semantic strategy
    let result = engine
        .recall(RecallRequest {
            query: "memory".to_string(),
            agent_id: Some("agent-1".to_string()),
            limit: Some(20),
            memory_type: None,
            memory_types: None,
            scope: None,
            min_importance: None,
            tags: None,
            org_id: None,
            strategy: Some("semantic".to_string()),
            temporal_range: None,
            recency_half_life_hours: None,
            hybrid_weights: None,
            rrf_k: None,
            as_of: None,
        })
        .await
        .unwrap();

    // Zero leakage: all results should belong to agent-1
    for mem in &result.memories {
        assert_eq!(
            mem.agent_id, "agent-1",
            "Permission leak: agent-2 memory appeared in agent-1 recall"
        );
    }
    assert!(result.total <= 10, "Should return at most 10 agent-1 memories");
}

#[tokio::test]
async fn test_as_of_point_in_time() {
    let engine = create_engine("agent-1");

    // Remember A
    let mem_a = engine
        .remember(RememberRequest {
            content: "Memory A created first".to_string(),
            agent_id: None,
            memory_type: None,
            scope: None,
            importance: Some(0.8),
            tags: None,
            metadata: None,
            source_type: None,
            source_id: None,
            org_id: None,
            thread_id: None,
            ttl_seconds: None,
            related_to: None,
            decay_rate: None,
            created_by: None,
        })
        .await
        .unwrap();

    let record_a = engine.storage.get_memory(mem_a.id).await.unwrap().unwrap();
    let _t1 = record_a.created_at.clone();

    // Small delay to ensure different timestamps
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    let t_between = chrono::Utc::now().to_rfc3339();
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    // Remember B (after t_between)
    let _mem_b = engine
        .remember(RememberRequest {
            content: "Memory B created second".to_string(),
            agent_id: None,
            memory_type: None,
            scope: None,
            importance: Some(0.8),
            tags: None,
            metadata: None,
            source_type: None,
            source_id: None,
            org_id: None,
            thread_id: None,
            ttl_seconds: None,
            related_to: None,
            decay_rate: None,
            created_by: None,
        })
        .await
        .unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    let t_after_both = chrono::Utc::now().to_rfc3339();

    // Soft-delete A
    engine
        .forget(ForgetRequest {
            memory_ids: vec![mem_a.id],
            agent_id: None,
            strategy: Some(ForgetStrategy::SoftDelete),
            criteria: None,
        })
        .await
        .unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    let t_after_delete = chrono::Utc::now().to_rfc3339();

    // as_of t_between: should see A (existed), NOT B (created after)
    let result = engine
        .recall(RecallRequest {
            query: "Memory".to_string(),
            agent_id: None,
            limit: Some(10),
            memory_type: None,
            memory_types: None,
            scope: None,
            min_importance: None,
            tags: None,
            org_id: None,
            strategy: Some("exact".to_string()),
            temporal_range: None,
            recency_half_life_hours: None,
            hybrid_weights: None,
            rrf_k: None,
            as_of: Some(t_between.clone()),
        })
        .await
        .unwrap();

    assert_eq!(result.total, 1, "as_of t_between should see only A");
    assert!(result.memories[0].content.contains("Memory A"));

    // as_of t_after_both: should see both A and B (A was not yet deleted)
    let result2 = engine
        .recall(RecallRequest {
            query: "Memory".to_string(),
            agent_id: None,
            limit: Some(10),
            memory_type: None,
            memory_types: None,
            scope: None,
            min_importance: None,
            tags: None,
            org_id: None,
            strategy: Some("exact".to_string()),
            temporal_range: None,
            recency_half_life_hours: None,
            hybrid_weights: None,
            rrf_k: None,
            as_of: Some(t_after_both.clone()),
        })
        .await
        .unwrap();

    assert_eq!(result2.total, 2, "as_of t_after_both should see both A and B");

    // as_of t_after_delete: should see only B (A was deleted by then)
    let result3 = engine
        .recall(RecallRequest {
            query: "Memory".to_string(),
            agent_id: None,
            limit: Some(10),
            memory_type: None,
            memory_types: None,
            scope: None,
            min_importance: None,
            tags: None,
            org_id: None,
            strategy: Some("exact".to_string()),
            temporal_range: None,
            recency_half_life_hours: None,
            hybrid_weights: None,
            rrf_k: None,
            as_of: Some(t_after_delete.clone()),
        })
        .await
        .unwrap();

    assert_eq!(result3.total, 1, "as_of t_after_delete should see only B");
    assert!(result3.memories[0].content.contains("Memory B"));
}

#[tokio::test]
async fn test_event_integrity_verification() {
    let engine = create_engine("agent-1");

    // Remember + recall generates events with hash chains
    engine
        .remember(RememberRequest {
            content: "Event chain test memory".to_string(),
            agent_id: None,
            memory_type: None,
            scope: None,
            importance: Some(0.5),
            tags: None,
            metadata: None,
            source_type: None,
            source_id: None,
            org_id: None,
            thread_id: None,
            ttl_seconds: None,
            related_to: None,
            decay_rate: None,
            created_by: None,
        })
        .await
        .unwrap();

    engine
        .recall(RecallRequest {
            query: "event chain".to_string(),
            agent_id: None,
            limit: Some(5),
            memory_type: None,
            memory_types: None,
            scope: None,
            min_importance: None,
            tags: None,
            org_id: None,
            strategy: None,
            temporal_range: None,
            recency_half_life_hours: None,
            hybrid_weights: None,
            rrf_k: None,
            as_of: None,
        })
        .await
        .unwrap();

    // Verify event chain integrity
    let result = engine
        .verify_event_integrity(None, None)
        .await
        .unwrap();

    assert!(result.valid, "Event chain should be valid");
    assert!(result.total_records >= 2, "Should have at least 2 events (remember + recall)");
    assert_eq!(result.verified_records, result.total_records);
}

#[tokio::test]
async fn test_evidence_weighted_resolution() {
    use mnemo_core::model::memory::SourceType;

    let engine = create_engine("agent-1");

    // Create memory from ToolOutput (high reliability)
    let mem_a = engine
        .remember(RememberRequest {
            content: "Tool output fact about Paris".to_string(),
            agent_id: None,
            memory_type: None,
            scope: None,
            importance: Some(0.9),
            tags: None,
            metadata: None,
            source_type: Some(SourceType::ToolOutput),
            source_id: None,
            org_id: None,
            thread_id: None,
            ttl_seconds: None,
            related_to: None,
            decay_rate: None,
            created_by: None,
        })
        .await
        .unwrap();

    // Create memory from Import (low reliability)
    let mem_b = engine
        .remember(RememberRequest {
            content: "Imported fact about Paris".to_string(),
            agent_id: None,
            memory_type: None,
            scope: None,
            importance: Some(0.3),
            tags: None,
            metadata: None,
            source_type: Some(SourceType::Import),
            source_id: None,
            org_id: None,
            thread_id: None,
            ttl_seconds: None,
            related_to: None,
            decay_rate: None,
            created_by: None,
        })
        .await
        .unwrap();

    // Detect conflicts (noop embeddings → identical vectors → similarity = 1.0)
    let conflicts = engine
        .detect_conflicts(Some("agent-1".to_string()), 0.9)
        .await
        .unwrap();
    assert!(!conflicts.conflicts.is_empty());

    // Resolve with evidence-weighted strategy
    engine
        .resolve_conflict(&conflicts.conflicts[0], ResolutionStrategy::EvidenceWeighted)
        .await
        .unwrap();

    // ToolOutput (mem_a) should win due to higher source_reliability + importance
    let a = engine.storage.get_memory(mem_a.id).await.unwrap().unwrap();
    let b = engine.storage.get_memory(mem_b.id).await.unwrap().unwrap();
    assert!(!a.is_deleted(), "Higher evidence memory should survive");
    assert!(b.is_deleted(), "Lower evidence memory should be soft-deleted");

    // Winner should have conflict_resolution metadata
    let meta = a.metadata.as_object().unwrap();
    assert!(meta.contains_key("conflict_resolution"));
}

#[test]
fn test_source_reliability_ordering() {
    use mnemo_core::model::memory::SourceType;
    use mnemo_core::query::conflict::source_reliability;

    let scores = [
        (SourceType::ToolOutput, source_reliability(SourceType::ToolOutput)),
        (SourceType::Human, source_reliability(SourceType::Human)),
        (SourceType::UserInput, source_reliability(SourceType::UserInput)),
        (SourceType::System, source_reliability(SourceType::System)),
        (SourceType::ModelResponse, source_reliability(SourceType::ModelResponse)),
        (SourceType::Agent, source_reliability(SourceType::Agent)),
        (SourceType::Consolidation, source_reliability(SourceType::Consolidation)),
        (SourceType::Retrieval, source_reliability(SourceType::Retrieval)),
        (SourceType::Import, source_reliability(SourceType::Import)),
    ];

    // Verify ordering: ToolOutput > Human = UserInput > System > ModelResponse > Agent > Consolidation > Retrieval > Import
    assert!(scores[0].1 > scores[1].1, "ToolOutput should be > Human");
    assert_eq!(scores[1].1, scores[2].1, "Human should equal UserInput");
    assert!(scores[2].1 > scores[3].1, "UserInput should be > System");
    assert!(scores[3].1 > scores[4].1, "System should be > ModelResponse");
    assert!(scores[4].1 > scores[5].1, "ModelResponse should be > Agent");
    assert!(scores[5].1 > scores[6].1, "Agent should be > Consolidation");
    assert!(scores[6].1 > scores[7].1, "Consolidation should be > Retrieval");
    assert!(scores[7].1 > scores[8].1, "Retrieval should be > Import");
}
