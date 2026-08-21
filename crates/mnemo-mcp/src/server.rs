use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use rmcp::{
    ErrorData as McpError, ServerHandler,
    handler::server::{common::Extension, router::tool::ToolRouter, wrapper::Parameters},
    model::*,
    tool, tool_handler, tool_router,
};
// rmcp 2.x renamed the tool-result content type `Content` -> `ContentBlock`
// (same `::text()` / `::image()` / `::json()` constructors). Alias keeps the
// existing call sites unchanged.
use rmcp::model::ContentBlock as Content;

use mnemo_attention_state::AttentionStateStore;
use mnemo_core::model::capability::CapabilityIssuer;
use mnemo_core::model::memory::{MemoryType, Scope, SourceType};
use mnemo_core::query::MnemoEngine;
use mnemo_core::query::branch::BranchRequest;
use mnemo_core::query::checkpoint::CheckpointRequest;
use mnemo_core::query::consolidate::ConsolidateRequest;
use mnemo_core::query::experience::{RecallPlanRequest, RememberPlanRequest};
use mnemo_core::query::forget::{ForgetRequest, ForgetStrategy, ForgetSubjectRequest};
use mnemo_core::query::merge::{MergeRequest, MergeStrategy};
use mnemo_core::query::recall::{RecallRequest, TemporalRange};
use mnemo_core::query::remember::RememberRequest;
use mnemo_core::query::replay::ReplayRequest;
use mnemo_core::query::share::ShareRequest;

use crate::identity::{self, CAPABILITY_META_KEY};
use crate::role_filter::{AllowDecision, CallerContext, RoleFilter};
use crate::tools::agent_managed::{
    AGENT_MANAGED_TAG, MemForgetInput, MemReadInput, MemReviseInput, MemWriteInput,
};
use crate::tools::attention_state::{AttentionStateGetInput, AttentionStatePutInput};
use crate::tools::branch::BranchInput;
use crate::tools::checkpoint::CheckpointInput;
use crate::tools::consolidate::ConsolidateInput;
use crate::tools::delegate::DelegateInput;
use crate::tools::experience::{RecallPlanInput, RememberPlanInput};
use crate::tools::forget::ForgetInput;
use crate::tools::forget_subject::ForgetSubjectInput;
use crate::tools::merge::MergeInput;
use crate::tools::provenance::{ForgetByProvenanceInput, ProvenanceInput};
use crate::tools::recall::RecallInput;
use crate::tools::remember::RememberInput;
use crate::tools::replay::ReplayInput;
use crate::tools::share::ShareInput;
use crate::tools::trajectory_audit::TrajectoryAuditInput;
use crate::tools::verify::VerifyInput;

#[derive(Clone)]
pub struct MnemoServer {
    engine: Arc<MnemoEngine>,
    // Populated by the `#[tool_router]` macro and consumed by the macro's
    // generated code. Rustc can't see it referenced outside the macro
    // expansion, so we silence the false-positive dead_code lint.
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
    activity_tracker: Option<Arc<AtomicU64>>,
    /// v0.4.5 — optional attention-state-memory store. When set, the
    /// `mnemo.attention_state.put` and `mnemo.attention_state.get`
    /// MCP tools dispatch into it. When unset, both tools return a
    /// spec-shaped error result ("attention_state store not
    /// attached").
    attention_state: Option<Arc<dyn AttentionStateStore>>,
    /// v0.5.19 — optional role-aware tool filter. When set, `list_tools`
    /// hides tools the caller's roles don't permit AND `call_tool` rejects a
    /// denied tool by name with a spec-compliant `-32601` error (so a caller
    /// cannot invoke a tool it was never shown). When unset, all tools are
    /// exposed and every call passes (byte-for-byte pre-v0.5.19 behaviour).
    /// See [`crate::role_filter`].
    role_filter: Option<Arc<dyn RoleFilter>>,
    /// Per-request identity verifier ([ADR 0002](../../../docs/adr/0002-request-identity-model.md)).
    ///
    /// When set, a capability presented in a request's `_meta` is verified and
    /// becomes the caller's identity for that **one call**. When unset, a
    /// request that presents a capability is *rejected* rather than downgraded
    /// to the boot identity — see [`crate::identity`] for the fail-closed table.
    ///
    /// Unset is the default and keeps pre-ADR-0002 behaviour byte-for-byte:
    /// no capability presented, boot-derived identity, exactly as on stdio today.
    capability_issuer: Option<Arc<CapabilityIssuer>>,
    /// Capability-leased reads ([#126](https://github.com/sattyamjjain/mnemo/issues/126)).
    ///
    /// When attached, `mnemo.recall` mints a short-lived lease bound to the
    /// caller's per-request identity, and `mnemo.forget_subject` **requires**
    /// one — binding the destructive act to a read the same caller just
    /// performed (the OX-MCP exfiltrate-then-act chain).
    ///
    /// Unattached (the default) both tools behave exactly as they always have.
    /// Enforcing unconditionally would break every shipped client of a
    /// docs-drift-tested tool on upgrade; that is the operator's call.
    lease_store: Option<Arc<crate::lease::LeaseStore>>,
}

impl MnemoServer {
    /// Freshness hint for `tools/list` ([SEP-2549] `ttlMs`).
    ///
    /// The catalog a caller sees is the compiled-in tool router filtered by that
    /// caller's roles, so for a fixed capability it does not change while the
    /// process runs. It is still only a minute: the filter is driven by an
    /// operator-supplied manifest, and a caller holding an hour-old catalog
    /// across a manifest change would keep calling tools it can no longer see.
    /// A short hint costs one extra listing; a long one costs correctness.
    ///
    /// [SEP-2549]: https://github.com/modelcontextprotocol/modelcontextprotocol/pull/2549
    const TOOLS_LIST_TTL_MS: u64 = 60_000;

    /// Freshness hint for `resources/list`.
    ///
    /// Zero, meaning immediately stale. Resources here are memory records, and
    /// any `mnemo.remember` changes the set, so a listing is accurate only at
    /// the moment it is produced. Advertising any positive TTL would invite a
    /// client to serve a list that a write has already invalidated.
    const RESOURCES_LIST_TTL_MS: u64 = 0;

    fn touch_activity(&self) {
        if let Some(ref t) = self.activity_tracker {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            t.store(now, Ordering::Relaxed);
        }
    }

    /// The **boot-derived** caller context — the fallback used only when a
    /// request presents no capability at all.
    ///
    /// On stdio with no capability in `_meta` this is the whole story: the
    /// binary's operator is the caller, and the caller's roles are declared in
    /// the manifest and held inside the [`RoleFilter`] itself (see
    /// [`crate::role_filter::ManifestRoleFilter`]). We pass the server's default
    /// agent id as the opaque caller id (for audit) and an empty role vec; the
    /// filter combines it with its manifest roles.
    ///
    /// Prefer [`Self::resolve_caller`] on any path that has a
    /// [`RequestMetaObject`] in hand: per [ADR
    /// 0002](../../../docs/adr/0002-request-identity-model.md) identity is
    /// per-request, and this method cannot see the request.
    fn boot_caller_context(&self) -> CallerContext {
        CallerContext::new(self.engine.default_agent_id.clone(), Vec::new())
    }

    /// Resolve the caller for **one request** from its `_meta`
    /// ([ADR 0002](../../../docs/adr/0002-request-identity-model.md)).
    ///
    /// A capability under [`CAPABILITY_META_KEY`] is verified (signature,
    /// expiry, key id) and its principal becomes the caller id with its
    /// `role:`-prefixed scope tokens as roles. No capability → the boot
    /// fallback. A capability that cannot be verified → an error; it is never
    /// downgraded to the boot identity, because that would hand a forged token
    /// the operator's authority. The full table is in [`crate::identity`].
    ///
    /// Rejections surface as `-32602` (invalid params) with the reason in the
    /// message: the credential travelled in the request's parameters, and MCP
    /// has no dedicated authentication error code.
    ///
    /// `pub` so the wiring is testable without a live MCP service harness —
    /// constructing the [`rmcp::service::RequestContext`] that `call_tool` and
    /// `list_tools` take is not practical in a unit test, but a
    /// [`rmcp::model::RequestMetaObject`] is. Same reasoning as
    /// [`Self::visible_tool_names`].
    pub fn resolve_caller(
        &self,
        meta: &rmcp::model::RequestMetaObject,
        extensions: &rmcp::model::Extensions,
    ) -> Result<CallerContext, McpError> {
        // On the streamable-HTTP transport rmcp injects the `http::request::Parts`
        // into the request extensions, so an `Authorization: Bearer <capability>`
        // header lands in the *same* resolution path as `_meta`. On stdio there
        // are no parts and this is simply `None`.
        // Presence of HTTP parts means this request came in over the network
        // rather than the operator's own stdio pipe. On stdio, "no capability"
        // legitimately means the operator; over HTTP it would mean *anyone who
        // can reach the port* acting as the operator, so the fallback is
        // disabled there and an unauthenticated request is rejected.
        let http_parts = extensions.get::<http::request::Parts>();
        let require_capability = http_parts.is_some();

        let bearer = http_parts
            .and_then(|parts| parts.headers.get(identity::AUTHORIZATION_HEADER))
            .map(|value| {
                value
                    .to_str()
                    .map_err(|_| {
                        McpError::invalid_params(
                            "`Authorization` header is not valid UTF-8".to_string(),
                            None,
                        )
                    })
                    .and_then(|raw| {
                        identity::capability_from_bearer(raw)
                            .map_err(|err| McpError::invalid_params(err.to_string(), None))
                    })
            })
            .transpose()?;

        identity::resolve_caller_from(
            meta.0.0.get(CAPABILITY_META_KEY),
            bearer.as_ref(),
            self.capability_issuer.as_deref(),
            &self.engine.default_agent_id,
            require_capability,
        )
        .map_err(|err| McpError::invalid_params(err.to_string(), None))
    }

