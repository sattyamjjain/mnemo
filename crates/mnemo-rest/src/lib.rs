pub mod handlers;

use std::sync::Arc;

use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post};
use axum::Router;
use mnemo_core::query::MnemoEngine;
use tower_http::cors::{AllowOrigin, CorsLayer};

/// Construct the full Axum router for the Mnemo REST API.
///
/// All routes are nested under `/v1/` and the router carries
/// `Arc<MnemoEngine>` as shared state.
///
/// CORS is restrictive by default (localhost only). Set the
/// `MNEMO_CORS_ORIGINS` environment variable to a comma-separated
/// list of allowed origins to override (e.g. `https://app.example.com`).
/// Set it to `*` to allow all origins (not recommended for production).
pub fn router(engine: Arc<MnemoEngine>) -> Router {
    let cors = build_cors_layer();

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
        .layer(DefaultBodyLimit::max(2 * 1024 * 1024)) // 2 MB max request body
        .layer(cors)
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(engine)
}

fn build_cors_layer() -> CorsLayer {
    use axum::http::{HeaderName, Method};

    let base = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST, Method::DELETE, Method::OPTIONS])
        .allow_headers([
            HeaderName::from_static("content-type"),
            HeaderName::from_static("authorization"),
        ])
        .max_age(std::time::Duration::from_secs(3600));

    match std::env::var("MNEMO_CORS_ORIGINS") {
        Ok(val) if val == "*" => base.allow_origin(AllowOrigin::any()),
        Ok(val) => {
            let origins: Vec<_> = val
                .split(',')
                .filter_map(|s| s.trim().parse().ok())
                .collect();
            base.allow_origin(origins)
        }
        Err(_) => {
            // Default: localhost only
            let origins: Vec<_> = [
                "http://localhost:3000",
                "http://localhost:8080",
                "http://127.0.0.1:3000",
                "http://127.0.0.1:8080",
            ]
            .iter()
            .filter_map(|s| s.parse().ok())
            .collect();
            base.allow_origin(origins)
        }
    }
}
