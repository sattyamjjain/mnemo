use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::hash::compute_content_hash;
use crate::model::capability::Capability;
use crate::model::event::{AgentEvent, EventType};
use crate::model::memory::{ConsolidationState, MemoryRecord, MemoryType, Scope, SourceType};
use crate::model::relation::Relation;
use crate::model::write_provenance::{WriteFlag, WriteOp};
use crate::opaque_reasoning;
use crate::query::MnemoEngine;
#[allow(unused_imports)]
use base64::Engine as _;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RememberRequest {
    pub content: String,
    pub agent_id: Option<String>,
    pub memory_type: Option<MemoryType>,
    pub scope: Option<Scope>,
    pub importance: Option<f32>,
    pub tags: Option<Vec<String>>,
    pub metadata: Option<serde_json::Value>,
    pub source_type: Option<SourceType>,
    pub source_id: Option<String>,
    pub org_id: Option<String>,
    pub thread_id: Option<String>,
    pub ttl_seconds: Option<u64>,
    pub related_to: Option<Vec<String>>,
    pub decay_rate: Option<f32>,
    pub created_by: Option<String>,
}

impl RememberRequest {
    pub fn new(content: String) -> Self {
        Self {
            content,
            agent_id: None,
            memory_type: None,
            scope: None,
            importance: None,
            tags: None,
            metadata: None,
            source_type: None,
            source_id: None,
            org_id: None,
            thread_id: None,
            ttl_seconds: None,
            related_to: None,
            decay_rate: None,
            created_by: None,
        }
    }
}

#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RememberResponse {
    pub id: Uuid,
    pub content_hash: String,
}

impl RememberResponse {
    pub fn new(id: Uuid, content_hash: String) -> Self {
        Self { id, content_hash }
    }
}

pub async fn execute(engine: &MnemoEngine, request: RememberRequest) -> Result<RememberResponse> {
    remember_inner(engine, request, None).await
}

/// REMEMBER authorised by a verifiable [`Capability`]. The capability is verified
/// against the engine's issuer before the write, and its id is recorded in the
/// write provenance (the principal comes from the capability, not `created_by`).
pub async fn execute_with_capability(
    engine: &MnemoEngine,
    request: RememberRequest,
    capability: &Capability,
) -> Result<RememberResponse> {
    engine.verify_capability(capability)?;
    remember_inner(engine, request, Some(capability)).await
}

