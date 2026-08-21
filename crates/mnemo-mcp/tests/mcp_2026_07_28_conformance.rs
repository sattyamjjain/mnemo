//! Pins the claims in `docs/src/integrations/mcp-2026-07-28.md` to the server.
//!
//! A conformance table is a claim about behaviour, and this repo's recurring
//! defect is a claim that stopped being true without anything failing. So the
//! rows that can be checked mechanically are checked here, and the document is
//! parsed rather than trusted.
//!
//! Two kinds of assertion live in this file:
//!
//! 1. **CONFORMS rows** - assert the good behaviour, so a regression fails.
//! 2. **GAP rows** - assert the *absence*, so that closing a gap without
//!    updating the table fails too. A table that silently understates what the
//!    server does is as wrong as one that overstates it, and only the second
//!    kind of test catches the first kind of drift.

use std::sync::Arc;

use mnemo_core::embedding::DeterministicEmbedding;
use mnemo_core::index::usearch::UsearchIndex;
use mnemo_core::query::MnemoEngine;
use mnemo_core::storage::duckdb::DuckDbStorage;
use mnemo_mcp::server::MnemoServer;
use rmcp::model::{ListToolsResult, ProtocolVersion};
use rmcp::{ServerHandler, ServiceExt};

const DOC: &str = include_str!("../../../docs/src/integrations/mcp-2026-07-28.md");

/// The revision mnemo does not implement and therefore must not advertise.
const UNIMPLEMENTED: &str = "2026-07-28";

fn server() -> MnemoServer {
    MnemoServer::new(Arc::new(MnemoEngine::new(
        Arc::new(DuckDbStorage::open_in_memory().expect("in-memory duckdb")),
        Arc::new(UsearchIndex::new(128).expect("usearch index")),
        Arc::new(DeterministicEmbedding::new(128)),
        "conformance-agent".to_string(),
        None,
    )))
}

fn advertised() -> Vec<String> {
    server()
        .supported_protocol_versions()
        .iter()
        .map(|v| v.as_str().to_string())
        .collect()
}

/// The versions listed inside the pinned block in the conformance doc.
fn documented() -> Vec<String> {
    let start = DOC
        .find("<!-- BEGIN pinned: advertised-protocol-versions -->")
        .expect("the doc must carry the pinned block this test reads");
    let end = DOC
        .find("<!-- END pinned: advertised-protocol-versions -->")
        .expect("the pinned block must be closed");
    assert!(start < end, "pinned block markers are inverted");
    DOC[start..end]
        .lines()
        .filter_map(|l| l.trim().strip_prefix("- `"))
        .filter_map(|l| l.strip_suffix('`'))
        .map(|s| s.to_string())
        .collect()
}

/// The defect the conformance page found: rmcp's default advertised every
/// revision the SDK knows, including one mnemo does not serve.
#[test]
fn mnemo_does_not_advertise_the_revision_it_does_not_implement() {
    let advertised = advertised();
    assert!(
        !advertised.iter().any(|v| v == UNIMPLEMENTED),
        "mnemo advertised {UNIMPLEMENTED} in `supported_protocol_versions()`, which rmcp \
         derives `server/discover` from. Either implement the revision (and update \
         docs/src/integrations/mcp-2026-07-28.md), or keep it out of the list. \
         Advertising a revision the server does not serve is a claim, not a courtesy. \
         Got: {advertised:?}"
    );
}

/// The narrowing must not have gone too far: the revisions mnemo does serve
/// have to stay advertised, or older clients lose the ability to negotiate.
#[test]
fn mnemo_still_advertises_every_revision_it_does_serve() {
    let advertised = advertised();
    for expected in [
        ProtocolVersion::V_2024_11_05,
        ProtocolVersion::V_2025_03_26,
        ProtocolVersion::V_2025_06_18,
        ProtocolVersion::V_2025_11_25,
    ] {
        assert!(
            advertised.iter().any(|v| v == expected.as_str()),
            "mnemo stopped advertising {expected}, which it does serve; a client that \
             negotiates it would be pushed to the server fallback for no reason. Got: \
             {advertised:?}"
        );
    }
}

/// The table's headline number, asserted rather than asserted-about.
#[test]
fn initialize_still_settles_on_2025_11_25() {
    assert_eq!(
        server().get_info().protocol_version,
        ProtocolVersion::V_2025_11_25,
        "the conformance page's headline is that mnemo negotiates 2025-11-25. If this \
         changed, that page is now wrong and every UPSTREAM-BLOCKED row needs rereading."
    );
}

/// Doc parity: the page's pinned list must be the server's actual list.
#[test]
fn the_conformance_doc_lists_exactly_what_the_server_advertises() {
    let documented = documented();
    assert!(
        !documented.is_empty(),
        "the pinned block in the conformance doc parsed to nothing; the format the test \
         reads (`- \\`YYYY-MM-DD\\`` lines) must have changed"
    );
    assert_eq!(
        documented,
        advertised(),
        "docs/src/integrations/mcp-2026-07-28.md lists a different set of protocol \
         versions than the server advertises. The doc is the thing a reader trusts, so \
         update whichever is wrong."
    );
}

/// A live client/server pair, so the list result under test is the one a real
/// client receives rather than one this test built.
async fn listed_tools() -> ListToolsResult {
    let (client_io, server_io) = tokio::io::duplex(1 << 16);
    tokio::spawn(async move {
        if let Ok(running) = server().serve(server_io).await {
            let _ = running.waiting().await;
        }
    });
    let client = ().serve(client_io).await.expect("client connects");
    let result = client.list_tools(None).await.expect("tools/list succeeds");
    let _ = client.cancel().await;
    result
}

