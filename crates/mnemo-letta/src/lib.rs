//! v0.4.0-rc3 (Task B5) — Letta-protocol-compatible REST surface.
//!
//! Letta (formerly MemGPT) standardised three REST shapes that the
//! Letta-Code competitor benchmarks rely on:
//!
//! 1. `POST /v1/agents` — create a new agent and return an
//!    `agent_id`. Mnemo treats Letta agents as our own `agent_id`s
//!    (one Mnemo `MnemoEngine` can host many Letta agents at once).
//!
//! 2. `POST /v1/agents/{agent_id}/messages` — submit a message to
//!    an agent and get back the assistant's reply. Mnemo persists the
//!    user message as a `MemoryRecord` (memory_type=Episodic) and
//!    answers from a recall over the agent's memories.
//!
//! 3. `GET  /v1/agents/{agent_id}/memory` — return the agent's
//!    current "core memory" — the persona/human blocks Letta exposes
//!    to its agent loop. Mnemo maps these to memory_type=Semantic
//!    records tagged `letta-block:persona` / `letta-block:human`.
//!
//! Three endpoints is the minimum that lets a Letta-Code-shaped
//! benchmark or sample notebook talk to Mnemo without code changes.
//! The shapes are deliberately tolerant of forward-compat fields the
//! Letta team may add later — extra keys deserialise into a catch-all
//! map rather than failing the request.

pub mod handlers;
pub mod model;

use std::sync::Arc;

use axum::Router;
use axum::routing::{get, post};
use mnemo_core::query::MnemoEngine;

/// Build the Letta-compatible router. Mount it under `/letta` in a
/// host application or run it standalone.
pub fn router(engine: Arc<MnemoEngine>) -> Router {
    Router::new()
        .route("/v1/agents", post(handlers::create_agent))
        .route(
            "/v1/agents/{agent_id}/messages",
            post(handlers::send_message),
        )
        .route("/v1/agents/{agent_id}/memory", get(handlers::get_memory))
        .with_state(engine)
}
