pub mod handlers;

use std::sync::Arc;

use axum::Router;
use axum::extract::{DefaultBodyLimit, Request, State};
use axum::http::{Method, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::routing::{get, post};
use mnemo_core::query::MnemoEngine;
use tower_http::cors::{AllowOrigin, CorsLayer};

/// Construct the full Axum router for the Mnemo REST API, reading the
/// bearer-token secret from the `MNEMO_AUTH_TOKEN` environment variable.
///
/// When `MNEMO_AUTH_TOKEN` is set (non-empty), every request except
/// `/v1/health` and CORS preflight (`OPTIONS`) must carry a matching
/// `Authorization: Bearer <token>` header or it is rejected with `401`. When
/// the variable is unset, the server runs **open** and logs a warning — the
/// floor for "don't run an unauthenticated memory server" is opt-in but loud.
///
/// All routes are nested under `/v1/` and the router carries
/// `Arc<MnemoEngine>` as shared state. CORS is restrictive by default
/// (localhost only); set `MNEMO_CORS_ORIGINS` to override.
pub fn router(engine: Arc<MnemoEngine>) -> Router {
    let token = std::env::var("MNEMO_AUTH_TOKEN")
        .ok()
        .filter(|s| !s.is_empty());
    router_with_auth(engine, token)
}

/// Like [`router`] but with the bearer secret passed explicitly (so tests and
/// embedders can configure auth without touching the process environment).
/// `Some(token)` enables bearer auth; `None` runs open (with a warning).
pub fn router_with_auth(engine: Arc<MnemoEngine>, auth_token: Option<String>) -> Router {
    let cors = build_cors_layer();

    let app = Router::new()
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
        .route("/v1/consolidate", post(handlers::consolidate_handler))
        .route("/v1/branches", post(handlers::branch_handler))
        .route("/v1/merge", post(handlers::merge_handler))
        .route("/v1/replay", post(handlers::replay_handler))
        .route("/v1/verify", post(handlers::verify_handler))
        .route(
            "/v1/compliance/trajectory_audit",
            post(handlers::trajectory_audit_handler),
        )
        .route("/v1/delegate", post(handlers::delegate_handler))
        .route("/v1/forget_subject", post(handlers::forget_subject_handler))
        .route("/v1/ingest/otlp", post(handlers::otlp_ingest_handler))
        .route("/v1/health", get(handlers::health_handler))
        .layer(DefaultBodyLimit::max(2 * 1024 * 1024)) // 2 MB max request body
        .layer(cors)
        .layer(tower_http::trace::TraceLayer::new_for_http());

    // Bearer-token gate (outermost so it runs before handlers). When unset,
    // run open but log loudly — never silently serve an unauthenticated
    // memory database without surfacing it.
    let app = match auth_token {
        Some(token) if !token.is_empty() => {
            tracing::info!(
                "REST bearer-token auth ENABLED (Authorization: Bearer <MNEMO_AUTH_TOKEN>)"
            );
            app.layer(middleware::from_fn_with_state(
                Arc::new(token),
                require_bearer,
            ))
        }
        _ => {
            tracing::warn!(
                "REST API running WITHOUT authentication — set MNEMO_AUTH_TOKEN to require a \
                 bearer token. Do not expose an unauthenticated memory server."
            );
            app
        }
    };

    app.with_state(engine)
}

/// Axum middleware: require `Authorization: Bearer <expected>` on every request
/// except `/v1/health` and CORS preflight (`OPTIONS`). Returns `401` otherwise.
async fn require_bearer(
    State(expected): State<Arc<String>>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // Liveness probes and CORS preflight must not require the secret.
    if req.method() == Method::OPTIONS || req.uri().path() == "/v1/health" {
        return Ok(next.run(req).await);
    }
    let provided = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());
    if mnemo_core::auth::bearer_token_matches(provided, &expected) {
        Ok(next.run(req).await)
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
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
