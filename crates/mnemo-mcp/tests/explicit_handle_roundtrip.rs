//! The explicit-handle pattern, exercised over the real `tools/call` wire path.
//!
//! # Why this test exists
//!
//! The 2026-07-28 MCP revision removes protocol-level sessions and the
//! `Mcp-Session-Id` header, and states the replacement directly:
//!
//! > Servers that need cross-call state use explicit, server-minted handles
//! > passed as ordinary tool arguments.
//! >
//! > - [SEP-2567], MCP specification 2026-07-28 changelog
//!
//! mnemo already worked this way, which under the previous revision was one
//! valid option among several and is now the sanctioned one. A property that
//! holds by accident is a property that can be lost by accident, so this file
//! pins it.
//!
//! # What it actually proves
//!
//! `mnemo.checkpoint` mints a checkpoint id; `mnemo.branch` forks from that id;
//! `mnemo.replay` reconstructs from it. The chain is driven end to end through
//! JSON-RPC `tools/call` over an in-memory duplex transport, so what is under
//! test is the surface a client sees, not an internal shortcut. The duplex
//! transport carries **no session identifier of any kind** - there is no
//! `Mcp-Session-Id` header to send and no session store to consult - so if any
//! step depended on connection-scoped state rather than on the handle, it could
//! not resolve and the chain would break.
//!
//! The negative case is covered too: a handle invented by the client, rather
//! than minted by the server, must not resolve.
//!
//! [SEP-2567]: https://github.com/modelcontextprotocol/modelcontextprotocol/pull/2567

use std::sync::Arc;

use mnemo_core::embedding::DeterministicEmbedding;
use mnemo_core::index::usearch::UsearchIndex;
use mnemo_core::query::MnemoEngine;
use mnemo_core::storage::duckdb::DuckDbStorage;
use mnemo_mcp::server::MnemoServer;
use rmcp::ServiceExt;
use rmcp::model::{CallToolRequestParams, CallToolResponse};
use rmcp::service::RunningService;

fn engine() -> Arc<MnemoEngine> {
    Arc::new(MnemoEngine::new(
        Arc::new(DuckDbStorage::open_in_memory().expect("in-memory duckdb")),
        Arc::new(UsearchIndex::new(128).expect("usearch index")),
        Arc::new(DeterministicEmbedding::new(128)),
        "handle-test-agent".to_string(),
        None,
    ))
}

/// A live client/server pair over an in-memory duplex.
///
/// This is the whole point of the test setup: a duplex pipe has no headers, no
/// cookies and no session table. Every fact a tool call relies on must travel
/// in that call's own arguments.
async fn connected() -> RunningService<rmcp::RoleClient, ()> {
    let (client_io, server_io) = tokio::io::duplex(1 << 16);
    let server = MnemoServer::new(engine());
    tokio::spawn(async move {
        if let Ok(running) = server.serve(server_io).await {
            let _ = running.waiting().await;
        }
    });
    ().serve(client_io).await.expect("client connects")
}

/// Call a tool and return its first text block parsed as JSON.
async fn call_json(
    client: &RunningService<rmcp::RoleClient, ()>,
    name: &'static str,
    args: serde_json::Value,
) -> serde_json::Value {
    let arguments = match args {
        serde_json::Value::Object(map) => map,
        other => panic!("tool arguments must be a JSON object, got {other}"),
    };
    let response = client
        .call_tool_once(CallToolRequestParams::new(name).with_arguments(arguments))
        .await
        .unwrap_or_else(|e| panic!("`{name}` should reach the server: {e}"));

    let result = match response {
        CallToolResponse::Complete(result) => result,
        other => panic!("`{name}` is synchronous, expected Complete, got {other:?}"),
    };
    assert_ne!(
        result.is_error,
        Some(true),
        "`{name}` returned a tool error: {:?}",
        result.content
    );
    let text = result
        .content
        .first()
        .and_then(|c| c.as_text())
        .map(|t| t.text.clone())
        .unwrap_or_else(|| panic!("`{name}` should return a text block"));
    serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("`{name}` should return JSON, got {text:?}: {e}"))
}

