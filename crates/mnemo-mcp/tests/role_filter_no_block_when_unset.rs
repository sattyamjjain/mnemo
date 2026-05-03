//! v0.4.2 (A1) — manifests without a `[role_filter]` block keep
//! pre-v0.4.2 behaviour byte-for-byte (every tool is reachable, no
//! audit events emitted).

use std::sync::Arc;

use mnemo_mcp::role_filter::{
    CallerContext, CapturingAuditSink, ManifestRoleFilter, RoleFilter, RoleFilterConfig,
};

#[test]
fn default_config_is_noop() {
    let filter = ManifestRoleFilter::new(RoleFilterConfig::default());
    assert!(filter.is_noop());
}

#[test]
fn default_config_allows_every_known_tool() {
    let filter = ManifestRoleFilter::new(RoleFilterConfig::default());
    let caller = CallerContext::new("anyone", vec![]);
    for tool in &[
        "mnemo.remember",
        "mnemo.recall",
        "mnemo.forget",
        "mnemo.forget_subject",
        "mnemo.share",
        "mnemo.checkpoint",
        "mnemo.branch",
        "mnemo.merge",
        "mnemo.replay",
        "mnemo.delegate",
        "mnemo.verify",
    ] {
        assert!(
            filter.allows(&caller, tool).is_allow(),
            "default no-op filter must allow {tool}"
        );
    }
}

#[test]
fn default_config_emits_no_audit_events() {
    let sink = Arc::new(CapturingAuditSink::new());
    let filter = ManifestRoleFilter::new(RoleFilterConfig::default()).with_audit_sink(sink.clone());
    let caller = CallerContext::new("anyone", vec![]);
    for tool in &["mnemo.remember", "mnemo.forget", "mnemo.delegate"] {
        let _ = filter.allows(&caller, tool);
    }
    assert!(
        sink.snapshot().is_empty(),
        "no-op filter must not emit audit events"
    );
}

#[test]
fn filter_tools_with_default_returns_input_unchanged() {
    let filter = ManifestRoleFilter::new(RoleFilterConfig::default());
    let caller = CallerContext::new("anyone", vec![]);
    let advertised = vec![
        "mnemo.remember".to_string(),
        "mnemo.recall".to_string(),
        "mnemo.forget".to_string(),
    ];
    let visible = filter.filter_tools(&caller, &advertised);
    assert_eq!(visible, advertised);
}
