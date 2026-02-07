use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::hash::{compute_chain_hash, compute_content_hash};
#[allow(unused_imports)]
use base64::Engine as _;
use crate::model::event::{AgentEvent, EventType};
use crate::model::memory::{
    ConsolidationState, MemoryRecord, MemoryType, Scope, SourceType,
};
use crate::model::relation::Relation;
use crate::query::MnemoEngine;

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RememberResponse {
    pub id: Uuid,
    pub content_hash: String,
}

pub async fn execute(engine: &MnemoEngine, request: RememberRequest) -> Result<RememberResponse> {
    // Validate
    if request.content.trim().is_empty() {
        return Err(Error::Validation("content cannot be empty".to_string()));
    }

    let importance = request.importance.unwrap_or(0.5);
    if !(0.0..=1.0).contains(&importance) {
        return Err(Error::Validation(
            "importance must be between 0.0 and 1.0".to_string(),
        ));
    }

    let agent_id = request.agent_id.unwrap_or_else(|| engine.default_agent_id.clone());
    let org_id = request.org_id.or_else(|| engine.default_org_id.clone());
    let now = chrono::Utc::now();
    let now_str = now.to_rfc3339();
    let id = Uuid::now_v7();

    // Compute embedding
    let embedding = engine.embedding.embed(&request.content).await?;

    // Compute content hash
    let content_hash = compute_content_hash(&request.content, &agent_id, &now_str);

    // Chain linking: look up prev_hash
    let prev_hash_raw = engine
        .storage
        .get_latest_memory_hash(&agent_id, request.thread_id.as_deref())
        .await?;
    let prev_hash = Some(compute_chain_hash(&content_hash, prev_hash_raw.as_deref()));

    // Compute expires_at from ttl_seconds
    let expires_at = request.ttl_seconds.map(|ttl| {
        (now + chrono::Duration::seconds(ttl as i64)).to_rfc3339()
    });

    let mut record = MemoryRecord {
        id,
        agent_id: agent_id.clone(),
        content: request.content,
        memory_type: request.memory_type.unwrap_or(MemoryType::Episodic),
        scope: request.scope.unwrap_or(Scope::Private),
        importance,
        tags: request.tags.unwrap_or_default(),
        metadata: request.metadata.unwrap_or(serde_json::Value::Object(serde_json::Map::new())),
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

    // Encrypt content if encryption is configured (after embedding, before storage)
    if let Some(ref enc) = engine.encryption {
        let encrypted = enc.encrypt(record.content.as_bytes())?;
        record.content = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &encrypted);
    }

    // Store in database
    engine.storage.insert_memory(&record).await?;

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
                let _ = engine.storage.insert_relation(&relation).await;
            }
        }
    }

    // Emit MemoryWrite event with hash chain linking (fire-and-forget)
    let prev_event_hash = engine.storage.get_latest_event_hash(&agent_id, record.thread_id.as_deref()).await.unwrap_or(None);
    let event_prev_hash = Some(compute_chain_hash(&content_hash, prev_event_hash.as_deref()));
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
        prev_hash: event_prev_hash,
        embedding: None,
    };
    // Optionally embed the event payload
    if engine.embed_events {
        if let Ok(emb) = engine.embedding.embed(&event.payload.to_string()).await {
            event.embedding = Some(emb);
        }
    }
    let _ = engine.storage.insert_event(&event).await;

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
