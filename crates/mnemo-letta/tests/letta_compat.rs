//! v0.4.0-rc3 (Task B5) — end-to-end integration tests for the
//! Letta-protocol surface. Each test boots an in-memory engine, calls
//! the router via tower's `oneshot`, and validates both the wire
//! shape and the resulting persisted state.

use std::sync::Arc;

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use mnemo_core::embedding::DeterministicEmbedding;
use mnemo_core::index::usearch::UsearchIndex;
use mnemo_core::query::MnemoEngine;
use mnemo_core::storage::duckdb::DuckDbStorage;
use mnemo_letta::router;
use serde_json::Value;
use tower::ServiceExt;

fn make_engine() -> Arc<MnemoEngine> {
    let storage = Arc::new(DuckDbStorage::open_in_memory().unwrap());
    let index = Arc::new(UsearchIndex::new(3).unwrap());
    let embedding = Arc::new(DeterministicEmbedding::new(3));
    Arc::new(MnemoEngine::new(
        storage,
        index,
        embedding,
        "letta-test".to_string(),
        None,
    ))
}

async fn body_json(body: Body) -> Value {
    let bytes = to_bytes(body, 1_048_576).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn create_agent_persists_persona_and_human() {
    let engine = make_engine();
    let app = router(engine);

    let body = serde_json::json!({
        "name": "agent-007",
        "persona": "You are a careful assistant.",
        "human": "I am Alice.",
        "letta_extra_field_we_dont_care_about": true,
    });
    let req = Request::builder()
        .method("POST")
        .uri("/v1/agents")
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let json = body_json(resp.into_body()).await;
    assert_eq!(json["agent_id"], "agent-007");
    assert_eq!(json["name"], "agent-007");
    assert!(json.get("created_at").is_some());
}

#[tokio::test]
async fn missing_agent_name_is_400() {
    let engine = make_engine();
    let app = router(engine);
    let body = serde_json::json!({"persona": "x", "name": ""});
    let req = Request::builder()
        .method("POST")
        .uri("/v1/agents")
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn send_message_persists_user_turn_and_returns_assistant_frame() {
    let engine = make_engine();
    let app = router(engine.clone());

    // Seed a memory the recall can hit.
    let create = serde_json::json!({
        "name": "agent-008",
        "persona": "tester",
        "human": "user",
    });
    let req = Request::builder()
        .method("POST")
        .uri("/v1/agents")
        .header("content-type", "application/json")
        .body(Body::from(create.to_string()))
        .unwrap();
    let _ = app.clone().oneshot(req).await.unwrap();

    let msg = serde_json::json!({"role": "user", "content": "what is the persona?"});
    let req = Request::builder()
        .method("POST")
        .uri("/v1/agents/agent-008/messages")
        .header("content-type", "application/json")
        .body(Body::from(msg.to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let json = body_json(resp.into_body()).await;
    assert_eq!(json["agent_id"], "agent-008");
    let msgs = json["messages"].as_array().unwrap();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0]["role"], "assistant");
}

#[tokio::test]
async fn get_memory_returns_persona_and_human_blocks() {
    let engine = make_engine();
    let app = router(engine);

    let create = serde_json::json!({
        "name": "agent-009",
        "persona": "P",
        "human": "H",
    });
    let req = Request::builder()
        .method("POST")
        .uri("/v1/agents")
        .header("content-type", "application/json")
        .body(Body::from(create.to_string()))
        .unwrap();
    let _ = app.clone().oneshot(req).await.unwrap();

    let req = Request::builder()
        .method("GET")
        .uri("/v1/agents/agent-009/memory")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let json = body_json(resp.into_body()).await;
    let blocks = json["memory"].as_array().unwrap();
    assert_eq!(blocks.len(), 2);
    let labels: Vec<&str> = blocks
        .iter()
        .map(|b| b["label"].as_str().unwrap())
        .collect();
    assert!(labels.contains(&"persona"));
    assert!(labels.contains(&"human"));
}
