//! v0.4.2 (A1) — MCP role-aware tool filter.
//!
//! Aligns mnemo's MCP server with the role-based annotations in the
//! 2025-11-25 MCP authorization spec
//! (<https://modelcontextprotocol.io/specification/2025-11-25/basic/authorization>).
//!
//! # Threat model
//!
//! In stdio transport, the MCP protocol carries no per-call caller
//! identity — the binary's *operator* is the caller. The role filter
//! is therefore a manifest-time gate:
//!
//! 1. The operator declares the binary's caller-context roles via the
//!    manifest `[role_filter] caller_roles = [...]` array.
//! 2. Each tool is associated with a role allow-list and/or deny-list.
//! 3. `tools/list` returns only tools the caller's roles permit.
//! 4. `tools/call` for a denied tool returns spec-compliant `-32601`
//!    (method not found) with the role context echoed back in `data`.
//!
//! When the manifest omits the `[role_filter]` block entirely, the
//! filter is a *no-op* — every tool stays exposed and every call
//! passes. This guarantees byte-for-byte backward compatibility with
//! pre-v0.4.2 manifests.
//!
//! # Forward compatibility
//!
//! The `RoleFilter` trait accepts a `CallerContext` so a future HTTP
//! transport that extracts roles from an `Authorization` header (per
//! the MCP authorization spec §6) can plug in without changing the
//! filter's vocabulary.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

/// Opaque, manifest-supplied role label. Operators are expected to
/// pre-hash sensitive subject identities; mnemo never inspects the
/// string beyond equality comparison.
pub type RoleId = String;

/// Fully-qualified MCP tool name (e.g. `"mnemo.recall"`).
pub type ToolName = String;

/// Per-call caller context. In stdio mode this is built from the
/// manifest's `caller_roles`. In future transports (HTTP, SSE) it
/// will be built from authorisation metadata on the request.
#[derive(Debug, Clone)]
pub struct CallerContext {
    /// Caller identity. In stdio this is the manifest-declared agent
    /// id; in HTTP it would be the authenticated subject. Already a
    /// salted, opaque identifier — never raw subject material.
    pub caller_id: String,
    /// Roles the caller carries. Order does not matter for the filter
    /// (we build a `BTreeSet` internally for membership checks).
    pub roles: Vec<RoleId>,
}

impl CallerContext {
    pub fn new(caller_id: impl Into<String>, roles: Vec<RoleId>) -> Self {
        Self {
            caller_id: caller_id.into(),
            roles,
        }
    }
}

/// Filter verdict for a single `(caller, tool)` pair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AllowDecision {
    Allow,
    Deny { reason: String },
}

impl AllowDecision {
    pub fn is_allow(&self) -> bool {
        matches!(self, AllowDecision::Allow)
    }
}

/// What the filter does when neither `allow` nor `deny` mentions the
/// tool. `AllowAll` keeps the existing manifest behaviour; `DenyAll`
/// turns the role filter into a strict allow-list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DefaultPolicy {
    #[default]
    AllowAll,
    DenyAll,
}

/// Manifest-deserialisable filter configuration. Keys in `allow` and
/// `deny` are tool names; values are role lists.
///
/// `deny` always wins over `allow` — a tool that appears in both for
/// the same role is denied (defensive default; matches
/// `@RolesAllowed`-style precedence in JSR-250 / Spring Security).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RoleFilterConfig {
    /// Roles the binary's caller has been assigned. Empty list means
    /// "no roles" — under `DefaultPolicy::DenyAll` this denies
    /// everything; under `AllowAll` it allows everything (since no
    /// allow/deny entry can match).
    #[serde(default)]
    pub caller_roles: Vec<RoleId>,
    #[serde(default)]
    pub default: DefaultPolicy,
    #[serde(default)]
    pub allow: BTreeMap<ToolName, Vec<RoleId>>,
    #[serde(default)]
    pub deny: BTreeMap<ToolName, Vec<RoleId>>,
}

impl RoleFilterConfig {
    /// True when the config is an empty default (no roles, no allow,
    /// no deny, AllowAll). In that case the filter behaves exactly
    /// like the pre-v0.4.2 server.
    pub fn is_noop(&self) -> bool {
        self.caller_roles.is_empty()
            && self.allow.is_empty()
            && self.deny.is_empty()
            && self.default == DefaultPolicy::AllowAll
    }
}

