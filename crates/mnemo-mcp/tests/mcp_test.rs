//! MCP server integration tests.
//!
//! Since rmcp's #[tool] macro generates private methods, these tests verify
//! server construction, ServerHandler impl, and engine integration through
//! the public MnemoEngine API that the MCP tools delegate to.

use std::sync::Arc;

use mnemo_core::embedding::DeterministicEmbedding;
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
    let embedding = Arc::new(DeterministicEmbedding::new(128));
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
    // Both tools and the new v0.3.2 resources capability are advertised.
    assert!(info.capabilities.tools.is_some());
    assert!(
        info.capabilities.resources.is_some(),
        "resources capability must be advertised in v0.3.2"
    );
}

/// The building blocks of `list_resources` / `read_resource`: seed
/// memories, list them via the same storage path the resource handler
/// uses, and fetch one by id. Full MCP-handler dispatch needs a running
/// service harness and stdio transport — covered by the broader
/// end-to-end tests; this asserts the data surface the handler depends on.
#[tokio::test]
async fn test_resource_surface_storage_contract() {
    use mnemo_mcp::server::MEMORY_RESOURCE_SCHEME;

    let (_, engine) = create_server();
    let first = engine
        .remember(RememberRequest {
            content: "First resource memory".to_string(),
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

    let filter = mnemo_core::storage::MemoryFilter {
        agent_id: Some("test-agent".to_string()),
        include_deleted: false,
        ..Default::default()
    };
    let records = engine.storage.list_memories(&filter, 50, 0).await.unwrap();
    assert!(!records.is_empty());

    let uri = format!("{MEMORY_RESOURCE_SCHEME}{}", first.id);
    assert!(uri.starts_with("mem://"));

    let round_trip = engine.storage.get_memory(first.id).await.unwrap().unwrap();
    assert_eq!(round_trip.content, "First resource memory");
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
            explain: None,
            with_provenance: None,
            mode: None,
            current_fact_resolver: None,
            orientation_cache: None,
            evidence_budget: None,
            retained_token_budget: None,
            domain_scope: None,
            reasoning_trust: None,
        })
        .await
        .unwrap();

    assert_eq!(recall.total, 1);
    assert!(recall.memories[0].content.contains("MCP recall test"));
}

