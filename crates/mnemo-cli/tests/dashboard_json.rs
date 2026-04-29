//! v0.4.1 (P2-6) — Grafana dashboard JSON validates.

#[test]
fn grafana_json_parses_and_carries_required_fields() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("dashboards")
        .join("mnemo-grafana.json");
    let body = std::fs::read_to_string(&path).expect("dashboard JSON readable");
    let parsed: serde_json::Value =
        serde_json::from_str(&body).expect("dashboard JSON is valid JSON");

    // Grafana v11.5 minimum field set.
    for required in ["schemaVersion", "title", "uid", "panels", "time"] {
        assert!(
            parsed.get(required).is_some(),
            "missing required field {required}"
        );
    }
    let schema = parsed["schemaVersion"].as_u64().unwrap();
    assert!(
        schema >= 38,
        "schemaVersion {schema} is below the Grafana 11.5 floor of 38"
    );

    // Operator-critical panels must exist by title.
    let panels = parsed["panels"].as_array().unwrap();
    let titles: Vec<&str> = panels.iter().filter_map(|p| p["title"].as_str()).collect();
    for required_panel in [
        "Recall p50 (ms)",
        "Tool-catalog drift events / 5m",
        "HMAC chain intact",
    ] {
        assert!(
            titles
                .iter()
                .any(|t| t.contains(required_panel.trim_end_matches(" / 5m"))),
            "missing required panel {required_panel:?}; got {:?}",
            titles
        );
    }
}
