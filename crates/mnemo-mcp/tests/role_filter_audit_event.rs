//! v0.4.2 (A1) — every denied call emits an `McpRoleDenied` audit event
//! carrying `(caller_id, tool_name, attempted_at, reason)`.

use std::collections::BTreeMap;
use std::sync::Arc;

use mnemo_mcp::role_filter::{
    CallerContext, CapturingAuditSink, DefaultPolicy, ManifestRoleFilter, RoleAuditSink,
    RoleFilter, RoleFilterConfig,
};

fn deny_only_filter(sink: Arc<dyn RoleAuditSink>) -> ManifestRoleFilter {
    let mut deny = BTreeMap::new();
    deny.insert("mnemo.forget_subject".to_string(), vec!["read-only".into()]);
    deny.insert("mnemo.delegate".to_string(), vec!["read-only".into()]);
    let config = RoleFilterConfig {
        caller_roles: vec![],
        default: DefaultPolicy::AllowAll,
        allow: BTreeMap::new(),
        deny,
    };
    ManifestRoleFilter::new(config).with_audit_sink(sink)
}

#[test]
fn deny_emits_audit_event_with_caller_and_tool() {
    let sink = Arc::new(CapturingAuditSink::new());
    let filter = deny_only_filter(sink.clone());
    let caller = CallerContext::new("op-7", vec!["read-only".into()]);

    let _ = filter.allows(&caller, "mnemo.forget_subject");
    let _ = filter.allows(&caller, "mnemo.delegate");
    // Allowed call — must NOT emit an audit row.
    let _ = filter.allows(&caller, "mnemo.recall");

    let events = sink.snapshot();
    assert_eq!(events.len(), 2, "expected one deny event per blocked call");

    assert_eq!(events[0].caller_id, "op-7");
    assert_eq!(events[0].tool_name, "mnemo.forget_subject");
    assert!(events[0].reason.contains("deny"));

    assert_eq!(events[1].caller_id, "op-7");
    assert_eq!(events[1].tool_name, "mnemo.delegate");
    assert!(events[1].reason.contains("deny"));
}

#[test]
fn allow_does_not_emit_audit_event() {
    let sink = Arc::new(CapturingAuditSink::new());
    let filter = deny_only_filter(sink.clone());
    // Caller has no role, AllowAll default — every call passes.
    let caller = CallerContext::new("op-8", vec![]);
    for tool in &["mnemo.remember", "mnemo.recall", "mnemo.share"] {
        let _ = filter.allows(&caller, tool);
    }
    assert_eq!(
        sink.snapshot().len(),
        0,
        "no deny events should be emitted on allow path"
    );
}

#[test]
fn allow_list_miss_under_deny_all_emits_audit() {
    let mut allow = BTreeMap::new();
    allow.insert("mnemo.recall".to_string(), vec!["agent".into()]);
    let config = RoleFilterConfig {
        caller_roles: vec![],
        default: DefaultPolicy::DenyAll,
        allow,
        deny: BTreeMap::new(),
    };
    let sink = Arc::new(CapturingAuditSink::new());
    let filter = ManifestRoleFilter::new(config).with_audit_sink(sink.clone());
    let caller = CallerContext::new("op-9", vec!["other".into()]);

    let _ = filter.allows(&caller, "mnemo.recall");

    let events = sink.snapshot();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].tool_name, "mnemo.recall");
    assert!(events[0].reason.contains("not permitted"));
}
