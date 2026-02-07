use serde::{Deserialize, Serialize};
use uuid::Uuid;
use std::collections::{HashSet, VecDeque};

use crate::error::Result;
use crate::model::event::{AgentEvent, EventType};
use crate::query::MnemoEngine;

/// Direction for causal chain traversal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TraceDirection {
    /// Walk upward through `parent_event_id` links (ancestors).
    Up,
    /// Walk downward through child events (descendants). This is the original behavior.
    Down,
    /// Combine upward and downward traversal, deduplicating by event ID.
    Both,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CausalChain {
    pub root: Uuid,
    pub nodes: Vec<CausalNode>,
    pub depth: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CausalNode {
    pub event: AgentEvent,
    pub children: Vec<Uuid>,
    pub depth: usize,
}

/// Trace a causal chain starting from a root event.
///
/// - `direction`: controls whether to walk upward (ancestors), downward (descendants), or both.
/// - `event_type_filter`: when `Some`, only events matching the given `EventType` are included
///   in the returned nodes. However, traversal still proceeds through non-matching events to
///   preserve connectivity (i.e., filtering is applied to output, not to graph exploration).
pub async fn trace_causality(
    engine: &MnemoEngine,
    event_id: Uuid,
    max_depth: usize,
    direction: TraceDirection,
    event_type_filter: Option<EventType>,
) -> Result<CausalChain> {
    // Load root event
    let root_event = engine
        .storage
        .get_event(event_id)
        .await?
        .ok_or_else(|| crate::error::Error::NotFound(format!("event {event_id} not found")))?;

    let mut seen = HashSet::new();
    let mut nodes: Vec<CausalNode> = Vec::new();
    let mut actual_depth: usize = 0;

    // Helper closure: decide whether an event passes the filter.
    let passes_filter = |event: &AgentEvent| -> bool {
        match &event_type_filter {
            Some(filter) => event.event_type == *filter,
            None => true,
        }
    };

    // Always include the root if it passes the filter.
    seen.insert(event_id);
    if passes_filter(&root_event) {
        nodes.push(CausalNode {
            event: root_event.clone(),
            children: Vec::new(),
            depth: 0,
        });
    }

    // --- Upward traversal ---
    if direction == TraceDirection::Up || direction == TraceDirection::Both {
        let mut current_event = root_event.clone();
        let mut depth: usize = 0;

        while depth < max_depth {
            let parent_id = match current_event.parent_event_id {
                Some(pid) => pid,
                None => break,
            };

            if !seen.insert(parent_id) {
                break; // Already visited (cycle guard)
            }

            let parent_event = match engine.storage.get_event(parent_id).await? {
                Some(e) => e,
                None => break,
            };

            depth += 1;
            actual_depth = actual_depth.max(depth);

            if passes_filter(&parent_event) {
                nodes.push(CausalNode {
                    event: parent_event.clone(),
                    children: vec![current_event.id],
                    depth,
                });
            }

            current_event = parent_event;
        }
    }

    // --- Downward traversal (BFS) ---
    if direction == TraceDirection::Down || direction == TraceDirection::Both {
        let mut queue: VecDeque<(Uuid, usize)> = VecDeque::new();
        queue.push_back((event_id, 0));

        while let Some((current_id, current_depth)) = queue.pop_front() {
            if current_depth >= max_depth {
                continue;
            }

            let children = engine.storage.list_child_events(current_id, 100).await?;
            let child_ids: Vec<Uuid> = children.iter().map(|e| e.id).collect();

            // Update the parent node's children list (if present in nodes).
            if let Some(parent_node) = nodes.iter_mut().find(|n| n.event.id == current_id) {
                parent_node.children = child_ids.clone();
            }

            for child_event in children {
                if !seen.insert(child_event.id) {
                    continue; // Already visited
                }

                let child_depth = current_depth + 1;
                actual_depth = actual_depth.max(child_depth);

                if passes_filter(&child_event) {
                    nodes.push(CausalNode {
                        event: child_event.clone(),
                        children: Vec::new(),
                        depth: child_depth,
                    });
                }

                // Continue traversal even if the event was filtered out.
                queue.push_back((child_event.id, child_depth));
            }
        }
    }

    Ok(CausalChain {
        root: event_id,
        nodes,
        depth: actual_depth,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_causal_chain_serde() {
        let chain = CausalChain {
            root: Uuid::now_v7(),
            nodes: vec![],
            depth: 0,
        };
        let json = serde_json::to_string(&chain).unwrap();
        let deserialized: CausalChain = serde_json::from_str(&json).unwrap();
        assert_eq!(chain.root, deserialized.root);
    }

    #[test]
    fn test_trace_direction_serde() {
        // Verify all variants serialize and round-trip correctly.
        let directions = vec![TraceDirection::Up, TraceDirection::Down, TraceDirection::Both];
        for dir in &directions {
            let json = serde_json::to_string(dir).unwrap();
            let deserialized: TraceDirection = serde_json::from_str(&json).unwrap();
            assert_eq!(*dir, deserialized);
        }

        // Verify the snake_case rename: "up", "down", "both".
        assert_eq!(serde_json::to_string(&TraceDirection::Up).unwrap(), "\"up\"");
        assert_eq!(serde_json::to_string(&TraceDirection::Down).unwrap(), "\"down\"");
        assert_eq!(serde_json::to_string(&TraceDirection::Both).unwrap(), "\"both\"");

        // Verify deserialization from snake_case strings.
        assert_eq!(
            serde_json::from_str::<TraceDirection>("\"up\"").unwrap(),
            TraceDirection::Up
        );
        assert_eq!(
            serde_json::from_str::<TraceDirection>("\"both\"").unwrap(),
            TraceDirection::Both
        );
    }

    #[test]
    fn test_causal_chain_filtering() {
        // Build a CausalChain with mixed event types and verify that filtering
        // logic (applied externally here, since the real filter is in the async
        // function) correctly retains only matching nodes.
        let make_event = |event_type: EventType| -> AgentEvent {
            AgentEvent {
                id: Uuid::now_v7(),
                agent_id: "agent-1".to_string(),
                thread_id: None,
                run_id: None,
                parent_event_id: None,
                event_type,
                payload: serde_json::json!({}),
                trace_id: None,
                span_id: None,
                model: None,
                tokens_input: None,
                tokens_output: None,
                latency_ms: None,
                cost_usd: None,
                timestamp: "2025-01-01T00:00:00Z".to_string(),
                logical_clock: 1,
                content_hash: vec![],
                prev_hash: None,
                embedding: None,
            }
        };

        let write_event = make_event(EventType::MemoryWrite);
        let read_event = make_event(EventType::MemoryRead);
        let checkpoint_event = make_event(EventType::Checkpoint);

        let all_nodes = vec![
            CausalNode { event: write_event.clone(), children: vec![], depth: 0 },
            CausalNode { event: read_event.clone(), children: vec![], depth: 1 },
            CausalNode { event: checkpoint_event.clone(), children: vec![], depth: 2 },
        ];

        // Simulate filtering for MemoryWrite only.
        let filter = EventType::MemoryWrite;
        let filtered: Vec<&CausalNode> = all_nodes
            .iter()
            .filter(|n| n.event.event_type == filter)
            .collect();

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].event.event_type, EventType::MemoryWrite);

        // Simulate filtering for MemoryRead.
        let filter_read = EventType::MemoryRead;
        let filtered_read: Vec<&CausalNode> = all_nodes
            .iter()
            .filter(|n| n.event.event_type == filter_read)
            .collect();

        assert_eq!(filtered_read.len(), 1);
        assert_eq!(filtered_read[0].event.id, read_event.id);

        // No filter: all nodes are present.
        assert_eq!(all_nodes.len(), 3);
    }
}
