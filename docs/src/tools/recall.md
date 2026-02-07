# mnemo.recall

Retrieve memories using semantic search, full-text search, exact filters, graph traversal, or hybrid retrieval.

## Input Schema

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `query` | string | yes | Search query |
| `agent_id` | string | no | Filter by agent (uses server default) |
| `limit` | number | no | Max results (default 10) |
| `memory_type` | string | no | Filter by single type |
| `memory_types` | string[] | no | Filter by multiple types |
| `scope` | string | no | Filter by scope |
| `min_importance` | number | no | Minimum importance threshold |
| `tags` | string[] | no | Filter by tags (any match) |
| `org_id` | string | no | Filter by organization |
| `strategy` | string | no | `vector`, `bm25`, `exact`, `graph`, `hybrid` (default: `hybrid`) |
| `temporal_range` | object | no | `{ after: string, before: string }` ISO timestamps |

## Strategies

- **vector**: Cosine similarity via USearch/pgvector
- **bm25**: Full-text search via Tantivy
- **exact**: Filter-only (no embeddings needed)
- **graph**: Vector seeds + 2-hop graph expansion with RRF
- **hybrid** (default): Vector + BM25 + recency + graph fused via RRF

## Response

| Field | Type | Description |
|-------|------|-------------|
| `memories` | array | Matching memories with scores |
| `total` | number | Total count of results |

Each memory includes: `id`, `agent_id`, `content`, `memory_type`, `scope`, `importance`, `tags`, `score`, `created_at`, `updated_at`.

## Example

```json
{
  "query": "user preferences",
  "strategy": "hybrid",
  "limit": 5,
  "min_importance": 0.3
}
```
