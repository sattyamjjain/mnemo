//! Topic-document consolidation primitive (v0.5.0).
//!
//! Anchored on **Infini-Memory** (arXiv:2606.10677) — "each topic document
//! serves as a semantic unit for collecting related evidence, preserving
//! metadata, and revising facts over time."
//!
//! [`execute`] groups a *caller-chosen* set of member memories into a single
//! revisable **topic document**: it collects evidence (the member ids), it
//! preserves provenance (per-member source, timestamp, confidence), it supports
//! fact revision (supersede an earlier topic document while keeping the old row
//! and the hash-chain history), and the result is retrievable as a unit (a
//! normal recallable [`MemoryRecord`] plus `consolidated_from` relations).
//!
//! This is the caller-driven, by-id, revisable sibling of the offline
//! tag-cluster pass in [`super::lifecycle::run_consolidation`]. It is
//! deterministic (no LLM): with no caller `summary`, the document content is a
//! stable join of the member contents ordered by `(created_at, id)`, so the
//! same inputs always yield the same document. Being a plain engine primitive,
//! it flows identically through MCP, REST, and gRPC.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::hash::{compute_chain_hash, compute_content_hash};
use crate::model::acl::Permission;
use crate::model::event::EventType;
use crate::model::memory::{ConsolidationState, MemoryRecord, MemoryType, Scope, SourceType};
use crate::model::relation::Relation;
use crate::query::MnemoEngine;
#[allow(unused_imports)]
use base64::Engine as _;

/// Request to consolidate a set of member memories into one topic document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsolidateRequest {
    /// The member memories to collect as evidence. Must be non-empty.
    /// Duplicates are ignored. Every id must exist, be readable by the
    /// resolved agent, and not be soft-deleted.
    pub memory_ids: Vec<Uuid>,
    /// The topic document's name / fact key. Stored in `metadata.topic`,
    /// used as the document tag, and (absent `summary`) as the heading.
    pub topic_name: String,
    /// Agent that owns the new topic document. Defaults to
    /// `engine.default_agent_id`.
    pub agent_id: Option<String>,
    /// Optional caller-supplied document body. When omitted (or blank) the
    /// body is synthesised deterministically from the member contents.
    pub summary: Option<String>,
    /// Optional id of an existing topic document this one revises. When set,
    /// the new document becomes `version = old.version + 1` with
    /// `prev_version_id = old.id`; the old document is retained (marked
    /// `Consolidated` with a `superseded_by` pointer, NOT deleted, so the hash
    /// chain stays whole), and a [`EventType::MemoryRevised`] audit event is
    /// emitted alongside the consolidation event.
    pub supersede: Option<Uuid>,
    /// Optional thread/session scope for the topic document and its events.
    pub thread_id: Option<String>,
    /// Optional caller metadata, merged under the provenance keys this
    /// primitive writes (`topic`, `consolidated_from`, `members`, `revision_of`).
    pub metadata: Option<serde_json::Value>,
}

impl ConsolidateRequest {
    /// Construct a request with only the required fields set.
    pub fn new(memory_ids: Vec<Uuid>, topic_name: String) -> Self {
        Self {
            memory_ids,
            topic_name,
            agent_id: None,
            summary: None,
            supersede: None,
            thread_id: None,
            metadata: None,
        }
    }
}

/// Result of a consolidation.
#[non_exhaustive]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsolidateResponse {
    /// Id of the newly created topic document.
    pub topic_document_id: Uuid,
    /// Echoes the request topic name.
    pub topic_name: String,
    /// Number of distinct member memories collected.
    pub source_count: usize,
    /// Version of the topic document (1, or `old.version + 1` on revision).
    pub version: u32,
    /// Id of the topic document this one superseded, if any.
    pub superseded_id: Option<Uuid>,
    /// The distinct member ids, in the deterministic order used for synthesis.
    pub member_ids: Vec<Uuid>,
    /// Hex-encoded SHA-256 content hash of the (plaintext) document body.
    pub content_hash: String,
    /// Id of the emitted [`EventType::MemoryConsolidated`] audit event.
    pub consolidation_event_id: Uuid,
    /// Id of the emitted [`EventType::MemoryRevised`] audit event (revision only).
    pub revision_event_id: Option<Uuid>,
}

/// Decrypt a record's content in place if engine-level encryption is configured.
/// Mirrors the read-path decryption used by `recall`.
fn decrypt_in_place(engine: &MnemoEngine, record: &mut MemoryRecord) {
    if let Some(ref enc) = engine.encryption {
        match base64::engine::general_purpose::STANDARD.decode(&record.content) {
            Ok(bytes) => match enc.decrypt(&bytes) {
                Ok(plain) => match String::from_utf8(plain) {
                    Ok(text) => record.content = text,
                    Err(e) => {
                        tracing::error!(memory_id = %record.id, error = %e, "decrypted content is not valid UTF-8");
                        record.content = "[content unavailable: decryption error]".to_string();
                    }
                },
                Err(e) => {
                    tracing::error!(memory_id = %record.id, error = %e, "failed to decrypt member content");
                    record.content = "[content unavailable: decryption error]".to_string();
                }
            },
            Err(e) => {
                tracing::error!(memory_id = %record.id, error = %e, "failed to decode encrypted member content");
                record.content = "[content unavailable: decryption error]".to_string();
            }
        }
    }
}