/// Agent-controlled memory mode (AutoMEM, arXiv:2606.04315) contract.
///
/// The `#[tool]` macro makes the tool methods private, so this exercises
/// the same engine path the `mnemo.mem_*` tools delegate to: the
/// reserved `agent-managed` tag scopes `mem_read` to the agent's own
/// flat store (while the default pipeline still sees everything), and a
/// revise (soft-forget old + write new) supersedes the stale entry.
#[tokio::test]
async fn test_agent_managed_flat_store_contract() {
    use mnemo_core::query::forget::{ForgetRequest, ForgetStrategy};
    use mnemo_mcp::tools::agent_managed::AGENT_MANAGED_TAG;

    let (_, engine) = create_server();

    // mem_write: two agent-curated entries carry the reserved tag.
    let mut w1 = RememberRequest::new("project deadline is March".to_string());
    w1.tags = Some(vec![AGENT_MANAGED_TAG.to_string()]);
    let revisable = engine.remember(w1).await.unwrap();

    let mut w2 = RememberRequest::new("user prefers dark mode".to_string());
    w2.tags = Some(vec![AGENT_MANAGED_TAG.to_string()]);
    engine.remember(w2).await.unwrap();

    // A pipeline-only entry the agent did NOT curate (no reserved tag).
    engine
        .remember(RememberRequest::new(
            "incidental log line the agent ignored".to_string(),
        ))
        .await
        .unwrap();

    // mem_read: tag-scoped recall sees only the 2 agent-managed entries,
    // never the pipeline-only one.
    let mut read = RecallRequest::new("project".to_string());
    read.tags = Some(vec![AGENT_MANAGED_TAG.to_string()]);
    read.limit = Some(50);
    let scoped = engine.recall(read).await.unwrap();
    assert!(scoped.total >= 1);
    assert!(
        scoped
            .memories
            .iter()
            .all(|m| m.tags.iter().any(|t| t == AGENT_MANAGED_TAG)),
        "mem_read must only surface agent-managed entries"
    );
    assert!(
        !scoped
            .memories
            .iter()
            .any(|m| m.content.contains("incidental log line")),
        "mem_read must not surface pipeline-only entries"
    );

    // The DEFAULT pipeline still sees the pipeline-only entry (untouched).
    let mut broad = RecallRequest::new("incidental".to_string());
    broad.limit = Some(50);
    let all = engine.recall(broad).await.unwrap();
    assert!(
        all.memories
            .iter()
            .any(|m| m.content.contains("incidental log line")),
        "default recall pipeline must remain the full-store fallback"
    );

    // mem_revise: soft-forget the stale deadline, write the corrected one.
    let mut fr = ForgetRequest::new(vec![revisable.id]);
    fr.strategy = Some(ForgetStrategy::SoftDelete);
    engine.forget(fr).await.unwrap();
    let mut w3 = RememberRequest::new("project deadline is May".to_string());
    w3.tags = Some(vec![AGENT_MANAGED_TAG.to_string()]);
    w3.metadata = Some(serde_json::json!({ "revises": revisable.id.to_string() }));
    engine.remember(w3).await.unwrap();

    let mut after = RecallRequest::new("deadline".to_string());
    after.tags = Some(vec![AGENT_MANAGED_TAG.to_string()]);
    after.limit = Some(50);
    let revised = engine.recall(after).await.unwrap();
    assert!(
        revised.memories.iter().any(|m| m.content.contains("May")),
        "revised value must be readable"
    );
    assert!(
        !revised.memories.iter().any(|m| m.content.contains("March")),
        "stale value must be superseded after revise"
    );
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

/// Item 1 (1d) — the role filter, once attached, both HIDES a denied tool from
/// `tools/list` and BLOCKS it from `tools/call`. `list_tools` / `call_tool` take
/// an rmcp `RequestContext` that requires a live service peer (not constructible
/// in a unit test), so we assert on `visible_tool_names()` and
/// `tool_call_denial()` — the exact functions those two handlers delegate to
/// (see `server.rs`). The second assertion (cannot invoke by name) is the one
/// that matters: a hidden-but-callable tool would still be exploitable.
#[tokio::test]
async fn role_filter_hides_and_blocks_denied_tools() {
    use mnemo_mcp::role_filter::{DefaultPolicy, ManifestRoleFilter, RoleFilterConfig};
    use std::collections::BTreeMap;

    // Baseline: with NO filter attached, every tool is visible and callable.
    let (unfiltered, engine) = create_server();
    assert!(
        unfiltered
            .visible_tool_names()
            .iter()
            .any(|n| n == "mnemo.forget"),
        "without a filter, mnemo.forget must be visible"
    );
    assert!(
        unfiltered.tool_call_denial("mnemo.forget").is_none(),
        "without a filter, no tool is denied"
    );

    // A manifest filter: the caller carries role `agent`; only `mnemo.recall` is
    // allowed for `agent`, everything else falls through to default DenyAll.
    let mut allow = BTreeMap::new();
    allow.insert("mnemo.recall".to_string(), vec!["agent".to_string()]);
    let config = RoleFilterConfig {
        caller_roles: vec!["agent".to_string()],
        default: DefaultPolicy::DenyAll,
        allow,
        deny: BTreeMap::new(),
    };
    let filter = std::sync::Arc::new(ManifestRoleFilter::new(config));
    let server = MnemoServer::new(engine).with_role_filter(filter);

    let visible = server.visible_tool_names();

    // (1) A denied tool is NOT shown in tools/list.
    assert!(
        !visible.iter().any(|n| n == "mnemo.forget"),
        "denied tool mnemo.forget must be hidden from tools/list, got {visible:?}"
    );
    // The allowed tool IS shown.
    assert!(
        visible.iter().any(|n| n == "mnemo.recall"),
        "allowed tool mnemo.recall must remain visible, got {visible:?}"
    );

    // (2) THE ONE THAT MATTERS: a denied tool cannot be invoked by name —
    // call_tool would return the structured -32601 built from this reason.
    let denial = server.tool_call_denial("mnemo.forget");
    assert!(
        denial.as_ref().is_some_and(|r| !r.is_empty()),
        "call_tool must reject mnemo.forget with a non-empty deny reason, got {denial:?}"
    );
    // An allowed tool is not denied.
    assert!(
        server.tool_call_denial("mnemo.recall").is_none(),
        "mnemo.recall must remain callable"
    );
}

/// Item 2 (2e) — the docs cannot silently drift from the registered tool set.
/// Regenerate the list of tool names the server actually registers (from the
/// live `tool_router`, no filter) and assert `docs/src/tools/README.md`
/// documents EXACTLY that set — no undocumented tool, no phantom tool. This is
/// what caught the old "10 tools" / phantom `mnemo.export_audit_log` drift.
#[tokio::test]
async fn docs_document_exactly_the_registered_tools() {
    use std::collections::BTreeSet;

    // Registered set: every tool the router exposes when no role filter is set.
    let (server, _engine) = create_server();
    let registered: BTreeSet<String> = server.visible_tool_names().into_iter().collect();
    assert_eq!(registered.len(), 21, "expected 21 registered tools");

    // Documented set: the first cell of every markdown table row that names a
    // tool. Prose mentions (e.g. the `mnemo.export_audit_log` note, which is a
    // planned/library capability, not a registered tool) live in paragraphs,
    // not `|`-delimited rows, so they are intentionally excluded.
    let readme_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/src/tools/README.md"
    );
    let readme = std::fs::read_to_string(readme_path)
        .unwrap_or_else(|e| panic!("cannot read {readme_path}: {e}"));

    let documented: BTreeSet<String> = readme
        .lines()
        .filter(|l| l.trim_start().starts_with('|'))
        .filter_map(|l| l.split('|').nth(1)) // first cell after leading '|'
        .filter_map(|cell| {
            // Extract a `mnemo.<name>` token (name may contain '_' and '.').
            let start = cell.find("mnemo.")?;
            let rest = &cell[start..];
            let end = rest
                .char_indices()
                .find(|(_, c)| !(c.is_ascii_alphanumeric() || *c == '_' || *c == '.'))
                .map(|(i, _)| i)
                .unwrap_or(rest.len());
            Some(rest[..end].trim_end_matches('.').to_string())
        })
        .collect();

    let missing_from_docs: Vec<_> = registered.difference(&documented).collect();
    let phantom_in_docs: Vec<_> = documented.difference(&registered).collect();

    assert!(
        missing_from_docs.is_empty(),
        "registered tools NOT documented in docs/src/tools/README.md: {missing_from_docs:?}"
    );
    assert!(
        phantom_in_docs.is_empty(),
        "docs/src/tools/README.md documents tools that are NOT registered: {phantom_in_docs:?}"
    );
    assert_eq!(
        registered, documented,
        "documented tool set must exactly equal the registered set"
    );
}