    /// The calling agent's id — **the single identity source in this server**,
    /// the same one the role filter gates on.
    ///
    /// This exists so there is exactly ONE identity source in the server.
    /// `list_resources` previously reached into `engine.default_agent_id`
    /// directly while tool gating went through [`Self::caller_context`]. On
    /// stdio both resolve to the same string, so the divergence was invisible —
    /// and it is precisely the divergence
    /// [ADR 0002](../../../docs/adr/0002-request-identity-model.md) warns about:
    /// under a multi-caller transport, a boot-derived `default_agent_id` would
    /// serve one agent's memories to *every* authenticated caller while the
    /// tool filter correctly gated per caller. A read leak behind a correct-
    /// looking gate.
    ///
    /// Routing both through here means the HTTP follow-up changes
    /// `caller_context()` once and resource scoping moves with it, rather than
    /// requiring someone to remember this second site.
    /// `identity_sources_agree` pins that they cannot drift apart again.
    pub fn caller_agent_id(&self) -> String {
        self.boot_caller_context().caller_id
    }

    /// The tool names visible to the current caller under the attached
    /// [`RoleFilter`] — exactly the set `tools/list` returns. All registered
    /// tools when no filter is attached. Exposed so the filtering decision is
    /// unit-testable without a live MCP service harness (which the
    /// `RequestContext` that `list_tools`/`call_tool` take requires).
    pub fn visible_tool_names(&self) -> Vec<String> {
        self.visible_tool_names_for(&self.boot_caller_context())
    }

    /// [`Self::visible_tool_names`] for an explicitly-resolved caller.
    ///
    /// `tools/list` uses this with the request's verified identity so a
    /// capability's roles decide the catalog, rather than a boot-time constant
    /// that would show every caller the same tools.
    pub fn visible_tool_names_for(&self, caller: &CallerContext) -> Vec<String> {
        let all: Vec<String> = self
            .tool_router
            .list_all()
            .into_iter()
            .map(|t| t.name.to_string())
            .collect();
        match &self.role_filter {
            Some(filter) => filter.filter_tools(caller, &all),
            None => all,
        }
    }

    /// `Some(reason)` when the attached [`RoleFilter`] denies calling
    /// `tool_name` for the current caller — the exact gate `tools/call` applies
    /// before dispatch. `None` (allowed) when no filter is attached or the
    /// filter permits the call.
    pub fn tool_call_denial(&self, tool_name: &str) -> Option<String> {
        self.tool_call_denial_for(&self.boot_caller_context(), tool_name)
    }

    /// [`Self::tool_call_denial`] for an explicitly-resolved caller — the gate
    /// `tools/call` applies, using the request's verified identity.
    pub fn tool_call_denial_for(&self, caller: &CallerContext, tool_name: &str) -> Option<String> {
        match &self.role_filter {
            Some(filter) => match filter.allows(caller, tool_name) {
                AllowDecision::Deny { reason } => Some(reason),
                AllowDecision::Allow => None,
            },
            None => None,
        }
    }

