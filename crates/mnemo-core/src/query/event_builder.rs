use uuid::Uuid;

use crate::model::event::{AgentEvent, EventType};
use crate::query::MnemoEngine;

/// Build an AgentEvent with proper hash chain linking.
///
/// Looks up the latest event hash for the agent and computes prev_hash
/// to maintain chain integrity. This centralizes event construction
/// that was previously duplicated across 8 query files.
pub async fn build_event(
    engine: &MnemoEngine,
    agent_id: &str,
    event_type: EventType,
    payload: serde_json::Value,
    content_for_hash: &str,
    thread_id: Option<String>,
) -> AgentEvent {
    let now = chrono::Utc::now().to_rfc3339();
    let event_content_hash = crate::hash::compute_content_hash(content_for_hash, agent_id, &now);
    let prev_event_hash = match engine.storage.get_latest_event_hash(agent_id, None).await {
        Ok(hash) => hash,
        Err(e) => {
            tracing::warn!(error = %e, "failed to get latest event hash, starting new chain segment");
            None
        }
    };
    let event_prev_hash = Some(crate::hash::compute_chain_hash(
        &event_content_hash,
        prev_event_hash.as_deref(),
    ));

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
        prev_hash: event_prev_hash,
        embedding: None,
    }
}