/// checkpoint -> branch -> replay, with the handle as the only state carrier.
#[tokio::test]
async fn checkpoint_branch_replay_threads_only_a_server_minted_handle() {
    let client = connected().await;
    let thread_id = "thread-explicit-handle";

    // 1. The server mints a handle. Nothing about this call identifies a
    //    session; the thread id is an ordinary argument the caller chose.
    let checkpoint = call_json(
        &client,
        "mnemo.checkpoint",
        serde_json::json!({
            "thread_id": thread_id,
            "state_snapshot": { "step": 1, "note": "before the fork" },
            "label": "root",
        }),
    )
    .await;

    let checkpoint_id = checkpoint
        .get("checkpoint_id")
        .or_else(|| checkpoint.get("id"))
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| panic!("checkpoint must return a handle, got {checkpoint}"))
        .to_string();
    assert!(
        uuid::Uuid::parse_str(&checkpoint_id).is_ok(),
        "the handle should be a server-minted uuid, got {checkpoint_id:?}"
    );

    // 2. A second call consumes that handle as an ordinary argument. If branch
    //    resolved its source from connection state instead, this would still
    //    "work" on one connection and silently break behind a gateway.
    let branch = call_json(
        &client,
        "mnemo.branch",
        serde_json::json!({
            "thread_id": thread_id,
            "new_branch_name": "explore",
            "source_checkpoint_id": checkpoint_id,
        }),
    )
    .await;
    assert_eq!(
        branch.get("source_checkpoint_id").and_then(|v| v.as_str()),
        Some(checkpoint_id.as_str()),
        "branch must fork from the handle it was given, not from whatever the \
         connection last touched; got {branch}"
    );
    assert_eq!(
        branch.get("branch_name").and_then(|v| v.as_str()),
        Some("explore"),
        "branch should report the branch it created, got {branch}"
    );

    // 3. A third call reconstructs from the same handle.
    //
    //    A *newer* checkpoint is minted first, on the same thread, so that
    //    "resolved the handle it was given" and "returned the only checkpoint
    //    there was" stop being the same observation. Without this the snapshot
    //    assertion below would pass even for a server that ignored the handle
    //    entirely and returned the latest state.
    call_json(
        &client,
        "mnemo.checkpoint",
        serde_json::json!({
            "thread_id": thread_id,
            "state_snapshot": { "step": 99, "note": "after the fork" },
            "label": "newer",
        }),
    )
    .await;

    let replay = call_json(
        &client,
        "mnemo.replay",
        serde_json::json!({
            "thread_id": thread_id,
            "checkpoint_id": checkpoint_id,
        }),
    )
    .await;
    let replayed = replay
        .get("checkpoint")
        .unwrap_or_else(|| panic!("replay should return the checkpoint it resolved, got {replay}"));
    assert_eq!(
        replayed.get("id").and_then(|v| v.as_str()),
        Some(checkpoint_id.as_str()),
        "replay must resolve the handle it was given, got {replay}"
    );
    // The handle is the state carrier, so the state it carries must come back
    // intact. This is the assertion that would fail if `replay` were quietly
    // reconstructing "current state" instead of the checkpoint's.
    assert_eq!(
        replayed.get("state_snapshot"),
        Some(&serde_json::json!({ "step": 1, "note": "before the fork" })),
        "the snapshot minted with the handle must round-trip through it, got {replay}"
    );

    // The chain completed with no session identifier anywhere in it.
    let _ = client.cancel().await;
}

/// A handle the client invented, rather than one the server minted, resolves to
/// nothing.
///
/// This is what makes the handle load-bearing rather than decorative: if replay
/// accepted any well-formed uuid and fell back to "the current state", the
/// pattern would be carrying no authority and the first test would pass for the
/// wrong reason.
#[tokio::test]
async fn a_handle_the_server_never_minted_does_not_resolve() {
    let client = connected().await;
    let thread_id = "thread-forged-handle";

    call_json(
        &client,
        "mnemo.checkpoint",
        serde_json::json!({
            "thread_id": thread_id,
            "state_snapshot": { "step": 1 },
        }),
    )
    .await;

    let forged = uuid::Uuid::new_v4().to_string();
    let forged_args = serde_json::json!({
        "thread_id": thread_id,
        "checkpoint_id": forged,
    })
    .as_object()
    .cloned()
    .expect("object");
    let response = client
        .call_tool_once(CallToolRequestParams::new("mnemo.replay").with_arguments(forged_args))
        .await
        .expect("the call itself reaches the server");

    let CallToolResponse::Complete(result) = response else {
        panic!("replay is synchronous, expected Complete");
    };

    assert_eq!(
        result.is_error,
        Some(true),
        "a checkpoint id the server never minted must be rejected, not silently \
         resolved to current state; got {:?}",
        result.content
    );
    let message = result
        .content
        .first()
        .and_then(|b| b.as_text())
        .map(|t| t.text.clone())
        .unwrap_or_default();
    assert!(
        message.contains(&forged),
        "the rejection should name the handle it could not resolve, got {message:?}"
    );

    let _ = client.cancel().await;
}
