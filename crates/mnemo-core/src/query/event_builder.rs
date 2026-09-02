use uuid::Uuid;

use crate::model::event::{AgentEvent, EventType};
use crate::query::MnemoEngine;

/// Build an `AgentEvent` ready to be appended.
///
/// `prev_hash` is left unset on purpose. It is assigned by
/// [`StorageBackend::append_event_chained`](crate::storage::StorageBackend::append_event_chained)
/// at insertion time, under the backend's own mutual exclusion, so that reading
/// the chain head and writing the event that names it cannot be interleaved.
/// Computing it here — as this did until v0.5.29 — put an arbitrary amount of
/// caller code between the read and the insert, and any two calls that
/// overlapped that window both wrote themselves as chain heads.
///
/// Pair every `build_event` with `append_event_chained`, never with
/// `insert_event`: the latter stores whatever `prev_hash` it is handed, which is
/// now `None`.
pub async fn build_event(
    _engine: &MnemoEngine,
    agent_id: &str,
    event_type: EventType,
    payload: serde_json::Value,
    content_for_hash: &str,
    thread_id: Option<String>,
) -> AgentEvent {
    let now = chrono::Utc::now().to_rfc3339();
    let event_content_hash = crate::hash::compute_content_hash(content_for_hash, agent_id, &now);
    AgentEvent {
        id: Uuid::now_v7(),
        agent_id: agent_id.to_string(),
        thread_id,
        run_id: None,
        parent_event_id: None,
        event_type,
        payload,
        trace_id: None,
        span_id: None,
        model: None,
        tokens_input: None,
        tokens_output: None,
        latency_ms: None,
        cost_usd: None,
        timestamp: now.clone(),
        logical_clock: 0,
        content_hash: event_content_hash,
        prev_hash: None,
        embedding: None,
    }
}
