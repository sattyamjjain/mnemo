//! Admin dashboard integration tests using axum's test utilities.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

use mnemo_core::embedding::NoopEmbedding;
use mnemo_core::index::usearch::UsearchIndex;
use mnemo_core::query::MnemoEngine;
use mnemo_core::storage::duckdb::DuckDbStorage;

fn create_test_engine() -> Arc<MnemoEngine> {
    let storage = Arc::new(DuckDbStorage::open_in_memory().unwrap());
    let index = Arc::new(UsearchIndex::new(128).unwrap());
    let embedding = Arc::new(NoopEmbedding::new(128));
    Arc::new(MnemoEngine::new(
        storage,
        index,
        embedding,
        "test-agent".to_string(),
        None,
    ))
}

#[tokio::test]
async fn test_admin_stats_endpoint() {
    let engine = create_test_engine();

    // Seed a memory so stats are non-trivial.
    engine
        .remember(mnemo_core::query::remember::RememberRequest {
            content: "Admin stats test memory".to_string(),
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

    let app = mnemo_admin::router(engine);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/admin/api/stats")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    // Verify all expected fields are present.
    assert!(
        json.get("memory_count").is_some(),
        "response must contain memory_count"
    );
    assert!(
        json.get("event_count").is_some(),
        "response must contain event_count"
    );
    assert!(
        json.get("agent_ids").is_some(),
        "response must contain agent_ids"
    );

    // With one seeded memory, memory_count must be at least 1.
    assert!(json["memory_count"].as_u64().unwrap() >= 1);

    // event_count must be a non-negative integer.
    assert!(json["event_count"].as_u64().is_some());

    // agent_ids must be an array containing at least one entry.
    assert!(json["agent_ids"].is_array());
    assert!(!json["agent_ids"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_admin_agents_endpoint() {
    let engine = create_test_engine();

    // Seed two memories with different agent IDs to verify distinct listing.
    engine
        .remember(mnemo_core::query::remember::RememberRequest {
            content: "Memory from agent-alpha".to_string(),
            agent_id: Some("agent-alpha".to_string()),
            memory_type: None,
            scope: None,
            importance: Some(0.7),
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

    engine
        .remember(mnemo_core::query::remember::RememberRequest {
            content: "Memory from agent-beta".to_string(),
            agent_id: Some("agent-beta".to_string()),
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

    let app = mnemo_admin::router(engine);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/admin/api/agents")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    // Response must be a JSON array.
    assert!(json.is_array(), "agents endpoint must return a JSON array");

    let agents = json.as_array().unwrap();

    // We seeded two distinct agent IDs.
    assert!(
        agents.len() >= 2,
        "expected at least 2 agents, got {}",
        agents.len()
    );

    // Verify both agent IDs are present (the array is sorted).
    let agent_strings: Vec<String> = agents
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert!(
        agent_strings.contains(&"agent-alpha".to_string()),
        "expected agent-alpha in list"
    );
    assert!(
        agent_strings.contains(&"agent-beta".to_string()),
        "expected agent-beta in list"
    );
}