pub async fn execute(
    engine: &MnemoEngine,
    request: ConsolidateRequest,
) -> Result<ConsolidateResponse> {
    // --- Validate ---------------------------------------------------------
    if request.memory_ids.is_empty() {
        return Err(Error::Validation("memory_ids cannot be empty".to_string()));
    }
    let topic = request.topic_name.trim().to_string();
    if topic.is_empty() {
        return Err(Error::Validation("topic_name cannot be empty".to_string()));
    }
    let agent_id = request
        .agent_id
        .clone()
        .unwrap_or_else(|| engine.default_agent_id.clone());
    super::validate_agent_id(&agent_id)?;

    // --- Collect evidence: fetch, permission-gate, decrypt ----------------
    // Only `accessible ∩ requested` proceeds; a missing/denied/deleted member
    // aborts the whole operation so nothing partial is written.
    let mut members: Vec<MemoryRecord> = Vec::with_capacity(request.memory_ids.len());
    let mut seen: std::collections::HashSet<Uuid> = std::collections::HashSet::new();
    for id in &request.memory_ids {
        if !seen.insert(*id) {
            continue;
        }
        let mut record = engine
            .storage
            .get_memory(*id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("memory {id} not found")))?;
        if record.is_deleted() {
            return Err(Error::Validation(format!(
                "memory {id} is deleted and cannot be consolidated"
            )));
        }
        if !engine
            .storage
            .check_permission(*id, &agent_id, Permission::Read)
            .await?
        {
            return Err(Error::PermissionDenied(format!(
                "agent {agent_id} cannot read memory {id}"
            )));
        }
        decrypt_in_place(engine, &mut record);
        members.push(record);
    }
    // Deterministic ordering: oldest first, ties broken by id.
    members.sort_by(|a, b| a.created_at.cmp(&b.created_at).then(a.id.cmp(&b.id)));

    // --- Revision bookkeeping --------------------------------------------
    let (version, prev_version_id, superseded_id) = match request.supersede {
        Some(old_id) => {
            let old = engine.storage.get_memory(old_id).await?.ok_or_else(|| {
                Error::NotFound(format!("topic document {old_id} to supersede not found"))
            })?;
            if old.agent_id != agent_id {
                return Err(Error::PermissionDenied(format!(
                    "agent {agent_id} cannot supersede topic document {old_id}"
                )));
            }
            (old.version.saturating_add(1), Some(old_id), Some(old_id))
        }
        None => (1, None, None),
    };

    // --- Synthesise the topic-document body (deterministic) ---------------
    let content_plain = match request.summary.as_ref() {
        Some(s) if !s.trim().is_empty() => s.clone(),
        _ => {
            let mut body = format!("# {topic}\n\n");
            for (i, m) in members.iter().enumerate() {
                if i > 0 {
                    body.push_str("\n\n");
                }
                body.push_str(&m.content);
            }
            body
        }
    };

    // --- Preserve metadata / provenance ----------------------------------
    let consolidated_from: Vec<String> = members.iter().map(|m| m.id.to_string()).collect();
    let member_meta: Vec<serde_json::Value> = members
        .iter()
        .map(|m| {
            serde_json::json!({
                "id": m.id.to_string(),
                "source_type": m.source_type.to_string(),
                "source_id": m.source_id,
                "created_at": m.created_at,
                "importance": m.importance,
            })
        })
        .collect();
    let mut meta_map = match request.metadata.clone() {
        Some(serde_json::Value::Object(m)) => m,
        _ => serde_json::Map::new(),
    };
    meta_map.insert("topic".to_string(), serde_json::json!(topic));
    meta_map.insert(
        "consolidated_from".to_string(),
        serde_json::json!(consolidated_from),
    );
    meta_map.insert("members".to_string(), serde_json::json!(member_meta));
    if let Some(sid) = superseded_id {
        meta_map.insert(
            "revision_of".to_string(),
            serde_json::json!(sid.to_string()),
        );
    }
    let metadata = serde_json::Value::Object(meta_map);

    // --- Build the topic-document record ----------------------------------
    let now = chrono::Utc::now().to_rfc3339();
    let id = Uuid::now_v7();
    let content_hash = compute_content_hash(&content_plain, &agent_id, &now);
    let prev_hash_raw = engine
        .storage
        .get_latest_memory_hash(&agent_id, request.thread_id.as_deref())
        .await?;
    let prev_hash = Some(compute_chain_hash(&content_hash, prev_hash_raw.as_deref()));
    let importance = members.iter().map(|m| m.importance).fold(0.0_f32, f32::max);
    let scope = members.first().map(|m| m.scope).unwrap_or(Scope::Private);
    let org_id = members
        .iter()
        .find_map(|m| m.org_id.clone())
        .or_else(|| engine.default_org_id.clone());
    let embedding = engine.embedding.embed(&content_plain).await?;

    let mut record = MemoryRecord {
        id,
        agent_id: agent_id.clone(),
        content: content_plain.clone(),
        memory_type: MemoryType::Semantic,
        scope,
        importance,
        tags: vec![topic.clone()],
        metadata,
        embedding: Some(embedding.clone()),
        content_hash: content_hash.clone(),
        prev_hash,
        source_type: SourceType::Consolidation,
        source_id: None,
        consolidation_state: ConsolidationState::Active,
        access_count: 0,
        org_id,
        thread_id: request.thread_id.clone(),
        created_at: now.clone(),
        updated_at: now.clone(),
        last_accessed_at: None,
        expires_at: None,
        deleted_at: None,
        decay_rate: None,
        created_by: Some("consolidate".to_string()),
        version,
        prev_version_id,
        quarantined: false,
        quarantine_reason: None,
        decay_function: None,
    };

    // Encrypt at rest after hashing/embedding, exactly like `remember`.
    if let Some(ref enc) = engine.encryption {
        let encrypted = enc.encrypt(record.content.as_bytes())?;
        record.content = base64::engine::general_purpose::STANDARD.encode(&encrypted);
    }

    // --- Persist + index --------------------------------------------------
    engine.storage.insert_memory(&record).await?;
    engine.index.add(id, &embedding)?;
    if let Some(ref ft) = engine.full_text {
        ft.add(id, &record.content)?;
        ft.commit()?;
    }

    // Evidence relations: topic_document --consolidated_from--> member.
    for m in &members {
        let relation = Relation {
            id: Uuid::now_v7(),
            source_id: id,
            target_id: m.id,
            relation_type: "consolidated_from".to_string(),
            weight: 1.0,
            metadata: serde_json::Value::Object(serde_json::Map::new()),
            created_at: now.clone(),
        };
        if let Err(e) = engine.storage.insert_relation(&relation).await {
            tracing::error!(relation_id = %relation.id, error = %e, "failed to insert consolidation relation");
        }
    }

    // --- Revision: mark the old document superseded, keep its history -----
    // The new document is the current version (`version = old.version + 1`,
    // `prev_version_id = old.id`). The old document is RETAINED (not
    // soft-deleted): deleting a mid-chain record would orphan the next
    // record's `prev_hash` and break `verify_integrity`. Instead it is
    // marked `Consolidated` with a `superseded_by` pointer. Recall callers
    // collapse to the current view with the current-fact resolver keyed on
    // the shared `topic` metadata (revisions reuse the same `topic_name`).
    if let Some(old_id) = superseded_id
        && let Some(mut old) = engine.storage.get_memory(old_id).await?
    {
        if let serde_json::Value::Object(ref mut m) = old.metadata {
            m.insert(
                "superseded_by".to_string(),
                serde_json::json!(id.to_string()),
            );
        }
        old.consolidation_state = ConsolidationState::Consolidated;
        old.updated_at = chrono::Utc::now().to_rfc3339();
        if let Err(e) = engine.storage.update_memory(&old).await {
            tracing::error!(memory_id = %old_id, error = %e, "failed to mark superseded topic document");
        }
    }

    // --- Audit events (hash-chained) -------------------------------------
    let consolidation_event = super::event_builder::build_event(
        engine,
        &agent_id,
        EventType::MemoryConsolidated,
        serde_json::json!({
            "topic": topic,
            "consolidated_from": consolidated_from,
            "topic_document_id": id.to_string(),
            "version": version,
        }),
        &id.to_string(),
        request.thread_id.clone(),
    )
    .await;
    let consolidation_event_id = consolidation_event.id;
    if let Err(e) = engine.storage.insert_event(&consolidation_event).await {
        tracing::error!(event_id = %consolidation_event.id, error = %e, "failed to insert consolidation event");
    }

    let revision_event_id = if let Some(old_id) = superseded_id {
        let revision_event = super::event_builder::build_event(
            engine,
            &agent_id,
            EventType::MemoryRevised,
            serde_json::json!({
                "superseded_id": old_id.to_string(),
                "superseded_by": id.to_string(),
                "topic": topic,
            }),
            &id.to_string(),
            request.thread_id.clone(),
        )
        .await;
        let rid = revision_event.id;
        if let Err(e) = engine.storage.insert_event(&revision_event).await {
            tracing::error!(event_id = %revision_event.id, error = %e, "failed to insert revision event");
        }
        Some(rid)
    } else {
        None
    };

    let member_ids: Vec<Uuid> = members.iter().map(|m| m.id).collect();
    let source_count = members.len();

    // Cache the new topic document like the write path does.
    if let Some(ref cache) = engine.cache {
        cache.put(record);
    }

    Ok(ConsolidateResponse {
        topic_document_id: id,
        topic_name: topic,
        source_count,
        version,
        superseded_id,
        member_ids,
        content_hash: hex::encode(&content_hash),
        consolidation_event_id,
        revision_event_id,
    })
}
