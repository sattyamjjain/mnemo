//! Authenticated Streamable HTTP transport for the MCP server.
//!
//! Feature-gated behind `http-transport`; **stdio remains the default**. This
//! is the multi-caller transport [#126](https://github.com/sattyamjjain/mnemo/issues/126)
//! defers its capability leases to — "the lease model earns its keep on a
//! multi-caller, authenticated transport ... where distinct callers hold
//! distinct identities".
//!
//! # How a caller is identified
//!
//! Identity resolution is **not** implemented here. rmcp's
//! [`StreamableHttpService`] injects the request's `http::request::Parts` into
//! the rmcp request extensions, and `mnemo_mcp::identity` reads the
//! `Authorization: Bearer <base64url(capability)>` header out of them — the
//! same resolver `_meta` capabilities go through on stdio.
//!
//! That is deliberate. One resolver with two carriers cannot drift; a
//! transport that authenticated callers *its own way* would be a second
//! security path to keep in sync, and this repo's recurring defect is exactly
//! a rule enforced in one place and assumed in another.
//!
//! # Why a capability key is mandatory here
//!
//! On stdio, no key means "one process, one caller" — coherent, because the
//! operator *is* the caller. On a network-facing port it would mean every
//! client silently resolves to the boot identity: a multi-caller transport
//! with a single-caller identity model. [`serve`] refuses to start rather than
//! offer that.

use std::net::SocketAddr;
use std::sync::Arc;

use mnemo_mcp::server::MnemoServer;
use rmcp::transport::streamable_http_server::{
    StreamableHttpService, session::local::LocalSessionManager,
};

/// Path the MCP endpoint is mounted at.
pub const MCP_PATH: &str = "/mcp";

/// Serve MCP over Streamable HTTP until the process is shut down.
///
/// `server` must already carry a capability issuer
/// ([`MnemoServer::with_capability_issuer`]); this refuses to start without
/// one, for the reason in the module docs.
pub async fn serve(
    server: MnemoServer,
    port: u16,
    has_capability_issuer: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if !has_capability_issuer {
        return Err(
            "--http-port requires --capability-key (or MNEMO_CAPABILITY_KEY).\n\
             \n\
             Without a key the server cannot verify per-request capabilities, so every \
             caller on this port would resolve to the boot --agent-id: a multi-caller \
             transport with a single-caller identity model. Refusing to start rather \
             than serve that."
                .into(),
        );
    }

    // `NeverSessionManager` would be the stateless choice, but the local
    // manager is what rmcp uses for pre-2026-07-28 clients that still negotiate
    // sessions. Identity does NOT ride the session either way — ADR 0002
    // rejected connection-scoped identity precisely because "a connection-scoped
    // principal IS a session token wearing different clothes". The session here
    // carries transport bookkeeping only; every call re-verifies its own
    // capability.
    let session_manager = Arc::new(LocalSessionManager::default());

    let mut config = rmcp::transport::streamable_http_server::StreamableHttpServerConfig::default();
    // Plain JSON responses for simple request/response tools; rmcp falls back to
    // SSE automatically if a handler emits a notification mid-call.
    config.json_response = true;

    let service = StreamableHttpService::new(move || Ok(server.clone()), session_manager, config);

    let app = axum::Router::new().nest_service(MCP_PATH, service);

    // Loopback by default. Binding 0.0.0.0 would expose the port to the network
    // on a surface whose TLS termination is the operator's responsibility, not
    // ours — see the deployment note in docs.
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener =
        tokio::net::TcpListener::bind(addr)
            .await
            .map_err(|e| -> Box<dyn std::error::Error> {
                format!("binding MCP HTTP transport on {addr}: {e}").into()
            })?;

    tracing::info!("Starting Mnemo MCP server on http://{addr}{MCP_PATH} (Streamable HTTP)");
    tracing::info!(
        "Per-request identity is ON: callers authenticate with \
         `Authorization: Bearer <base64url capability>`"
    );

    axum::serve(listener, app)
        .await
        .map_err(|e| -> Box<dyn std::error::Error> {
            format!("MCP HTTP transport terminated: {e}").into()
        })?;
    Ok(())
}
