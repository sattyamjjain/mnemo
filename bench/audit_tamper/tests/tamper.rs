//! Contract tests for the audit-log tamper-evidence bench.
//!
//! These pin the honest claims the bench makes so they can never silently drift:
//! the three structurally-detectable attacks are caught at 100%, the two
//! disclosed gaps are caught at 0%, a legitimate append is never flagged, and
//! the emitted report is byte-stable.

use mnemo_audit_tamper_bench::{BenchConfig, render_markdown, run_bench};

fn cfg() -> BenchConfig {
    // Small but >1 chain so the forge-content_hash attack exercises position 0
    // (t % chain_len == 0) and is still caught at its successor.
    BenchConfig {
        trials: 64,
        chain_len: 16,
    }
}

#[tokio::test]
async fn detection_rates_match_the_honest_threat_model() {
    let o = run_bench(&cfg()).await;

    let by = |name: &str| {
        o.attacks
            .iter()
            .find(|a| a.name.starts_with(name))
            .unwrap_or_else(|| panic!("missing attack row: {name}"))
    };

    // Structurally detectable → 100%.
    let delete = by("delete");
    assert_eq!(delete.detected, delete.n, "delete-mid must be fully caught");
    assert!(delete.caught_by_chain);

    let reorder = by("reorder");
    assert_eq!(reorder.detected, reorder.n, "reorder must be fully caught");
    assert!(reorder.caught_by_chain);

    let forge_hash = by("forge (integrity field");
    assert_eq!(
        forge_hash.detected, forge_hash.n,
        "content_hash forgery must be fully caught"
    );
    assert!(forge_hash.caught_by_chain);

    // Disclosed gaps → 0% (do not oversell).
    let forge_payload = by("forge (payload only");
    assert_eq!(
        forge_payload.detected, 0,
        "payload-only forgery is a disclosed gap for the pure chain verifier"
    );
    assert!(!forge_payload.caught_by_chain);

    let truncate = by("truncate");
    assert_eq!(
        truncate.detected, 0,
        "tail truncation is a disclosed gap for the pure chain verifier"
    );
    assert!(!truncate.caught_by_chain);
}

#[tokio::test]
async fn benign_control_has_zero_false_positives() {
    let o = run_bench(&cfg()).await;
    assert_eq!(
        o.benign_false_positives, 0,
        "a legitimately-appended chain must never be flagged as tampered"
    );
    assert!(o.benign_n > o.chain_len, "benign control extends the chain");
}

#[tokio::test]
async fn report_is_byte_stable_across_runs() {
    let a = render_markdown(&run_bench(&cfg()).await, "2026-07-16");
    let b = render_markdown(&run_bench(&cfg()).await, "2026-07-16");
    assert_eq!(a, b, "the report must be byte-identical run-to-run");
}