async fn remember_inner(
    engine: &MnemoEngine,
    request: RememberRequest,
    capability: Option<&Capability>,
) -> Result<RememberResponse> {
    // Validate
    if request.content.trim().is_empty() {
        return Err(Error::Validation("content cannot be empty".to_string()));
    }

    let resolved_tier = request.memory_type.unwrap_or(MemoryType::Episodic);

    // Tier-specific importance enforcement:
    // Procedural memories (system prompts, tool definitions) carry an
    // importance floor so they never decay below the recall threshold.
    let mut importance = request.importance.unwrap_or(0.5);
    if resolved_tier == MemoryType::Procedural && importance < engine.procedural_importance_floor {
        importance = engine.procedural_importance_floor;
    }
    if !(0.0..=1.0).contains(&importance) {
        return Err(Error::Validation(
            "importance must be between 0.0 and 1.0".to_string(),
        ));
    }

    let agent_id = request
        .agent_id
        .unwrap_or_else(|| engine.default_agent_id.clone());
    super::validate_agent_id(&agent_id)?;
    let org_id = request.org_id.or_else(|| engine.default_org_id.clone());
    let now = chrono::Utc::now();
    let now_str = now.to_rfc3339();
    let id = Uuid::now_v7();

    // Compute embedding
    let embedding = engine.embedding.embed(&request.content).await?;

    // Compute content hash
    let content_hash = compute_content_hash(&request.content, &agent_id, &now_str);

    // Chain linking happens at insertion time, inside the storage backend, so
    // that reading the head and writing the record that names it cannot be
    // interleaved by another writer. Reading the head here — as this did until
    // v0.5.29 — left ~80 lines of work between the read and the insert, and
    // every call that overlapped that window wrote itself as a fresh head. See
    // `StorageBackend::append_memory_chained`.
    let prev_hash = None;

    // Compute expires_at from ttl_seconds. Working-tier memories get an
    // automatic TTL so they can't outlive their session — caller-supplied
    // ttl_seconds still wins.
    let effective_ttl = request.ttl_seconds.or_else(|| {
        if resolved_tier == MemoryType::Working {
            Some(engine.ttl_working_seconds)
        } else {
            None
        }
    });
    let expires_at =
        effective_ttl.map(|ttl| (now + chrono::Duration::seconds(ttl as i64)).to_rfc3339());

    let mut record = MemoryRecord {
        id,
        agent_id: agent_id.clone(),
        content: request.content,
        memory_type: resolved_tier,
        scope: request.scope.unwrap_or(Scope::Private),
        importance,
        tags: request.tags.unwrap_or_default(),
        metadata: request
            .metadata
            .unwrap_or(serde_json::Value::Object(serde_json::Map::new())),
        embedding: Some(embedding.clone()),
        content_hash: content_hash.clone(),
        prev_hash,
        source_type: request.source_type.unwrap_or(SourceType::Agent),
        source_id: request.source_id,
        consolidation_state: ConsolidationState::Raw,
        access_count: 0,
        org_id,
        thread_id: request.thread_id,
        created_at: now_str.clone(),
        updated_at: now_str,
        last_accessed_at: None,
        expires_at,
        deleted_at: None,
        decay_rate: request.decay_rate,
        created_by: request.created_by,
        version: 1,
        prev_version_id: None,
        quarantined: false,
        quarantine_reason: None,
        decay_function: None,
    };

    // Opaque-reasoning-payload SHAPE check (arXiv:2608.09867). Run on the
    // PLAINTEXT content, BEFORE any encryption below — at-rest encryption
    // base64-encodes the content and would itself look like an opaque blob. We
    // flag the shape and record it on provenance; we do NOT reject the write
    // (warn-and-record) and we NEVER decode the content. A flag is not proof of a
    // secret — see crate::opaque_reasoning.
    let mut write_flags: Vec<WriteFlag> = Vec::new();
    if let Some(reason) = opaque_reasoning::detect(&record.content) {
        tracing::warn!(
            memory_id = %record.id,
            reason = reason,
            "remembered content has the shape of a provider opaque reasoning payload \
             (arXiv:2608.09867); recording an opaque_reasoning_payload flag on its \
             provenance. Shape only — this is NOT proof a secret is present. Revoke via \
             forget_by_principal / forget_by_session if needed."
        );
        write_flags.push(WriteFlag::OpaqueReasoningPayload);
    }

    // Encrypt content if encryption is configured (after embedding, before storage)
    if let Some(ref enc) = engine.encryption {
        let encrypted = enc.encrypt(record.content.as_bytes())?;
        record.content =
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &encrypted);
    }

    // Store in database. `append_memory_chained` assigns `prev_hash` under the
    // backend's own mutual exclusion and hands back what it wrote, so the local
    // copy — which is what the cache and the response carry — agrees with what
    // is durable.
    record.prev_hash = Some(engine.storage.append_memory_chained(&record).await?);

    // Write provenance: who wrote this, under what authority. The principal is
    // the capability holder if the write was capability-authorised, else the
    // record's `created_by`, else its `agent_id`. Session/trace is the
    // `thread_id`. Chained + tamper-evident (see model::write_provenance).
    let principal = capability
        .map(|c| c.principal.clone())
        .or_else(|| record.created_by.clone())
        .unwrap_or_else(|| record.agent_id.clone());
    engine
        .record_write_provenance(
            record.id,
            principal,
            capability.map(|c| c.id),
            record.thread_id.clone(),
            WriteOp::Remember,
            write_flags,
        )
        .await?;

    // Add to vector index
    engine.index.add(id, &embedding)?;

    // Add to full-text index if available
    if let Some(ref ft) = engine.full_text {
        ft.add(id, &record.content)?;
        ft.commit()?;
    }

    // Check for anomaly and update agent profile
    let anomaly_result = super::poisoning::check_for_anomaly(engine, &record).await?;
    if anomaly_result.is_anomalous {
        super::poisoning::quarantine_memory(engine, id, &anomaly_result.reasons.join("; ")).await?;
        tracing::warn!(
            memory_id = %id,
            score = anomaly_result.score,
            reasons = ?anomaly_result.reasons,
            "Memory quarantined due to anomaly detection"
        );
    }
    super::poisoning::update_agent_profile(engine, &record).await?;

    // Create relations if specified
    if let Some(ref related_ids) = request.related_to {
        for target_str in related_ids {
            if let Ok(target_id) = Uuid::parse_str(target_str) {
                let relation = Relation {
                    id: Uuid::now_v7(),
                    source_id: id,
                    target_id,
                    relation_type: "related_to".to_string(),
                    weight: 1.0,
                    metadata: serde_json::Value::Object(serde_json::Map::new()),
                    created_at: record.created_at.clone(),
                };
                if let Err(e) = engine.storage.insert_relation(&relation).await {
                    tracing::error!(relation_id = %relation.id, error = %e, "failed to insert relation");
                }
            }
        }
    }

    // Emit MemoryWrite event (fire-and-forget). `prev_hash` is left unset: the
    // append assigns it, for the same reason as the memory chain above.
    let mut event = AgentEvent {
        id: Uuid::now_v7(),
        agent_id: record.agent_id.clone(),
        thread_id: record.thread_id.clone(),
        run_id: None,
        parent_event_id: None,
        event_type: EventType::MemoryWrite,
        payload: serde_json::json!({"memory_id": id.to_string()}),
        trace_id: None,
        span_id: None,
        model: None,
        tokens_input: None,
        tokens_output: None,
        latency_ms: None,
        cost_usd: None,
        timestamp: record.created_at.clone(),
        logical_clock: 0,
        content_hash: content_hash.clone(),
        prev_hash: None,
        embedding: None,
    };
    // Optionally embed the event payload
    if engine.embed_events
        && let Ok(emb) = engine.embedding.embed(&event.payload.to_string()).await
    {
        event.embedding = Some(emb);
    }
    if let Err(e) = engine.storage.append_event_chained(&event).await {
        tracing::error!(event_id = %event.id, error = %e, "failed to insert audit event");
    }

    // Put in cache if configured
    if let Some(ref cache) = engine.cache {
        cache.put(record);
    }

    let hash_hex = hex::encode(&content_hash);

    Ok(RememberResponse {
        id,
        content_hash: hash_hex,
    })
}