/// Audit record emitted on every deny.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpRoleDenied {
    pub caller_id: String,
    pub tool_name: String,
    pub attempted_at: chrono::DateTime<chrono::Utc>,
    pub reason: String,
}

/// Sink for `McpRoleDenied` events. Implementations write to the
/// audit log (`audit_log_path` in the manifest) — for tests we use
/// an in-memory vec so we can assert on emitted records.
pub trait RoleAuditSink: Send + Sync {
    fn record_deny(&self, event: McpRoleDenied);
}

/// Vec-backed audit sink used by tests.
#[derive(Debug, Default)]
pub struct CapturingAuditSink {
    inner: std::sync::Mutex<Vec<McpRoleDenied>>,
}

impl CapturingAuditSink {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn snapshot(&self) -> Vec<McpRoleDenied> {
        self.inner.lock().expect("audit sink poisoned").clone()
    }
}

impl RoleAuditSink for CapturingAuditSink {
    fn record_deny(&self, event: McpRoleDenied) {
        self.inner.lock().expect("audit sink poisoned").push(event);
    }
}

/// The role filter contract. `MnemoServer::with_role_filter` holds an
/// `Arc<dyn RoleFilter>` so the server can be wired with either the
/// manifest-driven default or a custom filter at test time.
pub trait RoleFilter: Send + Sync {
    /// Return `Allow` or `Deny { reason }` for this caller/tool pair.
    /// Implementations are expected to also emit an audit event on
    /// deny via their configured `RoleAuditSink`.
    fn allows(&self, caller: &CallerContext, tool_name: &str) -> AllowDecision;

    /// Filter a list of advertised tool names down to the subset the
    /// caller is allowed to see. Default implementation simply runs
    /// `allows` over each name.
    fn filter_tools(&self, caller: &CallerContext, tool_names: &[String]) -> Vec<String> {
        tool_names
            .iter()
            .filter(|name| self.allows(caller, name).is_allow())
            .cloned()
            .collect()
    }
}

/// Manifest-driven implementation. Built from a [`RoleFilterConfig`]
/// + an optional [`RoleAuditSink`].
pub struct ManifestRoleFilter {
    config: RoleFilterConfig,
    audit_sink: Option<Arc<dyn RoleAuditSink>>,
}

impl ManifestRoleFilter {
    pub fn new(config: RoleFilterConfig) -> Self {
        Self {
            config,
            audit_sink: None,
        }
    }

    pub fn with_audit_sink(mut self, sink: Arc<dyn RoleAuditSink>) -> Self {
        self.audit_sink = Some(sink);
        self
    }

    /// True when this filter is the no-op identity — every call to
    /// `allows` will return `Allow` regardless of input.
    pub fn is_noop(&self) -> bool {
        self.config.is_noop()
    }

    fn caller_roles(&self) -> BTreeSet<&RoleId> {
        self.config.caller_roles.iter().collect()
    }

    fn deny_decision(
        &self,
        caller: &CallerContext,
        tool_name: &str,
        reason: &str,
    ) -> AllowDecision {
        if let Some(sink) = self.audit_sink.as_ref() {
            sink.record_deny(McpRoleDenied {
                caller_id: caller.caller_id.clone(),
                tool_name: tool_name.to_string(),
                attempted_at: chrono::Utc::now(),
                reason: reason.to_string(),
            });
        }
        AllowDecision::Deny {
            reason: reason.to_string(),
        }
    }
}

