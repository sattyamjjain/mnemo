pub mod handlers;

use std::sync::Arc;

use axum::routing::{get, post};
use axum::Router;
use mnemo_core::query::MnemoEngine;

/// Construct the Axum router for the Mnemo admin dashboard.
///
/// Mounts all admin API endpoints under `/admin/api/` and the HTML dashboard
/// at `/admin/`. The router carries `Arc<MnemoEngine>` as shared state.
///
/// # Routes
///
/// | Method | Path                            | Description                    |
/// |--------|---------------------------------|--------------------------------|
/// | GET    | `/admin/`                       | HTML dashboard                 |
/// | GET    | `/admin/api/health`             | Health check                   |
/// | GET    | `/admin/api/stats`              | Aggregate statistics           |
/// | GET    | `/admin/api/agents`             | List known agent IDs           |
/// | GET    | `/admin/api/memories`           | Paginated memory browser       |
/// | GET    | `/admin/api/events`             | Paginated event timeline       |
/// | POST   | `/admin/api/quarantine/:id`     | Quarantine a memory            |
/// | POST   | `/admin/api/unquarantine/:id`   | Release memory from quarantine |
pub fn router(engine: Arc<MnemoEngine>) -> Router {
    Router::new()
        // Dashboard
        .route("/admin/", get(handlers::dashboard_handler))
        // API
        .route("/admin/api/health", get(handlers::health_handler))
        .route("/admin/api/stats", get(handlers::stats_handler))
        .route("/admin/api/agents", get(handlers::agents_handler))
        .route("/admin/api/memories", get(handlers::memories_handler))
        .route("/admin/api/events", get(handlers::events_handler))
        .route(
            "/admin/api/quarantine/{id}",
            post(handlers::quarantine_handler),
        )
        .route(
            "/admin/api/unquarantine/{id}",
            post(handlers::unquarantine_handler),
        )
        .layer(tower_http::cors::CorsLayer::permissive())
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(engine)
}