/// A live client/server pair for `resources/list`, same reasoning as above.
async fn listed_resources() -> rmcp::model::ListResourcesResult {
    let (client_io, server_io) = tokio::io::duplex(1 << 16);
    tokio::spawn(async move {
        if let Ok(running) = server().serve(server_io).await {
            let _ = running.waiting().await;
        }
    });
    let client = ().serve(client_io).await.expect("client connects");
    let result = client
        .list_resources(None)
        .await
        .expect("resources/list succeeds");
    let _ = client.cancel().await;
    result
}

/// SEP-2549 caching hints, formerly a GAP row and now implemented.
///
/// Asserted against the values the conformance page states, so changing one
/// without changing the other fails.
#[tokio::test]
async fn list_results_carry_the_documented_cache_hints() {
    let tools = listed_tools().await;
    assert_eq!(
        tools.ttl_ms,
        Some(60_000),
        "tools/list should carry the documented 60s freshness hint"
    );
    let resources = listed_resources().await;
    assert_eq!(
        resources.ttl_ms,
        Some(0),
        "resources/list should advertise itself as immediately stale: any \
         `mnemo.remember` invalidates the listing, so a positive TTL would invite \
         a client to serve a list a write has already superseded"
    );
}

/// The security half of SEP-2549, asserted on its own so it cannot be weakened
/// by accident.
///
/// This is the row's real content. `cacheScope` is not a performance knob here:
/// both listings are scoped to the caller's per-request identity (ADR 0002), so
/// `Public` would let a shared intermediary serve one caller's tool catalog, or
/// one agent's memory records, to a different caller. `CacheScope::default()` is
/// `Public`, which is why "leave it unset" is not the safe option it looks like.
#[tokio::test]
async fn per_caller_listings_are_never_publicly_cacheable() {
    for (surface, scope) in [
        ("tools/list", listed_tools().await.cache_scope),
        ("resources/list", listed_resources().await.cache_scope),
    ] {
        assert_eq!(
            scope,
            Some(rmcp::model::CacheScope::Private),
            "{surface} is filtered per caller, so it must be advertised as \
             `private`. `{scope:?}` would permit a shared cache to serve one \
             caller's results to another."
        );
    }
}

/// CONFORMS row: deterministic `tools/list` ordering, which mnemo gets from
/// rmcp's router rather than from its own filter.
#[tokio::test]
async fn tool_listing_order_is_deterministic() {
    let first: Vec<String> = listed_tools()
        .await
        .tools
        .into_iter()
        .map(|t| t.name.to_string())
        .collect();
    let again: Vec<String> = listed_tools()
        .await
        .tools
        .into_iter()
        .map(|t| t.name.to_string())
        .collect();
    assert!(!first.is_empty(), "the server should list some tools");
    assert_eq!(
        first, again,
        "two identical listings disagreed, so the ordering is not deterministic"
    );
    let mut sorted = first.clone();
    sorted.sort();
    assert_eq!(
        first, sorted,
        "the conformance page claims deterministic ordering via rmcp's name sort; the \
         listing is no longer sorted by name"
    );
}

/// Every row that is not CONFORMS must tell a caller what to assume instead.
///
/// A conformance table earns trust by being useful when the answer is "no". A
/// row that says GAP and stops has told a reader that something is missing
/// without telling them how to behave, which is the shape of documentation that
/// looks answered and is not. The rule is: implement it, or say what a caller
/// should assume. This asserts the second half, so a future open row cannot be
/// added without one.
#[test]
fn every_open_row_tells_a_caller_what_to_assume() {
    const REQUIRED: &str = "A caller should assume";
    let mut offenders = Vec::new();

    for (idx, line) in DOC.lines().enumerate() {
        let trimmed = line.trim();
        if !trimmed.starts_with('|') {
            continue;
        }
        // Conformance rows have five columns: requirement, spec, mnemo, rmcp,
        // status. The status legend (two columns) and the closing summary
        // (three) also name GAP and UPSTREAM-BLOCKED, and neither is a row
        // about a specific behaviour, so neither needs the sentence.
        let columns = trimmed.matches('|').count();
        if columns < 6 {
            continue;
        }
        let open = trimmed.contains("**GAP**") || trimmed.contains("**UPSTREAM-BLOCKED**");
        if open && !trimmed.contains(REQUIRED) {
            let head = trimmed.split('|').nth(1).unwrap_or("?").trim().to_string();
            offenders.push(format!("  line {}: {head}", idx + 1));
        }
    }

    assert!(
        offenders.is_empty(),
        "docs/src/integrations/mcp-2026-07-28.md has open row(s) with no \
         \"{REQUIRED} ...\" sentence. An open row must either be implemented or \
         say how a caller should behave without it:\n{}",
        offenders.join("\n")
    );
}

/// The table must not quietly become all-green either.
///
/// If every row read CONFORMS, the honest reading is usually that a row was
/// deleted rather than closed. This asserts the page still tracks open work, so
/// "we conform" has to be earned by rows rather than reached by removing them.
#[test]
fn the_table_still_tracks_open_rows() {
    let open = DOC
        .lines()
        .filter(|l| l.trim().starts_with('|') && l.matches('|').count() >= 6)
        .filter(|l| l.contains("**GAP**") || l.contains("**UPSTREAM-BLOCKED**"))
        .count();
    assert!(
        open > 0,
        "the conformance table now reports no open rows at all. If mnemo really \
         did adopt 2026-07-28, this page should have been rewritten rather than \
         amended, and this test replaced along with it."
    );
}
