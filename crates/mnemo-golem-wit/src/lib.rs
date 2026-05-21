//! v0.4.6 — `mnemo-golem-wit` — WASM-component bindings for the
//! [`golem:vector`](https://github.com/golemcloud/golem-ai/issues/21)
//! WIT interface, vertical-slice (3 of ~30 upstream functions:
//! `upsert-vector`, `search-vectors`, `delete-vectors`).
//!
//! # Two-crate host-runner architecture
//!
//! mnemo's storage engine (`mnemo-core`) depends on `duckdb` and
//! `usearch` — both C++ libraries that cannot compile to
//! `wasm32-wasip2`. That rules out calling `mnemo-core` directly
//! from inside the component. The v0.4.6 design splits the
//! integration across two crates:
//!
//! - **this crate (`mnemo-golem-wit`)**: WIT bindings + a
//!   `Component` struct implementing the exported `vectors`
//!   interface. Each export delegates to a corresponding host
//!   import (`host-upsert` / `host-search` / `host-delete`).
//!   Compiles to `wasm32-wasip2` via `cargo component build`.
//!
//! - **[`mnemo-golem-host`](../../mnemo-golem-host)**: a regular
//!   Rust crate that supplies the host imports backed by a real
//!   `mnemo_core::MnemoEngine`. Carries the integration test
//!   suite (because it's what an integrator would use).
//!
//! # What this crate is NOT
//!
//! - **Not the full golem:vector contract.** Only 3 of ~30 upstream
//!   functions. See [`docs/research/golem-vector-wit-provider.md`](../../../docs/research/golem-vector-wit-provider.md)
//!   for the per-function gap list + the layering rationale +
//!   what's deferred to v0.5.x.
//! - **Not a Golem durability claim.** Golem's runtime is a
//!   durable-execution host; this component runs on it the same
//!   way any other guest does, but mnemo does not introspect
//!   Golem's checkpoint protocol.

#[allow(warnings)]
#[allow(clippy::all)]
mod bindings;

use bindings::exports::mnemo::golem_vector::vectors::{Guest, SearchResult, VectorError};
use bindings::mnemo::golem_vector::host;

/// The component struct `cargo component build` packages into a
/// WASM artifact. Implements the bindgen-generated `Guest` trait
/// for `mnemo:golem-vector/vectors`. Each method wraps a host
/// import call and translates a stringly-typed host error into
/// the WIT `vector-error::provider-error(string)` variant.
struct Component;

impl Guest for Component {
    fn upsert_vector(collection: String, id: String, vector: Vec<f32>) -> Result<(), VectorError> {
        if vector.is_empty() {
            return Err(VectorError::InvalidParams("empty vector".into()));
        }
        host::host_upsert(&collection, &id, &vector).map_err(VectorError::ProviderError)
    }

    fn search_vectors(
        collection: String,
        query: Vec<f32>,
        limit: u32,
    ) -> Result<Vec<SearchResult>, VectorError> {
        if query.is_empty() {
            return Err(VectorError::InvalidParams("empty query".into()));
        }
        let hits =
            host::host_search(&collection, &query, limit).map_err(VectorError::ProviderError)?;
        Ok(hits
            .into_iter()
            .map(|(id, score)| SearchResult { id, score })
            .collect())
    }

    fn delete_vectors(collection: String, ids: Vec<String>) -> Result<u32, VectorError> {
        host::host_delete(&collection, &ids).map_err(VectorError::ProviderError)
    }
}

bindings::export!(Component with_types_in bindings);
