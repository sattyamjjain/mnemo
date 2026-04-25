//! Bitemporal correctness tests for the v0.4.0-rc1 graph layer.
//!
//! The single most important property: when a fact is superseded by a
//! later contradicting fact, an `as_of` query *between* the two events
//! must still return the original answer; an `as_of` query *after* the
//! supersession must return the new one. Without this, we may as well
//! be running a regular (non-temporal) graph.

use chrono::{TimeZone, Utc};
use mnemo_graph::{DuckGraphStore, GraphStore, TemporalEdge, graph_expand};
use uuid::Uuid;

#[tokio::test]
async fn supersession_returns_correct_target_per_as_of() {
    let store = DuckGraphStore::open_in_memory().expect("store");
    let person = Uuid::now_v7();
    let company_a = Uuid::now_v7();
    let company_b = Uuid::now_v7();

    // 2026-01-01: person works_at A
    let t1 = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
    let edge_a = TemporalEdge::new(person, company_a, "works_at", t1, None, 0.9);
    store.insert_edge(&edge_a).await.unwrap();

    // 2026-04-01: person leaves A and joins B; close the A edge first
    // (a real extractor would do this when it sees a contradicting fact).
    let t2 = Utc.with_ymd_and_hms(2026, 4, 1, 0, 0, 0).unwrap();
    store.close_edge(edge_a.id, t2).await.unwrap();
    let edge_b = TemporalEdge::new(person, company_b, "works_at", t2, None, 0.95);
    store.insert_edge(&edge_b).await.unwrap();

    // ---- Property under test ----
    //
    // At t = 2026-02-15 the person works at A.
    let between = Utc.with_ymd_and_hms(2026, 2, 15, 0, 0, 0).unwrap();
    let reachable_between = graph_expand(&store, person, 2, between).await.unwrap();
    assert!(
        reachable_between.contains(&company_a),
        "as_of=2026-02-15 must reach company_a (the relation that was true then)"
    );
    assert!(
        !reachable_between.contains(&company_b),
        "as_of=2026-02-15 must NOT reach company_b (relation hadn't started yet)"
    );

    // At t = 2026-06-01 the person works at B.
    let after = Utc.with_ymd_and_hms(2026, 6, 1, 0, 0, 0).unwrap();
    let reachable_after = graph_expand(&store, person, 2, after).await.unwrap();
    assert!(
        reachable_after.contains(&company_b),
        "as_of=2026-06-01 must reach company_b (current relation)"
    );
    assert!(
        !reachable_after.contains(&company_a),
        "as_of=2026-06-01 must NOT reach company_a (relation closed at 2026-04-01)"
    );
}

#[tokio::test]
async fn confidence_orders_outgoing_edges_descending() {
    let store = DuckGraphStore::open_in_memory().unwrap();
    let src = Uuid::now_v7();
    let dst_low = Uuid::now_v7();
    let dst_high = Uuid::now_v7();
    let t = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
    store
        .insert_edge(&TemporalEdge::new(src, dst_low, "rel", t, None, 0.4))
        .await
        .unwrap();
    store
        .insert_edge(&TemporalEdge::new(src, dst_high, "rel", t, None, 0.95))
        .await
        .unwrap();
    let outgoing = store
        .outgoing_at(src, Utc.with_ymd_and_hms(2026, 6, 1, 0, 0, 0).unwrap())
        .await
        .unwrap();
    assert_eq!(outgoing.len(), 2);
    assert_eq!(
        outgoing[0].dst, dst_high,
        "higher-confidence edge must come first"
    );
    assert_eq!(outgoing[1].dst, dst_low);
}

#[tokio::test]
async fn graph_expand_respects_max_depth() {
    let store = DuckGraphStore::open_in_memory().unwrap();
    let a = Uuid::now_v7();
    let b = Uuid::now_v7();
    let c = Uuid::now_v7();
    let d = Uuid::now_v7();
    let t = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
    for (s, e) in [(a, b), (b, c), (c, d)] {
        store
            .insert_edge(&TemporalEdge::new(s, e, "next", t, None, 1.0))
            .await
            .unwrap();
    }
    let after = Utc.with_ymd_and_hms(2026, 6, 1, 0, 0, 0).unwrap();
    let depth_1 = graph_expand(&store, a, 1, after).await.unwrap();
    assert!(depth_1.contains(&b));
    assert!(!depth_1.contains(&c), "depth 1 must not reach c");

    let depth_2 = graph_expand(&store, a, 2, after).await.unwrap();
    assert!(depth_2.contains(&c));
    assert!(!depth_2.contains(&d), "depth 2 must not reach d");

    let depth_3 = graph_expand(&store, a, 3, after).await.unwrap();
    assert!(depth_3.contains(&d), "depth 3 must reach d");
}

#[tokio::test]
async fn close_edge_is_idempotent() {
    let store = DuckGraphStore::open_in_memory().unwrap();
    let edge = TemporalEdge::new(
        Uuid::now_v7(),
        Uuid::now_v7(),
        "rel",
        Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
        None,
        1.0,
    );
    store.insert_edge(&edge).await.unwrap();
    let close_at = Utc.with_ymd_and_hms(2026, 4, 1, 0, 0, 0).unwrap();
    store.close_edge(edge.id, close_at).await.unwrap();
    // Closing again must not move the valid_to timestamp.
    let bogus = Utc.with_ymd_and_hms(2030, 1, 1, 0, 0, 0).unwrap();
    store.close_edge(edge.id, bogus).await.unwrap();
    let edges = store.all_edges().await.unwrap();
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].valid_to, Some(close_at));
}

#[tokio::test]
async fn extract_stub_returns_empty_today() {
    // The v0.4.0-rc1 stub returns no edges; once the real extractor
    // lands this assertion flips and we'll write a proper test.
    let edges = TemporalEdge::extract("Priya works at Acme Corp.", &[]);
    assert!(edges.is_empty(), "v0.4.0-rc1 ships an extract stub");
}
