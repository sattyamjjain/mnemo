pub mod handlers;

use std::sync::Arc;

use axum::routing::{get, post};
use axum::Router;
use mnemo_core::query::MnemoEngine;

/// Construct the full Axum router for the Mnemo REST API.
///
/// All routes are nested under `/v1/` and the router carries
/// `Arc<MnemoEngine>` as shared state.
pub fn router(engine: Arc<MnemoEngine>) -> Router {
    Router::new()
        .route(
            "/v1/memories",
            post(handlers::remember_handler).get(handlers::recall_handler),
        )
        .route(
            "/v1/memories/{id}",
            get(handlers::get_memory_handler).delete(handlers::forget_handler),
        )
        .route("/v1/memories/{id}/share", post(handlers::share_handler))
        .route("/v1/checkpoints", post(handlers::checkpoint_handler))
        .route("/v1/branches", post(handlers::branch_handler))
        .route("/v1/merge", post(handlers::merge_handler))
        .route("/v1/replay", post(handlers::replay_handler))
        .route("/v1/verify", post(handlers::verify_handler))
        .route("/v1/delegate", post(handlers::delegate_handler))
        .route("/v1/ingest/otlp", post(handlers::otlp_ingest_handler))
        .route("/v1/health", get(handlers::health_handler))
        .layer(tower_http::cors::CorsLayer::permissive())
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(engine)
}
