//! REST API integration tests using axum's test utilities.

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
async fn test_rest_health() {
    let engine = create_test_engine();
    let app = mnemo_rest::router(engine);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "ok");
}

#[tokio::test]
async fn test_rest_remember_endpoint() {
    let engine = create_test_engine();
    let app = mnemo_rest::router(engine);

    let body = serde_json::json!({
        "content": "REST API test memory",
        "importance": 0.85
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/memories")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["id"].is_string());
    assert!(json["content_hash"].is_string());
}

#[tokio::test]
async fn test_rest_recall_endpoint() {
    let engine = create_test_engine();

    // First remember something
    engine
        .remember(mnemo_core::query::remember::RememberRequest {
            content: "Recall test content for REST".to_string(),
            agent_id: None,
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

    let app = mnemo_rest::router(engine);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/v1/memories?query=recall+test&strategy=exact")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["total"].as_u64().unwrap() >= 1);
    assert!(json["memories"].is_array());
}

#[tokio::test]
async fn test_rest_forget_endpoint() {
    let engine = create_test_engine();

    // Remember a memory first
    let mem = engine
        .remember(mnemo_core::query::remember::RememberRequest {
            content: "Memory to forget via REST".to_string(),
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

    let app = mnemo_rest::router(engine);

    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/v1/memories/{}", mem.id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["forgotten"].is_array());
    assert_eq!(json["forgotten"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn test_rest_get_memory_endpoint() {
    let engine = create_test_engine();

    let mem = engine
        .remember(mnemo_core::query::remember::RememberRequest {
            content: "Get by ID via REST".to_string(),
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

    let app = mnemo_rest::router(engine);

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/v1/memories/{}", mem.id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["content"], "Get by ID via REST");
    assert_eq!(json["id"], mem.id.to_string());
}

#[tokio::test]
async fn test_rest_verify_endpoint() {
    let engine = create_test_engine();

    // Remember a few things to build a chain
    engine
        .remember(mnemo_core::query::remember::RememberRequest {
            content: "Chain link 1".to_string(),
            agent_id: None,
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

    let app = mnemo_rest::router(engine);

    let body = serde_json::json!({
        "agent_id": "test-agent"
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/verify")
                .header("content-type", "application/json")
                .body(Body::from(serde_json::to_string(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json["valid"].as_bool().unwrap());
    assert_eq!(json["status"], "verified");
}

#[tokio::test]
async fn test_rest_not_found_memory() {
    let engine = create_test_engine();
    let app = mnemo_rest::router(engine);

    let fake_id = uuid::Uuid::now_v7();

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!("/v1/memories/{}", fake_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
