//! Contract test: every deletion path in mnemo-core must conform to the
//! retention floor — the real `forget`/`run_ttl_sweep`/`run_decay_pass`/
//! `run_consolidation`/cold-archive paths, driven end-to-end, must not drop or
//! rewrite an `agent_events` row.

use mnemo_retention_conformance_bench::{Config, run_report};

#[tokio::test]
async fn every_deletion_path_retains_the_log_dpdp() {
    let report = run_report(&Config::default()).await.unwrap();
    assert_eq!(report.profile, "dpdp-rules");
    assert_eq!(report.floor_days, 365);
    for f in &report.findings {
        assert!(f.pass, "path {} must retain the log: {}", f.path, f.detail);
    }
    assert!(report.conformant, "DPDP retention conformance must hold");
}

#[tokio::test]
async fn eu_ai_act_art19_profile_conforms() {
    let cfg = Config {
        profile: "eu-ai-act-art19".to_string(),
        ..Config::default()
    };
    let report = run_report(&cfg).await.unwrap();
    assert_eq!(report.floor_days, 180);
    assert!(report.conformant);
}

#[tokio::test]
async fn traffic_metadata_is_verified_present() {
    let report = run_report(&Config::default()).await.unwrap();
    // Every path emits a `::traffic_metadata` finding, and all must pass
    // (traffic-bearing events seeded and retained with fields intact).
    let traffic: Vec<_> = report
        .findings
        .iter()
        .filter(|f| f.path.ends_with("::traffic_metadata"))
        .collect();
    assert!(!traffic.is_empty(), "traffic-metadata checks must run");
    for f in traffic {
        assert!(f.pass, "traffic metadata must be retained: {}", f.detail);
    }
}
