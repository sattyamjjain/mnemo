# golem:vector WIT provider — host-runner anchor (v0.4.6)

> Recorded 2026-05-21. **Vertical-slice ship + design anchor for the
> full integration.** mnemo v0.4.6 ships the host-runner
> architecture and the load-bearing three functions
> (`upsert-vector`, `search-vectors`, `delete-vectors`) of the
> [`golem:vector@1.0.0`](https://github.com/golemcloud/golem-ai/issues/21)
> WIT contract. The remaining ~27 functions across the Collections /
> Search-Extended / Analytics / Namespaces / Connection interfaces
> are explicitly deferred to a v0.5.x follow-up — see the gap list
> below.

## Anchor

- Upstream issue: [golemcloud/golem-ai#21](https://github.com/golemcloud/golem-ai/issues/21).
- WIT package: `golem:vector@1.0.0`.
- v0.4.6 implements package `mnemo:golem-vector@0.1.0` — a strict
  subset of the upstream contract, namespaced under `mnemo:` to
  signal that this is mnemo's slice, not the full upstream
  contract.

## Architectural reality — why the two-crate split

`mnemo-core` depends on:

- `duckdb` with the `bundled` feature → C++ via libcxx.
- `usearch` with the `bundled` feature → C++ HNSW lib via cxx
  bindings.

Neither compiles to `wasm32-wasip2`. Implementing the WIT functions
by directly calling `mnemo-core` from inside a WASM component is
fundamentally blocked at the toolchain level.

v0.4.6 takes the **host-runner pattern**: the WASM component is a
*thin guest* whose exports each call into a host import; the host
(a regular Rust process) supplies those imports backed by the real
`MnemoEngine`. Two crates:

| Crate | Target | Role |
|---|---|---|
| [`mnemo-golem-wit`](../../crates/mnemo-golem-wit) | `wasm32-wasip2` (cdylib) | Implements WIT exports, delegates to WIT imports |
| [`mnemo-golem-host`](../../crates/mnemo-golem-host) | Native (Rust binary or library) | Owns an `Arc<MnemoEngine>`; provides the WIT host imports |

A single component WASM artifact ships at `target/wasm32-wasip1/release/mnemo_golem_wit.wasm` (~73K stripped).

## WIT subset — what v0.4.6 implements

```wit
package mnemo:golem-vector@0.1.0;

interface vectors {
    upsert-vector(collection, id, vector)        → result<_, vector-error>
    search-vectors(collection, query, limit)     → result<list<search-result>, vector-error>
    delete-vectors(collection, ids)              → result<u32, vector-error>
}

interface host {
    host-upsert(collection, id, vector)          → result<_, string>
    host-search(collection, query, limit)        → result<list<tuple<string, f32>>, string>
    host-delete(collection, ids)                 → result<u32, string>
}

world golem-vector-provider {
    import host;
    export vectors;
}
```

The component is intentionally thin: each export validates input,
calls the corresponding host import, and wraps a stringly-typed
host error into the WIT `vector-error::provider-error(string)`
variant.

## Today's vertical slice — Rust-native host trait + mnemo integration

The host crate ships:

- `trait MnemoGolemProvider` — async Rust shape of the three host
  imports.
- `struct MnemoGolemHost { engine: Arc<MnemoEngine> }` — backs the
  trait with mnemo's `remember` / `recall` (semantic, top-K) /
  `forget` (HardDelete) operations.
- 5 integration tests in `crates/mnemo-golem-host/src/lib.rs::tests`
  exercising put → search round-trip, collection scoping (mapped to
  `agent_id`), delete-removes-only-targeted-ids, and the two empty-
  input refusal paths.
- One example,
  [`crates/mnemo-golem-host/examples/golem_agent_round_trip.rs`](../../crates/mnemo-golem-host/examples/golem_agent_round_trip.rs),
  driving REMEMBER → RECALL → DELETE through the Rust API and
  showing the round trip end-to-end (3 upserts + 1 search + 1
  delete + 1 post-delete search).

## What's deferred to v0.5.x

### The wasmtime-component-loader wiring

v0.4.6 ships the Rust trait + mnemo-core integration AND ships the
WASM component (cdylib that compiles cleanly to `wasm32-wasip2`).
What's **not** wired today is the `wasmtime::component::Linker`
glue that lets the host crate instantiate the WASM component and
service its imports from a `Store<HostState>` carrying a
`MnemoGolemHost`. That glue is mechanical (bindgen via
`wasmtime::component::bindgen!`, `Linker::instance` calls, async
trampoline through `tokio::runtime::Handle::block_on`) but takes a
real day of API-version-pinning work because wasmtime's
component-model surface moves between minor versions.

The integration is **functionally complete** as a Rust-trait surface
today — an integrator wanting to invoke the WASM artifact wires the
Linker per their wasmtime version. A v0.5.x row lands the
canonical wiring + a `cargo run --example wasm_round_trip` that
loads the .wasm via wasmtime, dispatches three calls, and asserts
the same round-trip output the existing example produces over the
Rust trait.

### The remaining 27 WIT functions

| Interface | Count | Reason for deferral |
|---|---|---|
| Collections (`upsert-collection`, `list-collections`, `get-collection`, `update-collection`, `delete-collection`, `collection-exists`) | 6 | mnemo doesn't model collections as first-class entities — they're per-`agent_id` namespaces today; first-class collection metadata is a v0.5.x storage-schema row |
| Vectors-extended (`upsert-vectors` batch, `get-vector`(s), `update-vector`, `delete-by-filter`, `delete-namespace`, `list-vectors`, `count-vectors`) | 8 | Batch + filter operators (`eq`/`ne`/`gt`/`%in`/`regex`/`geo-within`/etc.) need mnemo's `MemoryFilter` extended; out of scope today |
| Search-Extended (`recommend-vectors`, `discover-vectors`, `search-groups`, `search-range`, `search-text`) | 5 | Each needs a non-trivial mnemo query mode (e.g. `recommend-vectors` ≈ positive/negative-example hybrid); `search-text` is the closest fit to mnemo's existing BM25 path but the WIT contract differs |
| Analytics (`get-collection-stats`, `get-field-stats`, `get-field-distribution`) | 3 | Per-collection / per-field stats need the collection-as-entity rework first |
| Namespaces (`upsert-namespace`, `list-namespaces`, `get-namespace`, `delete-namespace`, `namespace-exists`) | 5 | Same blocker — namespaces nested under collections need the collection rework |
| Connection (`connect`, `disconnect`, `get-connection-status`, `test-connection`) | 4 | mnemo doesn't have a "connection" concept — these would no-op or return synthetic status |

Total: **27 deferred**, **3 shipped** = 30-function upstream contract.

## Honest scope — what this anchor is NOT

- **NOT a Golem-runtime durability claim.** Golem's runtime is a
  durable-execution host; this component runs on it the same way
  any other guest does, but mnemo does not introspect Golem's
  checkpoint protocol. Claiming "Golem-durable by construction"
  would be overclaim — blocked by today's marketing-phrase test
  banlist extension.
- **NOT a multi-provider abstraction.** This is one provider
  (mnemo). The upstream WIT contemplates pluggable backends
  (Qdrant, Pinecone, Milvus, pgvector); routing across them is
  out of scope.
- **NOT a real embedder integration.** Vectors arrive pre-computed
  via the WIT; mnemo's hybrid retrieval (vector + BM25 + graph +
  recency) only exercises the vector lane in today's slice. A
  future row may extend `RecallRequest` with a `ProvidedEmbedding`
  field so the WIT-provided vector lands in mnemo's index
  directly without re-embedding the query text.
- **NOT a bounty-claimable submission for the full golem:vector
  contract.** The bounty (per the upstream issue) expects all
  primary interfaces; v0.4.6 is the host-runner scaffold + the
  vertical slice, not the full contract. A v0.5.x follow-up
  closes the remaining 27 functions.

## Cross-references

- WIT file: [`crates/mnemo-golem-wit/wit/world.wit`](../../crates/mnemo-golem-wit/wit/world.wit)
- Component impl: [`crates/mnemo-golem-wit/src/lib.rs`](../../crates/mnemo-golem-wit/src/lib.rs)
- Host trait + impl: [`crates/mnemo-golem-host/src/lib.rs`](../../crates/mnemo-golem-host/src/lib.rs)
- Example: [`crates/mnemo-golem-host/examples/golem_agent_round_trip.rs`](../../crates/mnemo-golem-host/examples/golem_agent_round_trip.rs)
- WASM artifact (release build): `target/wasm32-wasip1/release/mnemo_golem_wit.wasm`
- Companion v0.4.5 substrate anchor: [`context-memorization-2605.18226.md`](context-memorization-2605.18226.md)

## Sources

- Upstream issue: https://github.com/golemcloud/golem-ai/issues/21 — *golem:vector WIT v1.0.0*.
- Toolchain: `cargo-component v0.21.1` + `wasm32-wasip2` rustup target + `wasmtime v27.0` (component-model feature).
