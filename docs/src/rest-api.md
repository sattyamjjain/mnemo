# REST API

The REST API provides HTTP access to Mnemo, enabling non-MCP clients to interact with the memory database. Enable it with `--rest-port`:

```bash
mnemo --db-path my.db --rest-port 8080
```

All endpoints are under `/v1/`.

## Configuration

- **CORS**: controlled by `MNEMO_CORS_ORIGINS` environment variable. Defaults to `localhost:3000` and `localhost:8080`. Set to `*` for permissive mode.
- **Body limit**: 2 MB maximum request body.

## Endpoints

### Health Check

```
GET /v1/health
```

Returns `{"status": "ok"}`.

### Remember

```
POST /v1/memories
Content-Type: application/json

{
  "content": "User prefers dark mode",
  "importance": 0.8,
  "tags": ["preferences"]
}
```

Returns `{"id": "...", "content_hash": "..."}`.

### Recall

```
GET /v1/memories?query=preferences&limit=5&strategy=hybrid&min_importance=0.3
```

Query parameters:

| Parameter | Type | Description |
|-----------|------|-------------|
| `query` | string | Natural language search query (required) |
| `agent_id` | string | Filter by agent |
| `limit` | integer | Max results (default: 10, max: 100) |
| `memory_type` | string | Filter: `episodic`, `semantic`, `procedural`, `strategic` |
| `memory_types` | string | Comma-separated list of types |
| `scope` | string | Filter: `private`, `shared`, `global` |
| `min_importance` | float | Minimum importance threshold |
| `tags` | string | Comma-separated tag filter |
| `org_id` | string | Filter by organization |
| `strategy` | string | `hybrid`, `semantic`, `fulltext`, `exact`, `graph` |
| `as_of` | string | Point-in-time query (RFC 3339 timestamp) |
| `hybrid_weights` | string | Comma-separated RRF weights |
| `rrf_k` | float | RRF constant (default: 60) |

### Get Memory by ID

```
GET /v1/memories/{id}
```

### Forget

```
DELETE /v1/memories/{id}?strategy=soft_delete
```

Query parameters: `strategy` (`soft_delete`, `hard_delete`, `decay`, `consolidate`, `archive`), `agent_id`.

### Share

```
POST /v1/memories/{id}/share
Content-Type: application/json

{
  "target_agent_id": "agent-2",
  "permission": "read",
  "expires_in_hours": 24
}
```

### Checkpoint

```
POST /v1/checkpoints
Content-Type: application/json

{"label": "before-experiment"}
```

### Branch

```
POST /v1/branches
Content-Type: application/json

{"checkpoint_id": "...", "branch_name": "experiment-1"}
```

### Merge

```
POST /v1/merge
Content-Type: application/json

{"branch_name": "experiment-1"}
```

### Replay

```
POST /v1/replay
Content-Type: application/json

{"checkpoint_id": "..."}
```

### Verify

```
POST /v1/verify
Content-Type: application/json

{"agent_id": "my-agent"}
```

### Delegate

```
POST /v1/delegate
Content-Type: application/json

{
  "agent_id": "my-agent",
  "delegate_id": "agent-2",
  "permission": "read",
  "memory_ids": ["uuid-1", "uuid-2"],
  "expires_in_hours": 48
}
```

The `agent_id` field identifies the caller. The server verifies the caller has `Delegate` permission on each memory in `memory_ids` before creating the delegation.

### OTLP Ingest

```
POST /v1/ingest/otlp
Content-Type: application/json

{
  "resourceSpans": [...]
}
```

Accepts simplified OTLP JSON spans and converts them to agent events. Extracts GenAI semantic convention fields (`gen_ai.request.model`, `gen_ai.usage.input_tokens`, etc.).

Returns `{"accepted": <count>}`.

## Error Handling

Errors return appropriate HTTP status codes with generic messages:

| Status | Meaning |
|--------|---------|
| 400 | Validation error (bad input) |
| 403 | Permission denied |
| 404 | Memory not found |
| 500 | Internal error |

Error body: `{"error": "description"}`. Internal errors are logged server-side; the response contains only a generic message to prevent information leakage.