impl RoleFilter for ManifestRoleFilter {
    fn allows(&self, caller: &CallerContext, tool_name: &str) -> AllowDecision {
        // Combine manifest-declared caller_roles with any roles the
        // call site already attached to `caller.roles`. The set is
        // deduplicated via BTreeSet ordering.
        let mut roles: BTreeSet<&RoleId> = self.caller_roles();
        for r in &caller.roles {
            roles.insert(r);
        }

        // Deny always wins. Any role the caller carries that appears in
        // the deny list for this tool short-circuits to deny.
        if let Some(deny_roles) = self.config.deny.get(tool_name)
            && deny_roles.iter().any(|r| roles.contains(r))
        {
            return self.deny_decision(
                caller,
                tool_name,
                "tool denied by manifest [role_filter] deny entry",
            );
        }

        // Allow list: if the tool has an explicit allow entry, the
        // caller must carry at least one matching role. If the tool
        // has NO allow entry, fall through to the default policy.
        if let Some(allow_roles) = self.config.allow.get(tool_name) {
            if allow_roles.iter().any(|r| roles.contains(r)) {
                return AllowDecision::Allow;
            }
            return self.deny_decision(
                caller,
                tool_name,
                "tool not permitted for caller's roles by manifest [role_filter] allow entry",
            );
        }

        match self.config.default {
            DefaultPolicy::AllowAll => AllowDecision::Allow,
            DefaultPolicy::DenyAll => self.deny_decision(
                caller,
                tool_name,
                "tool not in manifest [role_filter] allow list and default policy is deny_all",
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> RoleFilterConfig {
        let mut allow = BTreeMap::new();
        allow.insert(
            "mnemo.recall".to_string(),
            vec!["auditor".into(), "agent".into()],
        );
        allow.insert("mnemo.verify".to_string(), vec!["auditor".into()]);
        allow.insert("mnemo.remember".to_string(), vec!["agent".into()]);
        let mut deny = BTreeMap::new();
        deny.insert("mnemo.forget".to_string(), vec!["auditor".into()]);
        RoleFilterConfig {
            caller_roles: vec![],
            default: DefaultPolicy::DenyAll,
            allow,
            deny,
        }
    }

    #[test]
    fn auditor_allowed_recall_and_verify_blocked_forget() {
        let filter = ManifestRoleFilter::new(cfg());
        let caller = CallerContext::new("op-1", vec!["auditor".into()]);
        assert!(filter.allows(&caller, "mnemo.recall").is_allow());
        assert!(filter.allows(&caller, "mnemo.verify").is_allow());
        assert!(!filter.allows(&caller, "mnemo.forget").is_allow());
        assert!(!filter.allows(&caller, "mnemo.remember").is_allow());
    }

    #[test]
    fn agent_allowed_remember_and_recall() {
        let filter = ManifestRoleFilter::new(cfg());
        let caller = CallerContext::new("op-2", vec!["agent".into()]);
        assert!(filter.allows(&caller, "mnemo.recall").is_allow());
        assert!(filter.allows(&caller, "mnemo.remember").is_allow());
        assert!(!filter.allows(&caller, "mnemo.verify").is_allow());
    }

    #[test]
    fn empty_config_is_noop() {
        let filter = ManifestRoleFilter::new(RoleFilterConfig::default());
        assert!(filter.is_noop());
        let caller = CallerContext::new("anyone", vec![]);
        for tool in &[
            "mnemo.remember",
            "mnemo.recall",
            "mnemo.forget",
            "mnemo.share",
            "mnemo.verify",
        ] {
            assert!(filter.allows(&caller, tool).is_allow(), "tool {tool}");
        }
    }

    #[test]
    fn deny_wins_over_allow() {
        let mut config = cfg();
        // Tool that's both allowed AND denied for the same role —
        // deny must win.
        config
            .allow
            .insert("mnemo.share".into(), vec!["auditor".into()]);
        config
            .deny
            .insert("mnemo.share".into(), vec!["auditor".into()]);
        let filter = ManifestRoleFilter::new(config);
        let caller = CallerContext::new("op-3", vec!["auditor".into()]);
        match filter.allows(&caller, "mnemo.share") {
            AllowDecision::Deny { reason } => assert!(reason.contains("deny")),
            AllowDecision::Allow => panic!("deny should have won"),
        }
    }

    #[test]
    fn filter_tools_subset() {
        let filter = ManifestRoleFilter::new(cfg());
        let caller = CallerContext::new("op-4", vec!["auditor".into()]);
        let advertised = vec![
            "mnemo.recall".to_string(),
            "mnemo.verify".to_string(),
            "mnemo.forget".to_string(),
            "mnemo.remember".to_string(),
        ];
        let visible = filter.filter_tools(&caller, &advertised);
        assert_eq!(
            visible,
            vec!["mnemo.recall".to_string(), "mnemo.verify".to_string()]
        );
    }
}