    /// The advertised tool catalog as `(name, description, input_schema_json)`
    /// triples — exactly what `tools/list` publishes, taken from
    /// `tool_router.list_all()` and filtered through any attached [`RoleFilter`]
    /// (so a pin is attested against what callers actually see, not the
    /// pre-filter superset). `description` is the empty string when the tool
    /// registers none; `input_schema_json` is the canonical `serde_json`
    /// encoding of the tool's `input_schema`. Consumed by the CLI's hardened-mode
    /// tool-catalog attestation and its `--print-catalog-pin` generator.
    pub fn advertised_tool_catalog(&self) -> Vec<(String, String, String)> {
        let visible: std::collections::HashSet<String> =
            self.visible_tool_names().into_iter().collect();
        self.tool_router
            .list_all()
            .into_iter()
            .filter(|t| visible.contains(t.name.as_ref()))
            .map(|t| {
                (
                    t.name.to_string(),
                    t.description.as_deref().unwrap_or("").to_string(),
                    serde_json::to_string(&*t.input_schema).unwrap_or_default(),
                )
            })
            .collect()
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// JSON view of a write-provenance record, with hashes as hex strings so the
/// shape is stable across the REST / MCP / SDK surfaces.
fn provenance_to_json(
    p: &mnemo_core::model::write_provenance::WriteProvenance,
) -> serde_json::Value {
    serde_json::json!({
        "id": p.id.to_string(),
        "memory_id": p.memory_id.to_string(),
        "principal": p.principal,
        "capability_id": p.capability_id.map(|c| c.to_string()),
        "session_id": p.session_id,
        "op": p.op.as_str(),
        "authored_at": p.authored_at.to_rfc3339(),
        "flags": p.flags.iter().map(|f| f.as_str()).collect::<Vec<_>>(),
        "content_hash": hex_encode(&p.content_hash),
        "prev_hash": p.prev_hash.as_deref().map(hex_encode),
    })
}

#[tool_router]
impl MnemoServer {
    pub fn new(engine: Arc<MnemoEngine>) -> Self {
        Self {
            engine,
            tool_router: Self::tool_router(),
            activity_tracker: None,
            attention_state: None,
            role_filter: None,
            capability_issuer: None,
            lease_store: None,
        }
    }

    /// Enable capability-leased reads ([#126](https://github.com/sattyamjjain/mnemo/issues/126)).
    ///
    /// With a store attached, `mnemo.recall` returns a `lease` in its result
    /// and `mnemo.forget_subject` requires a `lease_token` argument naming a
    /// live, correctly-scoped lease bound to the calling principal.
    ///
    /// **This changes two shipped tools' contracts**, so it is opt-in. It is
    /// also only meaningful alongside per-request identity
    /// ([`Self::with_capability_issuer`]): a lease bound to a boot-time agent
    /// id proves nothing when every caller shares it.
    pub fn with_lease_store(mut self, store: Arc<crate::lease::LeaseStore>) -> Self {
        self.lease_store = Some(store);
        self
    }

    pub fn with_activity_tracker(mut self, tracker: Arc<AtomicU64>) -> Self {
        self.activity_tracker = Some(tracker);
        self
    }

    /// Attach a capability issuer, enabling **per-request identity**
    /// ([ADR 0002](../../../docs/adr/0002-request-identity-model.md)).
    ///
    /// With an issuer attached, a request may present an HMAC-signed
    /// [`CapabilityIssuer`]-minted token under the `dev.mnemo/capability`
    /// `_meta` key; it is verified per call and its principal becomes the
    /// caller identity for that call only. Requests without a capability keep
    /// the boot-derived identity, so attaching an issuer does not break
    /// existing clients.
    ///
    /// Without an issuer, presenting a capability is an error rather than a
    /// silent downgrade — see [`crate::identity`].
    pub fn with_capability_issuer(mut self, issuer: Arc<CapabilityIssuer>) -> Self {
        self.capability_issuer = Some(issuer);
        self
    }

    /// v0.5.19 — attach a role-aware tool filter ([`RoleFilter`]). When set,
    /// `tools/list` hides tools the caller's roles don't permit and `tools/call`
    /// rejects a denied tool by name with a spec-compliant `-32601` error — so a
    /// caller can neither see nor invoke a tool it isn't allowed. A denied call
    /// returns a structured MCP error, never a silent empty result. Unset keeps
    /// the pre-v0.5.19 behaviour (all tools exposed). Typically built from the
    /// manifest `[role_filter]` block via
    /// [`ManifestRoleFilter`](crate::role_filter::ManifestRoleFilter).
    pub fn with_role_filter(mut self, filter: Arc<dyn RoleFilter>) -> Self {
        self.role_filter = Some(filter);
        self
    }

    /// v0.4.5 — attach an attention-state-memory store
    /// ([`mnemo_attention_state::AttentionStateStore`]). When set, the
    /// `mnemo.attention_state.put` + `.get` MCP tools dispatch into it;
    /// when unset, both tools return a spec-shaped error result rather
    /// than panicking. Anchored on
    /// [arXiv:2605.18226](https://arxiv.org/abs/2605.18226).
    pub fn with_attention_state(mut self, store: Arc<dyn AttentionStateStore>) -> Self {
        self.attention_state = Some(store);
        self
    }

    #[tool(
        name = "mnemo.remember",
        description = "Store a new memory. Use this to save facts, preferences, instructions, experiences, or any information that should be remembered for later. Memories are searchable by semantic similarity and keyword search."
    )]
    async fn remember(
        &self,
        Parameters(input): Parameters<RememberInput>,
    ) -> Result<CallToolResult, McpError> {
        self.touch_activity();
        let memory_type = match input.memory_type {
            Some(ref s) => match s.parse::<MemoryType>() {
                Ok(mt) => Some(mt),
                Err(_) => {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "invalid memory_type '{}': expected one of: episodic, semantic, procedural, working",
                        s
                    ))]));
                }
            },
            None => None,
        };
        let scope = match input.scope {
            Some(ref s) => match s.parse::<Scope>() {
                Ok(sc) => Some(sc),
                Err(_) => {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "invalid scope '{}': expected one of: private, shared, public, global",
                        s
                    ))]));
                }
            },
            None => None,
        };

        let source_type = match input.source_type {
            Some(ref s) => match parse_source_type(s) {
                Some(st) => Some(st),
                None => {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "invalid source_type '{}': expected one of: agent, human, system, user_input, tool_output, model_response, retrieval, consolidation, import",
                        s
                    ))]));
                }
            },
            None => None,
        };

        let mut request = RememberRequest::new(input.content);
        request.memory_type = memory_type;
        request.scope = scope;
        request.importance = input.importance;
        request.tags = input.tags;
        request.metadata = input.metadata;
        request.source_type = source_type;
        request.source_id = input.source_id;
        request.org_id = input.org_id;
        request.thread_id = input.thread_id;
        request.ttl_seconds = input.ttl_seconds;
        request.related_to = input.related_to;
        request.decay_rate = input.decay_rate;
        request.created_by = input.created_by;

        match self.engine.remember(request).await {
            Ok(response) => {
                let result = serde_json::json!({
                    "id": response.id.to_string(),
                    "content_hash": response.content_hash,
                    "status": "remembered"
                });
                Ok(CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&result)
                        .unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}")),
                )]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(
        name = "mnemo.recall",
        description = "Search and retrieve memories. Supports semantic search (vector similarity), lexical search (keyword BM25), and hybrid search (combining both with recency). Returns the most relevant memories ranked by score."
    )]
    async fn recall(
        &self,
        Parameters(input): Parameters<RecallInput>,
        Extension(caller): Extension<CallerContext>,
    ) -> Result<CallToolResult, McpError> {
        self.touch_activity();
        let memory_type = match input.memory_type {
            Some(ref s) => match s.parse::<MemoryType>() {
                Ok(mt) => Some(mt),
                Err(_) => {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "invalid memory_type '{}': expected one of: episodic, semantic, procedural, working",
                        s
                    ))]));
                }
            },
            None => None,
        };

        let memory_types = match input.memory_types {
            Some(ref types) => {
                let mut parsed = Vec::with_capacity(types.len());
                for s in types {
                    match s.parse::<MemoryType>() {
                        Ok(mt) => parsed.push(mt),
                        Err(_) => {
                            return Ok(CallToolResult::error(vec![Content::text(format!(
                                "invalid memory_type '{}' in memory_types: expected one of: episodic, semantic, procedural, working",
                                s
                            ))]));
                        }
                    }
                }
                Some(parsed)
            }
            None => None,
        };

        let scope = match input.scope {
            Some(ref s) => match s.parse::<Scope>() {
                Ok(sc) => Some(sc),
                Err(_) => {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "invalid scope '{}': expected one of: private, shared, public, global",
                        s
                    ))]));
                }
            },
            None => None,
        };

        let temporal_range = input.temporal_range.map(|tr| {
            let mut range = TemporalRange::new();
            range.after = tr.after;
            range.before = tr.before;
            range
        });

        let mut request = RecallRequest::new(input.query);
        request.limit = input.limit;
        request.memory_type = memory_type;
        request.memory_types = memory_types;
        request.scope = scope;
        request.min_importance = input.min_importance;
        request.tags = input.tags;
        request.org_id = input.org_id;
        request.strategy = input.strategy;
        request.temporal_range = temporal_range;
        request.recency_half_life_hours = input.recency_half_life_hours;
        request.hybrid_weights = input.hybrid_weights;
        request.rrf_k = input.rrf_k;
        request.as_of = input.as_of;
        request.explain = input.explain;
        request.current_fact_resolver = input.current_fact_resolver.map(|c| {
            mnemo_core::query::current_fact_resolver::CurrentFactResolverConfig {
                fact_key: c.fact_key,
                include_supersession_chain: c.include_supersession_chain.unwrap_or(false),
            }
        });
        request.orientation_cache = input.orientation_cache.map(|o| {
            mnemo_core::query::orientation_cache::OrientationCacheConfig {
                namespace: o.namespace,
                token_budget: o.token_budget,
                include_in_response: o.include_in_response.unwrap_or(true),
                distill: o.distill.unwrap_or(true),
            }
        });
        // v0.4.15 — domain-scoped recall. A non-empty `domain_scope`
        // restricts the candidate set to the metadata sub-corpus before
        // the dense step and selects the DomainScoped mode.
        if let Some(ds) = input.domain_scope {
            let scope = mnemo_core::retrieval::DomainScope {
                org_id: ds.org_id,
                namespace: ds.namespace,
                doc_class: ds.doc_class,
                tags: ds.tags,
            };
            if !scope.is_empty() {
                request.domain_scope = Some(scope);
                request.mode = Some(mnemo_core::retrieval::RetrievalMode::DomainScoped);
            }
        }

        match self.engine.recall(request).await {
            Ok(response) => {
                let mut result = serde_json::json!({
                    "memories": response.memories,
                    "total": response.total,
                });
                // #126 — mint a lease bound to THIS caller, so a following
                // `forget_subject` can prove it is acting on a read this same
                // principal just performed. Additive: absent unless the
                // operator attached a store.
                if let Some(store) = self.lease_store.as_ref() {
                    // #160 — narrow the lease to the subjects this read actually
                    // covered. Read off the RETURNED records' `subject:` tags,
                    // not inferred from the query: the response is what the
                    // caller was handed, so there is nothing to over- or
                    // under-estimate. A read that surfaced no subject-tagged
                    // record yields an empty set, which authorises no erasure —
                    // fail-closed, and the refusal message says why.
                    let subjects: std::collections::BTreeSet<String> = response
                        .memories
                        .iter()
                        .flat_map(|m| m.tags.iter())
                        .filter_map(|t| {
                            t.strip_prefix(mnemo_core::query::forget::SUBJECT_TAG_PREFIX)
                        })
                        .filter(|s| !s.is_empty())
                        .map(|s| s.to_string())
                        .collect();
                    let lease = store.issue(
                        &caller.caller_id,
                        std::iter::once(crate::lease::LeaseScope::ForgetSubject).collect(),
                        subjects,
                    );
                    result["lease"] = serde_json::json!({
                        "token": lease.id.to_string(),
                        "agent_id": lease.agent_id,
                        "scopes": lease.scopes.iter().map(|s| s.name()).collect::<Vec<_>>(),
                        "subjects": lease.subjects.iter().collect::<Vec<_>>(),
                        "ttl_seconds": store.ttl_seconds(),
                    });
                }
                if let Some(superseded) = response.superseded.as_ref() {
                    result["superseded"] = serde_json::to_value(superseded).unwrap_or_default();
                }
                if let Some(orientation) = response.orientation_cache.as_ref() {
                    result["orientation_cache"] =
                        serde_json::to_value(orientation).unwrap_or_default();
                }
                // v0.5.1 — active-reconstruction belief-state node (MRAgent,
                // arXiv:2606.06036), present when strategy = "reconstruct".
                if let Some(reconstruction) = response.reconstruction.as_ref() {
                    result["reconstruction"] =
                        serde_json::to_value(reconstruction).unwrap_or_default();
                }
                Ok(CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&result)
                        .unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}")),
                )]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(
        name = "mnemo.forget",
        description = "Delete one or more memories by ID. Supports soft delete (recoverable) or hard delete (permanent). Use this to remove outdated, incorrect, or no longer needed information."
    )]
    async fn forget(
        &self,
        Parameters(input): Parameters<ForgetInput>,
    ) -> Result<CallToolResult, McpError> {
        self.touch_activity();
        let memory_ids: Result<Vec<uuid::Uuid>, _> = input
            .memory_ids
            .iter()
            .map(|s| uuid::Uuid::parse_str(s))
            .collect();

        let memory_ids = match memory_ids {
            Ok(ids) => ids,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "invalid UUID: {e}"
                ))]));
            }
        };

        let strategy = match input.strategy {
            Some(ref s) => match s.as_str() {
                "soft_delete" => Some(ForgetStrategy::SoftDelete),
                "hard_delete" => Some(ForgetStrategy::HardDelete),
                "decay" => Some(ForgetStrategy::Decay),
                "consolidate" => Some(ForgetStrategy::Consolidate),
                "archive" => Some(ForgetStrategy::Archive),
                "redact" => Some(ForgetStrategy::Redact),
                unknown => {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "invalid strategy '{}': expected one of: soft_delete, hard_delete, decay, consolidate, archive, redact",
                        unknown
                    ))]));
                }
            },
            None => None,
        };

        let criteria = match input.criteria {
            Some(c) => {
                let memory_type = match c.memory_type {
                    Some(ref s) => match s.parse::<MemoryType>() {
                        Ok(mt) => Some(mt),
                        Err(_) => {
                            return Ok(CallToolResult::error(vec![Content::text(format!(
                                "invalid memory_type '{}' in criteria: expected one of: episodic, semantic, procedural, working",
                                s
                            ))]));
                        }
                    },
                    None => None,
                };
                Some(mnemo_core::query::forget::ForgetCriteria {
                    max_age_hours: c.max_age_hours,
                    min_importance_below: c.min_importance_below,
                    memory_type,
                    tags: c.tags,
                })
            }
            None => None,
        };

        let mut request = ForgetRequest::new(memory_ids);
        request.strategy = strategy;
        request.criteria = criteria;

        match self.engine.forget(request).await {
            Ok(response) => {
                let result = serde_json::json!({
                    "forgotten": response.forgotten.iter().map(|id| id.to_string()).collect::<Vec<_>>(),
                    "errors": response.errors,
                    "status": "forgotten"
                });
                Ok(CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&result)
                        .unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}")),
                )]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(
        name = "mnemo.provenance",
        description = "Read the write-provenance of memories: who wrote each one, under what capability, in what session, when. Provide exactly one selector: `memory_id` (one record), `principal` (everything a writer authored), or `session_id` (everything written under a session/trace). Provenance is queryable audit history and survives forgetting."
    )]
    async fn provenance(
        &self,
        Parameters(input): Parameters<ProvenanceInput>,
    ) -> Result<CallToolResult, McpError> {
        self.touch_activity();
        let limit = input.limit.unwrap_or(1000).min(10_000);
        let selectors = [
            input.memory_id.is_some(),
            input.principal.is_some(),
            input.session_id.is_some(),
        ]
        .iter()
        .filter(|b| **b)
        .count();
        if selectors != 1 {
            return Ok(CallToolResult::error(vec![Content::text(
                "provide exactly one of: memory_id, principal, session_id",
            )]));
        }

        let result = if let Some(mid) = input.memory_id {
            let id = match uuid::Uuid::parse_str(&mid) {
                Ok(id) => id,
                Err(e) => {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "invalid memory_id UUID: {e}"
                    ))]));
                }
            };
            match self.engine.write_provenance_for(id).await {
                Ok(Some(p)) => provenance_to_json(&p),
                Ok(None) => serde_json::Value::Null,
                Err(e) => return Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
            }
        } else if let Some(principal) = input.principal {
            match self.engine.writes_by_principal(&principal, limit).await {
                Ok(writes) => {
                    serde_json::Value::Array(writes.iter().map(provenance_to_json).collect())
                }
                Err(e) => return Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
            }
        } else {
            let session_id = input.session_id.unwrap_or_default();
            match self.engine.writes_by_session(&session_id, limit).await {
                Ok(writes) => {
                    serde_json::Value::Array(writes.iter().map(provenance_to_json).collect())
                }
                Err(e) => return Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
            }
        };

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&result)
                .unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}")),
        )]))
    }

    #[tool(
        name = "mnemo.forget_by_provenance",
        description = "FORGET BY PROVENANCE: revoke every memory a principal (or session/trace) authored, in one call. Remediation targeted at the responsible writer, not an indiscriminate wipe. Provide exactly one of `principal` or `session_id`; strategy is soft_delete (default), hard_delete, or redact. The provenance audit trail survives — wiping is not remediation."
    )]
    async fn forget_by_provenance(
        &self,
        Parameters(input): Parameters<ForgetByProvenanceInput>,
    ) -> Result<CallToolResult, McpError> {
        self.touch_activity();
        let strategy = match input.strategy.as_deref().unwrap_or("soft_delete") {
            "soft_delete" => ForgetStrategy::SoftDelete,
            "hard_delete" => ForgetStrategy::HardDelete,
            "redact" => ForgetStrategy::Redact,
            unknown => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "invalid strategy '{unknown}': expected one of: soft_delete, hard_delete, redact"
                ))]));
            }
        };

        let response = match (input.principal, input.session_id) {
            (Some(p), None) => self.engine.forget_by_principal(&p, strategy).await,
            (None, Some(s)) => self.engine.forget_by_session(&s, strategy).await,
            (Some(_), Some(_)) => {
                return Ok(CallToolResult::error(vec![Content::text(
                    "provide exactly one of principal or session_id, not both",
                )]));
            }
            (None, None) => {
                return Ok(CallToolResult::error(vec![Content::text(
                    "provide one of principal or session_id",
                )]));
            }
        };

        match response {
            Ok(response) => {
                let result = serde_json::json!({
                    "forgotten": response.forgotten.iter().map(|id| id.to_string()).collect::<Vec<_>>(),
                    "errors": response.errors,
                    "status": "forgotten"
                });
                Ok(CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&result)
                        .unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}")),
                )]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(
        name = "mnemo.forget_subject",
        description = "GDPR / DPDPA-aligned subject erasure. Finds every memory tagged with `subject:<subject_id>` and either redacts the content (default, preserves the audit hash chain) or hard-deletes the rows. Use 'redact' when a verifiable audit trail must survive the erasure."
    )]
    async fn forget_subject(
        &self,
        Parameters(input): Parameters<ForgetSubjectInput>,
        Extension(caller): Extension<CallerContext>,
    ) -> Result<CallToolResult, McpError> {
        self.touch_activity();
        // #126 — capability-leased read gate. Enforced only when a store is
        // attached; unattached, this is the pre-#126 behaviour exactly.
        //
        // The lease is checked against the CALLER's per-request identity, not
        // the requested `agent_id`: the question is "did *you* just read?", and
        // letting the caller name the agent the lease is checked against would
        // let it answer its own question.
        //
        // #160 — and against the requested `subject_id`, which the lease must
        // already cover. That is what makes the question "did you just read
        // *this subject*?" rather than merely "did you just read?", so an
        // injected wide delete cannot ride a narrow read's lease.
        if let Some(store) = self.lease_store.as_ref() {
            let Some(ref raw) = input.lease_token else {
                return Ok(CallToolResult::error(vec![Content::text(
                    "this server requires a lease for mnemo.forget_subject: call mnemo.recall \
                     first and pass the `lease` token it returns as `lease_token`. The lease \
                     binds this erasure to a read you just performed."
                        .to_string(),
                )]));
            };
            let token_id = match raw.parse::<uuid::Uuid>() {
                Ok(id) => id,
                Err(_) => {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "lease_token '{raw}' is not a valid lease id (expected the `lease.token` \
                         value from a mnemo.recall result)"
                    ))]));
                }
            };
            if let Err(err) = store.check(
                token_id,
                &caller.caller_id,
                crate::lease::LeaseScope::ForgetSubject,
                &input.subject_id,
            ) {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "lease rejected: {err}"
                ))]));
            }
        }
        let strategy = match input.strategy.as_deref().unwrap_or("redact") {
            "redact" => ForgetStrategy::Redact,
            "hard_delete" => ForgetStrategy::HardDelete,
            "soft_delete" => ForgetStrategy::SoftDelete,
            unknown => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "invalid strategy '{}': expected one of: redact, hard_delete, soft_delete",
                    unknown
                ))]));
            }
        };

        let request = ForgetSubjectRequest {
            subject_id: input.subject_id,
            agent_id: input.agent_id,
            strategy,
        };

        match self.engine.forget_subject(request).await {
            Ok(response) => {
                let result = serde_json::json!({
                    "subject_id": response.subject_id,
                    "strategy": response.strategy,
                    "matched": response.matched,
                    "forgotten": response.forgotten.iter().map(|id| id.to_string()).collect::<Vec<_>>(),
                    "cascaded_events": response.cascaded_events,
                    "errors": response.errors,
                });
                Ok(CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&result)
                        .unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}")),
                )]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(
        name = "mnemo.share",
        description = "Share one or more memories with another agent by granting them access permissions. Supports batch sharing via memory_ids. The memory scope will be updated to 'shared' automatically."
    )]
    async fn share(
        &self,
        Parameters(input): Parameters<ShareInput>,
    ) -> Result<CallToolResult, McpError> {
        self.touch_activity();

        // Support batch: memory_ids takes precedence over memory_id
        let id_strings = input
            .memory_ids
            .unwrap_or_else(|| vec![input.memory_id.clone()]);

        let permission = match input.permission {
            Some(ref s) => match s.parse() {
                Ok(p) => Some(p),
                Err(_) => {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "invalid permission '{}': expected one of: read, write, delete, share, delegate, admin",
                        s
                    ))]));
                }
            },
            None => None,
        };

        let mut all_acl_ids: Vec<String> = Vec::new();
        let mut all_shared_with: Vec<String> = Vec::new();
        let mut errors: Vec<String> = Vec::new();

        for id_str in &id_strings {
            let memory_id = match uuid::Uuid::parse_str(id_str) {
                Ok(id) => id,
                Err(e) => {
                    errors.push(format!("invalid UUID '{id_str}': {e}"));
                    continue;
                }
            };

            let mut request = ShareRequest::new(memory_id, input.target_agent_id.clone());
            request.target_agent_ids = input.target_agent_ids.clone();
            request.permission = permission;
            request.expires_in_hours = input.expires_in_hours;

            match self.engine.share(request).await {
                Ok(response) => {
                    for acl_id in &response.acl_ids {
                        all_acl_ids.push(acl_id.to_string());
                    }
                    if all_shared_with.is_empty() {
                        all_shared_with = response.shared_with_all;
                    }
                }
                Err(e) => {
                    errors.push(format!("share {id_str}: {e}"));
                }
            }
        }

        let result = serde_json::json!({
            "acl_ids": all_acl_ids,
            "memory_ids": id_strings,
            "shared_with": all_shared_with,
            "errors": errors,
            "status": "shared"
        });
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&result)
                .unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}")),
        )]))
    }

    #[tool(
        name = "mnemo.checkpoint",
        description = "Create a checkpoint to snapshot the current agent state. Checkpoints capture the state, active memories, and event cursor at a point in time, enabling git-like state management."
    )]
    async fn checkpoint(
        &self,
        Parameters(input): Parameters<CheckpointInput>,
    ) -> Result<CallToolResult, McpError> {
        self.touch_activity();
        let mut request = CheckpointRequest::new(input.thread_id, input.state_snapshot);
        request.branch_name = input.branch_name;
        request.label = input.label;
        request.metadata = input.metadata;

        match self.engine.checkpoint(request).await {
            Ok(response) => {
                let result = serde_json::json!({
                    "checkpoint_id": response.id.to_string(),
                    "parent_id": response.parent_id.map(|id| id.to_string()),
                    "branch_name": response.branch_name,
                    "status": "checkpointed"
                });
                Ok(CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&result)
                        .unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}")),
                )]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(
        name = "mnemo.consolidate",
        description = "Consolidate a set of related memories into one revisable topic document (Infini-Memory). Collects the members as evidence, preserves provenance (sources, timestamps, confidence), and records a hash-chained audit event. Pass `supersede` with an existing topic document id to revise a fact while keeping the old version in history."
    )]
    async fn consolidate(
        &self,
        Parameters(input): Parameters<ConsolidateInput>,
    ) -> Result<CallToolResult, McpError> {
        self.touch_activity();

        let mut memory_ids = Vec::with_capacity(input.memory_ids.len());
        for s in &input.memory_ids {
            match uuid::Uuid::parse_str(s) {
                Ok(id) => memory_ids.push(id),
                Err(e) => {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "invalid memory id '{s}': {e}"
                    ))]));
                }
            }
        }

        let supersede = match input.supersede.as_deref() {
            Some(s) => match uuid::Uuid::parse_str(s) {
                Ok(id) => Some(id),
                Err(e) => {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "invalid supersede id '{s}': {e}"
                    ))]));
                }
            },
            None => None,
        };

        let mut request = ConsolidateRequest::new(memory_ids, input.topic_name);
        request.agent_id = input.agent_id;
        request.summary = input.summary;
        request.supersede = supersede;
        request.thread_id = input.thread_id;
        request.metadata = input.metadata;

        match self.engine.consolidate(request).await {
            Ok(response) => {
                let result = serde_json::json!({
                    "topic_document_id": response.topic_document_id.to_string(),
                    "topic_name": response.topic_name,
                    "source_count": response.source_count,
                    "version": response.version,
                    "superseded_id": response.superseded_id.map(|id| id.to_string()),
                    "member_ids": response.member_ids.iter().map(|id| id.to_string()).collect::<Vec<_>>(),
                    "content_hash": response.content_hash,
                    "consolidation_event_id": response.consolidation_event_id.to_string(),
                    "revision_event_id": response.revision_event_id.map(|id| id.to_string()),
                    "status": "consolidated"
                });
                Ok(CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&result)
                        .unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}")),
                )]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(
        name = "mnemo.branch",
        description = "Fork the current state into a new branch for exploration. Creates a new branch from an existing checkpoint, copying the state snapshot and memory references."
    )]
    async fn branch(
        &self,
        Parameters(input): Parameters<BranchInput>,
    ) -> Result<CallToolResult, McpError> {
        self.touch_activity();
        let source_checkpoint_id = input
            .source_checkpoint_id
            .and_then(|s| uuid::Uuid::parse_str(&s).ok());

        let mut request = BranchRequest::new(input.thread_id, input.new_branch_name);
        request.source_checkpoint_id = source_checkpoint_id;
        request.source_branch = input.source_branch;

        match self.engine.branch(request).await {
            Ok(response) => {
                let result = serde_json::json!({
                    "checkpoint_id": response.checkpoint_id.to_string(),
                    "branch_name": response.branch_name,
                    "source_checkpoint_id": response.source_checkpoint_id.to_string(),
                    "status": "branched"
                });
                Ok(CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&result)
                        .unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}")),
                )]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(
        name = "mnemo.merge",
        description = "Merge a branch back into another branch. Supports full merge (all memories), cherry-pick (specific memories), and squash strategies."
    )]
    async fn merge(
        &self,
        Parameters(input): Parameters<MergeInput>,
    ) -> Result<CallToolResult, McpError> {
        self.touch_activity();
        let strategy = match input.strategy.as_deref() {
            Some("full_merge") => Some(MergeStrategy::FullMerge),
            Some("cherry_pick") => Some(MergeStrategy::CherryPick),
            Some("squash") => Some(MergeStrategy::Squash),
            Some(unknown) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "invalid strategy '{}': expected one of: full_merge, cherry_pick, squash",
                    unknown
                ))]));
            }
            None => None,
        };

        let cherry_pick_ids = input.cherry_pick_ids.map(|ids| {
            ids.iter()
                .filter_map(|s| uuid::Uuid::parse_str(s).ok())
                .collect()
        });

        let mut request = MergeRequest::new(input.thread_id, input.source_branch);
        request.target_branch = input.target_branch;
        request.strategy = strategy;
        request.cherry_pick_ids = cherry_pick_ids;

        match self.engine.merge(request).await {
            Ok(response) => {
                let result = serde_json::json!({
                    "checkpoint_id": response.checkpoint_id.to_string(),
                    "target_branch": response.target_branch,
                    "merged_memory_count": response.merged_memory_count,
                    "status": "merged"
                });
                Ok(CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&result)
                        .unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}")),
                )]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(
        name = "mnemo.replay",
        description = "Reconstruct the agent context at a specific checkpoint. Returns the checkpoint state, referenced memories, and events up to that point."
    )]
    async fn replay(
        &self,
        Parameters(input): Parameters<ReplayInput>,
    ) -> Result<CallToolResult, McpError> {
        self.touch_activity();
        let checkpoint_id = input
            .checkpoint_id
            .and_then(|s| uuid::Uuid::parse_str(&s).ok());

        let mut request = ReplayRequest::new(input.thread_id);
        request.checkpoint_id = checkpoint_id;
        request.branch_name = input.branch_name;
        request.as_of = input.as_of;

        match self.engine.replay(request).await {
            Ok(response) => {
                let result = serde_json::json!({
                    "checkpoint": {
                        "id": response.checkpoint.id.to_string(),
                        "branch_name": response.checkpoint.branch_name,
                        "state_snapshot": response.checkpoint.state_snapshot,
                        "label": response.checkpoint.label,
                        "created_at": response.checkpoint.created_at,
                    },
                    "memory_count": response.memories.len(),
                    "event_count": response.events.len(),
                    "memories": response.memories.iter().map(|m| {
                        serde_json::json!({
                            "id": m.id.to_string(),
                            "content": m.content,
                            "memory_type": m.memory_type.to_string(),
                            "created_at": m.created_at,
                        })
                    }).collect::<Vec<_>>(),
                    "status": "replayed"
                });
                Ok(CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&result)
                        .unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}")),
                )]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(
        name = "mnemo.delegate",
        description = "Delegate permissions to another agent. Allows granting scoped, time-bounded access to your memories with optional re-delegation depth limits."
    )]
    async fn delegate(
        &self,
        Parameters(input): Parameters<DelegateInput>,
        Extension(caller): Extension<CallerContext>,
    ) -> Result<CallToolResult, McpError> {
        self.touch_activity();
        use mnemo_core::model::acl::Permission;
        use mnemo_core::model::delegation::{Delegation, DelegationScope};

        let permission = match input.permission.parse::<Permission>() {
            Ok(p) => p,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        };

        let scope = if let Some(ref ids) = input.memory_ids {
            let parsed: Result<Vec<uuid::Uuid>, _> =
                ids.iter().map(|s| uuid::Uuid::parse_str(s)).collect();
            match parsed {
                Ok(uuids) => DelegationScope::ByMemoryId(uuids),
                Err(e) => {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "invalid UUID: {e}"
                    ))]));
                }
            }
        } else if let Some(ref tags) = input.tags {
            DelegationScope::ByTag(tags.clone())
        } else {
            DelegationScope::AllMemories
        };

        let now = chrono::Utc::now();
        let expires_at = input
            .expires_in_hours
            .map(|h| (now + chrono::Duration::seconds((h * 3600.0) as i64)).to_rfc3339());

        let delegation = Delegation {
            id: uuid::Uuid::now_v7(),
            // The delegator is whoever is CALLING, resolved per request from the
            // capability it presented (ADR 0002). Left boot-derived, a
            // multi-caller transport would attribute every caller's grants to
            // the boot identity — a caller could hand out permissions recorded
            // as someone else's. Authority claims are the last place a stale
            // identity should survive.
            delegator_id: caller.caller_id.clone(),
            delegate_id: input.delegate_id.clone(),
            permission,
            scope,
            max_depth: input.max_depth.unwrap_or(0),
            current_depth: 0,
            parent_delegation_id: None,
            created_at: now.to_rfc3339(),
            expires_at,
            revoked_at: None,
        };

        match self.engine.storage.insert_delegation(&delegation).await {
            Ok(()) => {
                let result = serde_json::json!({
                    "delegation_id": delegation.id.to_string(),
                    "delegator": delegation.delegator_id,
                    "delegate": delegation.delegate_id,
                    "permission": delegation.permission.to_string(),
                    "status": "delegated"
                });
                Ok(CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&result)
                        .unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}")),
                )]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(
        name = "mnemo.verify",
        description = "Verify the hash chain integrity of stored memories. Detects tampered or corrupted records by validating content hashes and chain linkage."
    )]
    async fn verify(
        &self,
        Parameters(input): Parameters<VerifyInput>,
    ) -> Result<CallToolResult, McpError> {
        self.touch_activity();
        match self
            .engine
            .verify_integrity(input.agent_id, input.thread_id.as_deref())
            .await
        {
            Ok(result) => {
                let response = serde_json::json!({
                    "valid": result.valid,
                    "total_records": result.total_records,
                    "verified_records": result.verified_records,
                    "first_broken_at": result.first_broken_at.map(|id| id.to_string()),
                    "error_message": result.error_message,
                    "status": if result.valid { "verified" } else { "integrity_violation" }
                });
                Ok(CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&response)
                        .unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}")),
                )]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(
        name = "mnemo.trajectory_audit",
        description = "GEM-aligned trajectory-correctness audit (arXiv:2605.26252). Replays the hash-chained event log for an (agent_id, thread_id?) scope and reports four trajectory-level signals: (a) unregulated-growth (active-bank vs ceiling timeline), (b) missing-semantic-revision (facts superseded but never revised), (c) capacity-driven-forgetting (forget events outside the 5 named strategies), (d) read-only-retrieval (scopes that only RECALL). Complements mnemo.verify (per-record chain integrity) on the orthogonal trajectory axis."
    )]
    async fn trajectory_audit(
        &self,
        Parameters(input): Parameters<TrajectoryAuditInput>,
        Extension(caller): Extension<CallerContext>,
    ) -> Result<CallToolResult, McpError> {
        self.touch_activity();
        // Default to the REQUEST's caller (ADR 0002), not the boot identity —
        // an omitted agent_id must audit the caller's own trajectory.
        let agent_id = input.agent_id.clone().unwrap_or(caller.caller_id);
        // Mirror `verify_integrity`'s storage fetch shape: list_events
        // returns DESC order; the trajectory audit needs chronological
        // input, so we `.reverse()` before handing to compliance.
        let mut events = match self
            .engine
            .storage
            .list_events(&agent_id, mnemo_core::query::MAX_BATCH_QUERY_LIMIT, 0)
            .await
        {
            Ok(e) => e,
            Err(e) => return Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        };
        events.reverse();

        let mut req = mnemo_compliance::trajectory::TrajectoryAuditRequest {
            agent_id: Some(agent_id),
            thread_id: input.thread_id.clone(),
            ..Default::default()
        };
        if let Some(c) = input.active_bank_ceiling {
            req.active_bank_ceiling = c;
        }
        if let Some(k) = input.fact_key {
            req.fact_key = k;
        }
        if let Some(s) = input.named_forget_strategies {
            req.named_forget_strategies = s;
        }

        match mnemo_compliance::trajectory::trajectory_audit(&events, &req) {
            Ok(report) => {
                let payload = match serde_json::to_value(&report) {
                    Ok(v) => v,
                    Err(e) => {
                        return Ok(CallToolResult::error(vec![Content::text(e.to_string())]));
                    }
                };
                let response = serde_json::json!({
                    "report": payload,
                    "all_ok": report.all_ok(),
                });
                Ok(CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&response)
                        .unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}")),
                )]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(
        name = "mnemo.attention_state.put",
        description = "v0.4.5 — Store a precomputed attention-state blob under (agent_id, prefix_hash). Anchored on arXiv:2605.18226 (Context Memorization). The blob is opaque to mnemo; producer is responsible for format + model compatibility. Returns the assigned record id and its SHA-256 digest. Returns an error result if the server was not built with `MnemoServer::with_attention_state`."
    )]
    async fn attention_state_put(
        &self,
        Parameters(input): Parameters<AttentionStatePutInput>,
    ) -> Result<CallToolResult, McpError> {
        self.touch_activity();
        let store = match &self.attention_state {
            Some(s) => s.clone(),
            None => {
                return Ok(CallToolResult::error(vec![Content::text(
                    "attention_state store not attached; build the server with MnemoServer::with_attention_state".to_string(),
                )]));
            }
        };
        let state_blob = match hex::decode(input.state_blob_hex.as_str()) {
            Ok(bytes) => bytes,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "invalid state_blob_hex (expected hex): {e}"
                ))]));
            }
        };
        match store
            .put(
                input.agent_id,
                input.prefix_hash,
                state_blob,
                input.model,
                input.ttl_seconds,
            )
            .await
        {
            Ok(rec) => {
                let response = serde_json::json!({
                    "id": rec.id.to_string(),
                    "agent_id": rec.agent_id,
                    "prefix_hash": rec.prefix_hash,
                    "model": rec.model,
                    "blob_sha256_hex": rec.blob_sha256_hex,
                    "ttl_seconds": rec.ttl_seconds,
                    "created_at": rec.created_at,
                });
                Ok(CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&response)
                        .unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}")),
                )]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(
        name = "mnemo.attention_state.get",
        description = "v0.4.5 — Look up the most-recent attention-state record for (agent_id, prefix_hash). Returns the typed record (with the blob hex-encoded) or `null` on miss. Returns an error result if the server was not built with `MnemoServer::with_attention_state`. Anchored on arXiv:2605.18226."
    )]
    async fn attention_state_get(
        &self,
        Parameters(input): Parameters<AttentionStateGetInput>,
    ) -> Result<CallToolResult, McpError> {
        self.touch_activity();
        let store = match &self.attention_state {
            Some(s) => s.clone(),
            None => {
                return Ok(CallToolResult::error(vec![Content::text(
                    "attention_state store not attached; build the server with MnemoServer::with_attention_state".to_string(),
                )]));
            }
        };
        match store.get(&input.agent_id, &input.prefix_hash).await {
            Ok(Some(rec)) => {
                let response = serde_json::json!({
                    "id": rec.id.to_string(),
                    "agent_id": rec.agent_id,
                    "prefix_hash": rec.prefix_hash,
                    "model": rec.model,
                    "state_blob_hex": hex::encode(&rec.state_blob),
                    "blob_sha256_hex": rec.blob_sha256_hex,
                    "ttl_seconds": rec.ttl_seconds,
                    "created_at": rec.created_at,
                });
                Ok(CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&response)
                        .unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}")),
                )]))
            }
            Ok(None) => Ok(CallToolResult::success(vec![Content::text(
                "null".to_string(),
            )])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    // ----- Agent-controlled memory mode (AutoMEM, arXiv:2606.04315) -----
    //
    // Four tools the agent itself calls to manage a flat store it
    // curates: write what it judges worth keeping, read it back, revise
    // stale entries, and forget. Each is a thin composition over the
    // verified `remember` / `recall` / `forget` primitives plus the
    // reserved `agent-managed` tag — no new engine surface, and the
    // default `mnemo.recall` pipeline stays the single-shot fallback.

    #[tool(
        name = "mnemo.mem_write",
        description = "Agent-controlled memory (AutoMEM): persist an entry YOU decided is worth keeping into your flat, agent-managed store. Unlike automatic ingestion, nothing is written unless you call this. The entry is tagged 'agent-managed' and readable via mnemo.mem_read. Use mnemo.remember/mnemo.recall for the general pipeline."
    )]
    async fn mem_write(
        &self,
        Parameters(input): Parameters<MemWriteInput>,
    ) -> Result<CallToolResult, McpError> {
        self.touch_activity();
        let memory_type = match input.memory_type {
            Some(ref s) => match s.parse::<MemoryType>() {
                Ok(mt) => Some(mt),
                Err(_) => {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "invalid memory_type '{}': expected one of: episodic, semantic, procedural, working",
                        s
                    ))]));
                }
            },
            None => None,
        };
        let mut tags = input.tags.unwrap_or_default();
        if !tags.iter().any(|t| t == AGENT_MANAGED_TAG) {
            tags.push(AGENT_MANAGED_TAG.to_string());
        }
        let mut request = RememberRequest::new(input.content);
        request.memory_type = memory_type;
        request.tags = Some(tags);
        request.importance = input.importance;
        request.metadata = input.metadata;
        request.source_type = Some(SourceType::Agent);
        request.agent_id = input.agent_id;
        request.org_id = input.org_id;
        match self.engine.remember(request).await {
            Ok(response) => {
                let result = serde_json::json!({
                    "id": response.id.to_string(),
                    "content_hash": response.content_hash,
                    "store": AGENT_MANAGED_TAG,
                    "status": "written"
                });
                Ok(CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&result)
                        .unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}")),
                )]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(
        name = "mnemo.mem_read",
        description = "Agent-controlled memory (AutoMEM): read back YOUR agent-managed flat store. Searches only entries you wrote via mnemo.mem_write (filtered to the 'agent-managed' tag), not the whole backend. For broad single-shot retrieval across all memories, use mnemo.recall instead."
    )]
    async fn mem_read(
        &self,
        Parameters(input): Parameters<MemReadInput>,
    ) -> Result<CallToolResult, McpError> {
        self.touch_activity();
        let mut tags = input.tags.unwrap_or_default();
        if !tags.iter().any(|t| t == AGENT_MANAGED_TAG) {
            tags.push(AGENT_MANAGED_TAG.to_string());
        }
        let mut request = RecallRequest::new(input.query);
        request.limit = Some(input.limit.unwrap_or(10));
        request.tags = Some(tags);
        request.agent_id = input.agent_id;
        request.org_id = input.org_id;
        request.strategy = Some("auto".to_string());
        match self.engine.recall(request).await {
            Ok(response) => {
                let result = serde_json::json!({
                    "memories": response.memories,
                    "total": response.total,
                    "store": AGENT_MANAGED_TAG,
                });
                Ok(CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&result)
                        .unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}")),
                )]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(
        name = "mnemo.mem_revise",
        description = "Agent-controlled memory (AutoMEM): supersede a stale agent-managed entry with a corrected one. Soft-deletes the old id and writes the new content (tagged 'agent-managed', metadata.revises=<old_id>); the newest write wins on later reads. This is how YOU keep the flat store current instead of letting stale facts accumulate."
    )]
    async fn mem_revise(
        &self,
        Parameters(input): Parameters<MemReviseInput>,
    ) -> Result<CallToolResult, McpError> {
        self.touch_activity();
        let old_id = match uuid::Uuid::parse_str(&input.id) {
            Ok(id) => id,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "invalid UUID '{}': {e}",
                    input.id
                ))]));
            }
        };
        // Step 1: soft-forget the stale entry (recoverable + auditable).
        let mut forget_req = ForgetRequest::new(vec![old_id]);
        forget_req.strategy = Some(ForgetStrategy::SoftDelete);
        forget_req.agent_id = input.agent_id.clone();
        if let Err(e) = self.engine.forget(forget_req).await {
            return Ok(CallToolResult::error(vec![Content::text(format!(
                "failed to retire prior entry: {e}"
            ))]));
        }
        // Step 2: write the corrected entry, linked back to the old id.
        let mut tags = input.tags.unwrap_or_default();
        if !tags.iter().any(|t| t == AGENT_MANAGED_TAG) {
            tags.push(AGENT_MANAGED_TAG.to_string());
        }
        let mut request = RememberRequest::new(input.content);
        request.tags = Some(tags);
        request.importance = input.importance;
        request.source_type = Some(SourceType::Agent);
        request.agent_id = input.agent_id;
        request.org_id = input.org_id;
        request.metadata = Some(serde_json::json!({ "revises": old_id.to_string() }));
        match self.engine.remember(request).await {
            Ok(response) => {
                let result = serde_json::json!({
                    "id": response.id.to_string(),
                    "revises": old_id.to_string(),
                    "content_hash": response.content_hash,
                    "store": AGENT_MANAGED_TAG,
                    "status": "revised"
                });
                Ok(CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&result)
                        .unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}")),
                )]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(
        name = "mnemo.mem_forget",
        description = "Agent-controlled memory (AutoMEM): drop an agent-managed entry YOU no longer want. Soft-deletes by default (recoverable); pass hard=true for permanent removal."
    )]
    async fn mem_forget(
        &self,
        Parameters(input): Parameters<MemForgetInput>,
    ) -> Result<CallToolResult, McpError> {
        self.touch_activity();
        let id = match uuid::Uuid::parse_str(&input.id) {
            Ok(id) => id,
            Err(e) => {
                return Ok(CallToolResult::error(vec![Content::text(format!(
                    "invalid UUID '{}': {e}",
                    input.id
                ))]));
            }
        };
        let mut request = ForgetRequest::new(vec![id]);
        request.strategy = Some(if input.hard.unwrap_or(false) {
            ForgetStrategy::HardDelete
        } else {
            ForgetStrategy::SoftDelete
        });
        request.agent_id = input.agent_id;
        match self.engine.forget(request).await {
            Ok(response) => {
                let result = serde_json::json!({
                    "forgotten": response.forgotten.iter().map(|id| id.to_string()).collect::<Vec<_>>(),
                    "errors": response.errors,
                    "status": "forgotten"
                });
                Ok(CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&result)
                        .unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}")),
                )]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    // ----- Experience-memory tier (DocTrace, arXiv:2606.10921) -----
    //
    // Two ops that cache and replay successful retrieval/reasoning plans.
    // Both dispatch to the engine, which gates them on the
    // experience-memory mode (`with_experience_memory`); with the mode
    // off, `remember_plan` returns a disabled error and `recall_plan`
    // returns a miss, so default behaviour is unchanged.

    #[tool(
        name = "mnemo.remember_plan",
        description = "Experience memory (DocTrace): cache a SUCCESSFUL retrieval/reasoning plan — the query, the ordered steps, the chunk ids that produced a confirmed-good outcome, and an outcome score in [0,1]. Plans below the success threshold are not stored. Replay later with mnemo.recall_plan. Requires the server's experience-memory mode to be enabled."
    )]
    async fn remember_plan(
        &self,
        Parameters(input): Parameters<RememberPlanInput>,
    ) -> Result<CallToolResult, McpError> {
        self.touch_activity();
        let scope = match input.scope {
            Some(ref s) => match s.parse::<Scope>() {
                Ok(sc) => Some(sc),
                Err(_) => {
                    return Ok(CallToolResult::error(vec![Content::text(format!(
                        "invalid scope '{}': expected one of: private, shared, public, global",
                        s
                    ))]));
                }
            },
            None => None,
        };
        let request = RememberPlanRequest {
            query: input.query,
            steps: input.steps,
            chunk_ids: input.chunk_ids,
            outcome_score: input.outcome_score,
            agent_id: input.agent_id,
            scope,
            org_id: input.org_id,
        };
        match self.engine.remember_plan(request).await {
            Ok(response) => {
                let result = serde_json::json!({
                    "id": response.id.map(|id| id.to_string()),
                    "signature": response.signature,
                    "stored": response.stored,
                    "status": if response.stored { "plan_cached" } else { "below_success_threshold" }
                });
                Ok(CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&result)
                        .unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}")),
                )]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }

    #[tool(
        name = "mnemo.recall_plan",
        description = "Experience memory (DocTrace): replay the best cached plan whose query signature matches the new query above a similarity threshold (default 0.7), returning the stored chunk ids + step order instead of re-running full retrieval. Returns a miss (plan: null) when nothing matches or the mode is disabled. RBAC-gated: only plans visible to the requesting agent are considered."
    )]
    async fn recall_plan(
        &self,
        Parameters(input): Parameters<RecallPlanInput>,
    ) -> Result<CallToolResult, McpError> {
        self.touch_activity();
        let request = RecallPlanRequest {
            query: input.query,
            agent_id: input.agent_id,
            org_id: input.org_id,
            similarity_threshold: input.similarity_threshold,
        };
        match self.engine.recall_plan(request).await {
            Ok(response) => {
                let result = serde_json::json!({
                    "plan": response.plan,
                    "candidates_considered": response.candidates_considered,
                    "hit": response.plan.is_some(),
                });
                Ok(CallToolResult::success(vec![Content::text(
                    serde_json::to_string_pretty(&result)
                        .unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}")),
                )]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(e.to_string())])),
        }
    }
}

