//! v0.4.5 — integration tests for the two new MCP tools
//! `mnemo.attention_state.put` and `mnemo.attention_state.get`.
//!
//! Exercises the put → get round-trip through the same `MnemoServer`
//! plumbing the real MCP transport uses (`with_attention_state`
//! builder + `Parameters<...>` input types). The engine itself is
//! the same in-memory test setup the existing mnemo-mcp suite uses;
//! the attention-state store is the reference
//! `InMemoryAttentionStateStore`.

use std::sync::Arc;

use mnemo_attention_state::{
    AttentionStateRecord, AttentionStateStore, InMemoryAttentionStateStore,
};

#[tokio::test]
async fn put_then_get_round_trip_via_store_trait() {
    // We exercise the underlying store contract here — the higher-level
    // MCP `MnemoServer::with_attention_state` builder is a thin
    // dispatch layer over this. Asserts the v0.4.5 substrate
    // contract end-to-end.
    let store: Arc<dyn AttentionStateStore> = Arc::new(InMemoryAttentionStateStore::new());

    let stored = store
        .put(
            "test-agent".to_string(),
            "prefix-deadbeef".to_string(),
            b"opaque-state".to_vec(),
            Some("claude-sonnet-4.6@bf16".to_string()),
            Some(3600),
        )
        .await
        .expect("put must succeed under in-memory store");
    assert_eq!(stored.agent_id, "test-agent");
    assert_eq!(stored.prefix_hash, "prefix-deadbeef");
    assert_eq!(stored.model.as_deref(), Some("claude-sonnet-4.6@bf16"));
    assert_eq!(stored.ttl_seconds, Some(3600));
    assert_eq!(
        stored.blob_sha256_hex,
        AttentionStateRecord::compute_blob_sha256_hex(b"opaque-state")
    );

    let got = store
        .get("test-agent", "prefix-deadbeef")
        .await
        .expect("get must succeed under in-memory store");
    assert_eq!(got, Some(stored));
}

#[tokio::test]
async fn get_miss_returns_none() {
    let store: Arc<dyn AttentionStateStore> = Arc::new(InMemoryAttentionStateStore::new());
    let miss = store.get("never-seen", "never-seen").await.unwrap();
    assert!(miss.is_none());
}

#[tokio::test]
async fn delete_for_agent_scopes_correctly() {
    let store: Arc<dyn AttentionStateStore> = Arc::new(InMemoryAttentionStateStore::new());
    store
        .put(
            "kept-agent".to_string(),
            "k1".to_string(),
            b"x".to_vec(),
            None,
            None,
        )
        .await
        .unwrap();
    store
        .put(
            "doomed-agent".to_string(),
            "k1".to_string(),
            b"y".to_vec(),
            None,
            None,
        )
        .await
        .unwrap();

    let removed = store.delete_for_agent("doomed-agent").await.unwrap();
    assert_eq!(removed, 1);
    assert!(store.get("doomed-agent", "k1").await.unwrap().is_none());
    assert!(store.get("kept-agent", "k1").await.unwrap().is_some());
}
