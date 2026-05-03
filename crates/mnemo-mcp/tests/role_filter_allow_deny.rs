//! v0.4.2 (A1) — `RoleFilter::allows` allow/deny matrix.
//!
//! Caller `auditor` can call `mnemo.recall` + `mnemo.verify`, blocked
//! from `mnemo.forget` + `mnemo.delegate`. Caller `agent` is allowed
//! everything in this manifest. Missing role + `DefaultPolicy::DenyAll`
//! returns deny.

use std::collections::BTreeMap;

use mnemo_mcp::role_filter::{
    AllowDecision, CallerContext, DefaultPolicy, ManifestRoleFilter, RoleFilter, RoleFilterConfig,
};

fn build_filter() -> ManifestRoleFilter {
    let mut allow = BTreeMap::new();
    allow.insert(
        "mnemo.recall".to_string(),
        vec!["auditor".into(), "agent".into()],
    );
    allow.insert("mnemo.verify".to_string(), vec!["auditor".into()]);
    allow.insert(
        "mnemo.remember".to_string(),
        vec!["agent".into(), "ops".into()],
    );
    allow.insert("mnemo.forget".to_string(), vec!["agent".into()]);
    allow.insert("mnemo.delegate".to_string(), vec!["agent".into()]);

    let config = RoleFilterConfig {
        caller_roles: vec![],
        default: DefaultPolicy::DenyAll,
        allow,
        deny: BTreeMap::new(),
    };
    ManifestRoleFilter::new(config)
}

#[test]
fn auditor_role_can_recall_and_verify() {
    let filter = build_filter();
    let caller = CallerContext::new("auditor-op", vec!["auditor".into()]);
    assert!(matches!(
        filter.allows(&caller, "mnemo.recall"),
        AllowDecision::Allow
    ));
    assert!(matches!(
        filter.allows(&caller, "mnemo.verify"),
        AllowDecision::Allow
    ));
}

#[test]
fn auditor_role_cannot_forget_or_delegate() {
    let filter = build_filter();
    let caller = CallerContext::new("auditor-op", vec!["auditor".into()]);
    match filter.allows(&caller, "mnemo.forget") {
        AllowDecision::Deny { reason } => {
            assert!(reason.contains("not permitted") || reason.contains("denied"))
        }
        AllowDecision::Allow => panic!("auditor must NOT be allowed to forget"),
    }
    match filter.allows(&caller, "mnemo.delegate") {
        AllowDecision::Deny { reason } => {
            assert!(reason.contains("not permitted") || reason.contains("denied"))
        }
        AllowDecision::Allow => panic!("auditor must NOT be allowed to delegate"),
    }
}

#[test]
fn agent_role_allowed_for_every_listed_tool() {
    let filter = build_filter();
    let caller = CallerContext::new("agent-op", vec!["agent".into()]);
    for tool in &[
        "mnemo.recall",
        "mnemo.remember",
        "mnemo.forget",
        "mnemo.delegate",
    ] {
        assert!(
            filter.allows(&caller, tool).is_allow(),
            "agent should be allowed to call {tool}"
        );
    }
}

#[test]
fn missing_role_under_deny_all_default_is_blocked() {
    let filter = build_filter();
    let caller = CallerContext::new("anonymous", vec![]);
    // No allow entry covers "mnemo.checkpoint", default is DenyAll, so
    // the bare caller is denied.
    match filter.allows(&caller, "mnemo.checkpoint") {
        AllowDecision::Deny { reason } => assert!(reason.contains("default policy is deny_all")),
        AllowDecision::Allow => panic!("DenyAll default should reject unrecognised tool"),
    }
}

#[test]
fn manifest_caller_roles_are_honoured() {
    // When the manifest itself declares caller_roles, the call site
    // does not need to pass them explicitly.
    let mut allow = BTreeMap::new();
    allow.insert("mnemo.recall".to_string(), vec!["auditor".into()]);
    let config = RoleFilterConfig {
        caller_roles: vec!["auditor".into()],
        default: DefaultPolicy::DenyAll,
        allow,
        deny: BTreeMap::new(),
    };
    let filter = ManifestRoleFilter::new(config);
    // Caller passes empty roles — the manifest-declared role still applies.
    let caller = CallerContext::new("manifest-only", vec![]);
    assert!(filter.allows(&caller, "mnemo.recall").is_allow());
}
