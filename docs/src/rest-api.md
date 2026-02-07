# REST API

The REST API provides HTTP access to Mnemo, enabling non-MCP clients to interact with the memory database. Enable it with `--rest-port`:

```bash
mnemo --db-path my.db --rest-port 8080
```

All endpoints are under `/v1/`.

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

Query parameters: `query`, `agent_id`, `limit`, `memory_type`, `scope`, `min_importance`, `tags` (comma-separated), `org_id`, `strategy`.

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
  "delegate_id": "agent-2",
  "permission": "read",
  "expires_in_hours": 48
}
```

## Error Handling

Errors return appropriate HTTP status codes:

| Status | Meaning |
|--------|---------|
| 400 | Validation error (bad input) |
| 403 | Permission denied |
| 404 | Memory not found |
| 500 | Internal error |

Error body: `{"error": "description"}`.