fn parse_source_type(s: &str) -> Option<SourceType> {
    match s {
        "agent" => Some(SourceType::Agent),
        "human" => Some(SourceType::Human),
        "system" => Some(SourceType::System),
        "user_input" => Some(SourceType::UserInput),
        "tool_output" => Some(SourceType::ToolOutput),
        "model_response" => Some(SourceType::ModelResponse),
        "retrieval" => Some(SourceType::Retrieval),
        "consolidation" => Some(SourceType::Consolidation),
        "import" => Some(SourceType::Import),
        _ => None,
    }
}

/// Cap on how many most-recent memories `list_resources` exposes.
const LIST_RESOURCES_LIMIT: usize = 50;

/// Prefix every memory URI in the resource layer carries.
pub const MEMORY_RESOURCE_SCHEME: &str = "mem://";

/// Compress a memory body into a ~60-char summary suitable for the
/// `name` field of an MCP resource listing.
fn summarize(content: &str) -> String {
    let cleaned: String = content
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let trimmed: String = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    if trimmed.len() <= 60 {
        return trimmed;
    }
    let mut out = String::new();
    for word in trimmed.split_whitespace() {
        if out.len() + word.len() + 1 > 57 {
            break;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    }
    out.push_str("...");
    out
}

#[tool_handler]
impl ServerHandler for MnemoServer {
    /// v0.5.19 — role-filtered `tools/list`. Because we define `list_tools`
    /// here, the `#[tool_handler]` macro skips generating its own (it only
    /// generates methods absent from this impl). When a [`RoleFilter`] is
    /// attached, a caller sees only the tools its roles permit; otherwise this
    /// is identical to the macro's default (`tool_router.list_all()`).
    async fn list_tools(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParams>,
        context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<rmcp::model::ListToolsResult, McpError> {
        // Per-request identity (ADR 0002): the catalog a caller sees is decided
        // by the capability it presented, not by a boot-time constant. A
        // rejected capability fails the listing rather than falling back —
        // otherwise a forged token would be shown the operator's full catalog.
        let caller = self.resolve_caller(&context.meta, &context.extensions)?;
        let visible: std::collections::HashSet<String> =
            self.visible_tool_names_for(&caller).into_iter().collect();
        let tools = self
            .tool_router
            .list_all()
            .into_iter()
            .filter(|t| visible.contains(t.name.as_ref()))
            .collect();
        // SEP-2549 caching hints.
        //
        // `cache_scope` MUST be `Private` here and it is not a judgement call:
        // this catalog is role-filtered per caller (ADR 0002), so two callers
        // presenting different capabilities see different tool sets. `Public`
        // would permit a shared intermediary to serve one caller's filtered
        // catalog to another, which is a capability leak. `CacheScope::default()`
        // is `Public`, so leaving the field unset is not the safe option it looks
        // like once anything downstream starts reading it.
        //
        // These fields are defined by the 2026-07-28 revision and mnemo
        // negotiates 2025-11-25, so no client is currently promised them. They
        // are emitted anyway because an unknown JSON field is ignored by every
        // client that does not know it, while an intermediary that DOES
        // understand `cacheScope` gets told the truth today rather than after
        // the eventual revision bump.
        Ok(rmcp::model::ListToolsResult {
            tools,
            ttl_ms: Some(Self::TOOLS_LIST_TTL_MS),
            cache_scope: Some(rmcp::model::CacheScope::Private),
            ..Default::default()
        })
    }

    /// v0.5.19 — role-filtered `tools/call`. A caller cannot invoke by name a
    /// tool it was not shown: a denied tool returns a spec-compliant `-32601`
    /// (method not found) with the deny reason echoed in `data` — never a silent
    /// empty result. Non-denied calls delegate to the same `tool_router.call`
    /// the macro-generated dispatch uses.
    async fn call_tool(
        &self,
        request: rmcp::model::CallToolRequestParams,
        context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<rmcp::model::CallToolResponse, McpError> {
        // rmcp 3.0: `call_tool` returns the `CallToolResponse` enum
        // (Complete / InputRequired / Task). The tool router already yields that
        // enum, so we pass its result through unchanged — every mnemo tool call is
        // synchronous, so it resolves to `CallToolResponse::Complete`.
        let name = request.name.to_string();
        // Per-request identity (ADR 0002). Resolved BEFORE the role gate so the
        // gate runs against the capability's roles, and before dispatch so a
        // rejected capability never reaches a tool body.
        let caller = self.resolve_caller(&context.meta, &context.extensions)?;
        if let Some(reason) = self.tool_call_denial_for(&caller, &name) {
            return Err(McpError::new(
                rmcp::model::ErrorCode::METHOD_NOT_FOUND,
                format!("tool '{name}' not found"),
                Some(serde_json::json!({
                    "tool": name,
                    "role_filter_reason": reason,
                })),
            ));
        }
        // Hand the resolved identity to the tool bodies. Tools that record
        // authority (delegate) or scope reads (trajectory_audit) take
        // `Extension<CallerContext>` and get THIS caller, not the boot one.
        let mut context = context;
        context.extensions.insert(caller);
        let tcc = rmcp::handler::server::tool::ToolCallContext::new(self, request, context);
        self.tool_router.call(tcc).await
    }

    async fn list_resources(
        &self,
        _request: Option<rmcp::model::PaginatedRequestParams>,
        context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<rmcp::model::ListResourcesResult, McpError> {
        use mnemo_core::storage::MemoryFilter;
        self.touch_activity();
        // Scope by the CALLER's per-request identity, not a boot-time constant.
        // This is the read-leak ADR 0002 names: boot-derived here, a
        // multi-caller transport would serve one agent's memories to every
        // authenticated caller while the tool filter correctly gated per caller.
        let caller = self.resolve_caller(&context.meta, &context.extensions)?;
        let filter = MemoryFilter {
            agent_id: Some(caller.caller_id),
            include_deleted: false,
            ..Default::default()
        };
        let records = self
            .engine
            .storage
            .list_memories(&filter, LIST_RESOURCES_LIMIT, 0)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        let resources = records
            .into_iter()
            .map(|r| {
                // rmcp 2.x flattened `RawResource` + `Annotated` into a single
                // `Resource` (no `.no_annotation()`; `size` is now `u64`).
                let mut res = rmcp::model::Resource::new(
                    format!("{MEMORY_RESOURCE_SCHEME}{}", r.id),
                    summarize(&r.content),
                );
                res.description = Some(format!("agent={} type={}", r.agent_id, r.memory_type));
                res.mime_type = Some("text/markdown".into());
                res.size = Some(r.content.len() as u64);
                res
            })
            .collect();
        // SEP-2549, as above. `Private` is load-bearing here for a stronger
        // reason than on `tools/list`: these resources are one agent's memory
        // records, scoped by the caller's per-request identity, so a shared
        // cache serving them to another caller would leak content and not just
        // a catalog.
        Ok(rmcp::model::ListResourcesResult {
            resources,
            ttl_ms: Some(Self::RESOURCES_LIST_TTL_MS),
            cache_scope: Some(rmcp::model::CacheScope::Private),
            ..Default::default()
        })
    }

    async fn read_resource(
        &self,
        request: rmcp::model::ReadResourceRequestParams,
        _context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> Result<rmcp::model::ReadResourceResponse, McpError> {
        self.touch_activity();
        let uri = request.uri.clone();
        let Some(id_str) = uri.strip_prefix(MEMORY_RESOURCE_SCHEME) else {
            return Err(McpError::invalid_params(
                format!("unknown resource scheme: {uri}"),
                None,
            ));
        };
        let id = uuid::Uuid::parse_str(id_str)
            .map_err(|e| McpError::invalid_params(format!("bad uuid: {e}"), None))?;
        let record = self
            .engine
            .storage
            .get_memory(id)
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?
            .ok_or_else(|| McpError::resource_not_found(format!("memory {id} not found"), None))?;
        let contents = rmcp::model::ResourceContents::TextResourceContents {
            uri,
            mime_type: Some("text/markdown".into()),
            text: record.content,
            meta: None,
        };
        // rmcp 3.0: `read_resource` returns the `ReadResourceResponse` enum. The
        // read is synchronous and always completes, so wrap in `Complete`.
        Ok(rmcp::model::ReadResourceResponse::Complete(
            rmcp::model::ReadResourceResult::new(vec![contents]),
        ))
    }

    /// The protocol revisions mnemo actually implements.
    ///
    /// Narrowed deliberately. rmcp's default is
    /// [`ProtocolVersion::KNOWN_VERSIONS`], which is every revision *the SDK*
    /// knows - as of rmcp 3.1.3 that includes `2026-07-28`. rmcp derives
    /// `server/discover` from this list, so taking the default made mnemo
    /// advertise a revision it does not serve: it answers `initialize` with
    /// `2025-11-25` (rmcp's `ProtocolVersion::LATEST`) while telling a
    /// discovering client it also speaks `2026-07-28`.
    ///
    /// That is a machine-readable claim that was not true, and it is the same
    /// claimed-but-not-wired shape this repo has repaired before (`role_filter`
    /// #124, tool-catalog attestation v0.5.20, `LeaseStore` → ADR 0001). A
    /// client entitled to believe it could send `2026-07-28` would get a server
    /// that still expects the `initialize` handshake that revision removes, and
    /// list results with neither `ttlMs` nor `cacheScope`.
    ///
    /// Narrowing is rmcp's own supported mechanism, not a workaround - its
    /// `negotiate_protocol_version` documents that "a server that narrows that
    /// list is never made to answer `initialize` with a version it cannot
    /// serve", and a client asking for an unlisted revision negotiates down to
    /// the server fallback rather than failing.
    ///
    /// `2026-07-28` goes back in when mnemo implements it, not when rmcp does.
    /// See `docs/src/integrations/mcp-2026-07-28.md` for the row-by-row state.
    fn supported_protocol_versions(
        &self,
    ) -> std::borrow::Cow<'static, [rmcp::model::ProtocolVersion]> {
        std::borrow::Cow::Borrowed(&[
            rmcp::model::ProtocolVersion::V_2024_11_05,
            rmcp::model::ProtocolVersion::V_2025_03_26,
            rmcp::model::ProtocolVersion::V_2025_06_18,
            rmcp::model::ProtocolVersion::V_2025_11_25,
        ])
    }

    fn get_info(&self) -> ServerInfo {
        let mut info = ServerInfo::default();
        info.instructions = Some(
            "Mnemo is an MCP-native memory database for AI agents. \
             Use mnemo.remember to store memories, mnemo.recall to search them, \
             mnemo.forget to delete them, mnemo.share to share with other agents, \
             mnemo.checkpoint to snapshot state, mnemo.branch to fork for exploration, \
             mnemo.merge to combine branches, mnemo.replay to reconstruct context, \
             mnemo.verify to check hash chain integrity, \
             and mnemo.delegate to grant scoped permissions to other agents."
                .into(),
        );
        info.capabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_resources()
            .build();
        // Implementation::from_build_env() picks up rmcp's own PKG name —
        // set the fields explicitly so the server advertises as "mnemo".
        let mut impl_block = Implementation::from_build_env();
        impl_block.name = "mnemo".into();
        impl_block.version = env!("CARGO_PKG_VERSION").into();
        info.server_info = impl_block;
        info
    }
}
