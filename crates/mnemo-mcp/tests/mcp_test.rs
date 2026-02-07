//! MCP server integration tests.
//!
//! Since rmcp's #[tool] macro generates private methods, these tests verify
//! server construction, ServerHandler impl, and engine integration through
//! the public MnemoEngine API that the MCP tools delegate to.

use std::sync::Arc;

use mnemo_core::embedding::NoopEmbedding;
use mnemo_core::index::usearch::UsearchIndex;
use mnemo_core::query::MnemoEngine;
use mnemo_core::query::recall::RecallRequest;
use mnemo_core::query::remember::RememberRequest;
use mnemo_core::storage::duckdb::DuckDbStorage;

use mnemo_mcp::server::MnemoServer;
use rmcp::ServerHandler;

fn create_server() -> (MnemoServer, Arc<MnemoEngine>) {
    let storage = Arc::new(DuckDbStorage::open_in_memory().unwrap());
    let index = Arc::new(UsearchIndex::new(128).unwrap());
    let embedding = Arc::new(NoopEmbedding::new(128));
    let engine = Arc::new(MnemoEngine::new(
        storage,
        index,
        embedding,
        "test-agent".to_string(),
        None,
    ));
    let server = MnemoServer::new(engine.clone());
    (server, engine)
}

#[tokio::test]
async fn test_server_construction() {
    let (server, _) = create_server();
    let info = server.get_info();
    assert_eq!(info.server_info.name, "mnemo");
    assert!(info.instructions.is_some());
    assert!(info.instructions.unwrap().contains("mnemo.remember"));
}

#[tokio::test]
async fn test_server_capabilities() {
    let (server, _) = create_server();
    let info = server.get_info();
    // Should have tools capability enabled
    assert!(info.capabilities.tools.is_some());
}

#[tokio::test]
async fn test_engine_remember_via_server_engine() {
    let (_, engine) = create_server();

    let result = engine
        .remember(RememberRequest {
            content: "Test memory from MCP context".to_string(),
            agent_id: None,
            memory_type: None,
            scope: None,
            importance: Some(0.7),
            tags: Some(vec!["mcp-test".to_string()]),
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

    assert!(!result.id.is_nil());
    assert!(!result.content_hash.is_empty());
}

#[tokio::test]
async fn test_engine_recall_via_server_engine() {
    let (_, engine) = create_server();

    // Store
    engine
        .remember(RememberRequest {
            content: "MCP recall test content".to_string(),
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

    // Recall
    let recall = engine
        .recall(RecallRequest {
            query: "recall test".to_string(),
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

    assert_eq!(recall.total, 1);
    assert!(recall.memories[0].content.contains("MCP recall test"));
}

#[tokio::test]
async fn test_engine_verify_via_server_engine() {
    let (_, engine) = create_server();

    // Store chained memories
    for i in 0..3 {
        engine
            .remember(RememberRequest {
                content: format!("Chained memory {} for MCP verify test", i),
                agent_id: None,
                memory_type: None,
                scope: None,
                importance: None,
                tags: None,
                metadata: None,
                source_type: None,
                source_id: None,
                org_id: None,
                thread_id: Some("mcp-verify-thread".to_string()),
                ttl_seconds: None,
                related_to: None,
                decay_rate: None,
                created_by: None,
            })
            .await
            .unwrap();
    }

    let result = engine
        .verify_integrity(None, Some("mcp-verify-thread"))
        .await
        .unwrap();

    assert!(result.valid);
    assert_eq!(result.total_records, 3);
    assert_eq!(result.verified_records, 3);
}
