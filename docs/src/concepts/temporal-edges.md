# Temporal edges (`mnemo-graph`)

The `mnemo-graph` crate (introduced in v0.4.0-rc1) adds a
**bitemporal graph layer** over the existing storage backends. It's
loosely inspired by [Graphiti (Zep)](https://github.com/getzep/graphiti)
and the [Graphiti paper](https://arxiv.org/abs/2501.13956): every
relation carries two clocks instead of one, so historical queries
can ask "what did we believe at time T?" without losing later
corrections.

## The two clocks

Every edge in the graph stores:

```text
valid_from              valid_to (None = still true)
    ^                       ^
    |   fact validity       |
    +-----------------------+
    |
    +-- recorded_at (when we wrote the row)
```

* `valid_from` / `valid_to` describe **fact validity** — when the
  relation is true in the world.
* `recorded_at` describes **system time** — when we wrote the row.
  Useful for audit replay: "show me what the graph looked like at
  recorded_at = 2026-04-15."

Without the second clock, there's no way to distinguish "we always
knew Priya works at Acme since 2025" from "we found out yesterday
Priya works at Acme since 2025." Both situations are common; both
need different answers from a debugging session.

## The `TemporalEdge` model

```rust
pub struct TemporalEdge {
    pub id: Uuid,
    pub src: Uuid,
    pub dst: Uuid,
    pub relation: String,
    pub valid_from: DateTime<Utc>,
    pub valid_to: Option<DateTime<Utc>>,   // None = still true
    pub confidence: f32,                   // [0.0, 1.0]
    pub recorded_at: DateTime<Utc>,
}
```

`relation` is a free-form string today (`"works_at"`,
`"located_in"`, `"reports_to"`). We considered an enum but
discarded it — codifying a relation set without real corpus data
risks pinning the wrong vocabulary. The full LLM extractor that
lands in v0.4.0 final will document the conventions it emits.

## Storage

Two tables — DuckDB + Postgres equivalents:

```sql
CREATE TABLE graph_nodes (
    id VARCHAR PRIMARY KEY,
    label VARCHAR,
    metadata JSON,
    created_at VARCHAR NOT NULL
);

CREATE TABLE graph_edges (
    id VARCHAR PRIMARY KEY,
    src VARCHAR NOT NULL,
    dst VARCHAR NOT NULL,
    relation VARCHAR NOT NULL,
    valid_from VARCHAR NOT NULL,
    valid_to VARCHAR,
    confidence FLOAT NOT NULL DEFAULT 1.0,
    recorded_at VARCHAR NOT NULL
);
CREATE INDEX idx_graph_edges_src_validfrom ON graph_edges(src, valid_from);
CREATE INDEX idx_graph_edges_dst ON graph_edges(dst);
```

## `graph_expand` — the bitemporal walk

```rust
use chrono::{TimeZone, Utc};
use mnemo_graph::{DuckGraphStore, GraphStore, TemporalEdge, graph_expand};

let store = DuckGraphStore::open_in_memory()?;

// Priya works at Acme starting 2026-01-01.
let priya = Uuid::now_v7();
let acme = Uuid::now_v7();
let acme_edge = TemporalEdge::new(
    priya, acme, "works_at",
    Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
    None, 0.9,
);
store.insert_edge(&acme_edge).await?;

// Priya leaves Acme on 2026-04-01 and joins Globex.
let globex = Uuid::now_v7();
store.close_edge(acme_edge.id, Utc.with_ymd_and_hms(2026, 4, 1, 0, 0, 0).unwrap()).await?;
store.insert_edge(&TemporalEdge::new(
    priya, globex, "works_at",
    Utc.with_ymd_and_hms(2026, 4, 1, 0, 0, 0).unwrap(),
    None, 0.95,
)).await?;

// Walk the graph at two different points in time.
let in_feb = Utc.with_ymd_and_hms(2026, 2, 15, 0, 0, 0).unwrap();
let in_june = Utc.with_ymd_and_hms(2026, 6, 1, 0, 0, 0).unwrap();

let reachable_feb = graph_expand(&store, priya, 2, in_feb).await?;
//     ^^^ contains acme, NOT globex (relation hadn't started yet)

let reachable_june = graph_expand(&store, priya, 2, in_june).await?;
//     ^^^ contains globex, NOT acme (relation closed at 2026-04-01)
```

This is the supersession property — without it, the graph layer
would be redundant with a regular non-temporal graph.

## Conflict resolution

When the LLM extractor (v0.4.0 final) emits a contradicting fact
with higher confidence than an existing edge, the convention is:

1. The new edge inserts with its own `valid_from` and `recorded_at`.
2. The pre-existing edge with the lower confidence has its `valid_to`
   set to the new edge's `valid_from` — capping its validity window.

The result: a sceptical operator can reconstruct the historical view
that contained the old answer (via `recorded_at`) AND the corrected
view (via `valid_from`).

## What ships in v0.4.0-rc1

| | Status |
|:--|:--|
| `TemporalEdge` model | ✓ |
| `GraphStore` async trait | ✓ |
| DuckDB-backed `DuckGraphStore` | ✓ |
| `graph_expand` BFS with `as_of` filter | ✓ |
| Postgres-backed store | _v0.4.0 final_ |
| `TemporalEdge::extract` LLM-driven | _v0.4.0 final_ |
| `hybrid_rrf` 4th-signal integration | _v0.4.0 final_ |
| MCP / REST / gRPC `graph_expand` tools | _v0.4.0 final_ |

The LLM extractor stays a stub today (`Vec::new()`) because the
prompt + ICL examples are still being tuned and shipping a
half-tuned extractor would put bad edges into everyone's graphs.

## Sources

* [Graphiti repo (Zep)](https://github.com/getzep/graphiti)
* [Graphiti paper (arXiv:2501.13956)](https://arxiv.org/abs/2501.13956)
* [`mnemo-graph` source](https://github.com/sattyamjjain/mnemo/blob/main/crates/mnemo-graph/src/lib.rs)
* [Bitemporal walk integration tests](https://github.com/sattyamjjain/mnemo/blob/main/crates/mnemo-graph/tests/temporal_walk.rs)
