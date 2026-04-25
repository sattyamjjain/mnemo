//! Bitemporal graph layer for Mnemo.
//!
//! Inspired by Graphiti ([repo](https://github.com/getzep/graphiti),
//! [paper](https://arxiv.org/abs/2501.13956)). The model is the same:
//! every edge carries `valid_from` / `valid_to` (when the *fact* is
//! true in the world) plus `recorded_at` (when the system saw it),
//! so historical queries can ask "what did we believe at time T?"
//! without losing later corrections.
//!
//! ```text
//! valid_from              valid_to (None = still true)
//!     ^                       ^
//!     |   fact validity       |
//!     +-----------------------+
//!     |
//!     +-- recorded_at (when we wrote the row)
//! ```
//!
//! Today this crate ships:
//!
//! 1. The [`TemporalEdge`] type and a [`GraphStore`] async trait.
//! 2. A DuckDB-backed [`DuckGraphStore`] that creates `graph_nodes`
//!    and `graph_edges` tables on first use and supports the round-trip
//!    + bitemporal `as_of` walk needed by retrieval.
//! 3. [`graph_expand`] — bounded BFS that respects `as_of` filtering
//!    and a maximum depth.
//!
//! The LLM-driven [`TemporalEdge::extract`] path is feature-gated under
//! `graph-extract` and currently returns an empty `Vec`. A real
//! extractor lands in v0.4.0 final once the prompt + ICL examples are
//! tuned.

pub mod extract;
pub mod model;
pub mod store;

pub use crate::model::TemporalEdge;
pub use crate::store::{GraphStore, duckdb::DuckGraphStore};

use chrono::{DateTime, Utc};
use std::collections::{HashSet, VecDeque};
use uuid::Uuid;

use crate::store::Result;

/// Bounded BFS from `seed` that respects bitemporal validity at
/// `as_of` and a max walk depth.
///
/// Returns every UUID reachable through edges whose
/// `valid_from <= as_of < valid_to.unwrap_or(MAX)`. Self-loops are
/// dropped. The seed is included in the returned set unless the
/// caller filters it out themselves.
pub async fn graph_expand(
    store: &dyn GraphStore,
    seed: Uuid,
    depth: u8,
    as_of: DateTime<Utc>,
) -> Result<Vec<Uuid>> {
    let mut visited: HashSet<Uuid> = HashSet::new();
    let mut frontier: VecDeque<(Uuid, u8)> = VecDeque::new();
    frontier.push_back((seed, 0));
    visited.insert(seed);

    while let Some((node, d)) = frontier.pop_front() {
        if d == depth {
            continue;
        }
        for edge in store.outgoing_at(node, as_of).await? {
            if edge.dst == node {
                continue;
            }
            if visited.insert(edge.dst) {
                frontier.push_back((edge.dst, d + 1));
            }
        }
    }
    Ok(visited.into_iter().collect())
}
