# mnemo.remember

Store a new memory record with optional metadata, tags, and relationships.

## Input Schema

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `content` | string | yes | The memory content to store |
| `agent_id` | string | no | Agent identifier (uses server default) |
| `memory_type` | string | no | `episodic`, `semantic`, `procedural`, `strategic` |
| `scope` | string | no | `private`, `shared`, `global` |
| `importance` | number | no | 0.0-1.0 importance score (default 0.5) |
| `tags` | string[] | no | Searchable tags |
| `metadata` | object | no | Arbitrary JSON metadata |
| `source_type` | string | no | `conversation`, `tool_output`, `reflection`, etc. |
| `source_id` | string | no | Reference to source (e.g., message ID) |
| `related_to` | string[] | no | UUIDs of related memories (creates graph edges) |
| `org_id` | string | no | Organization scope |
| `thread_id` | string | no | Conversation thread ID |
| `ttl_seconds` | number | no | Time-to-live in seconds |
| `decay_rate` | number | no | Custom decay rate for importance |
| `created_by` | string | no | Creator identifier |

## Response

| Field | Type | Description |
|-------|------|-------------|
| `id` | string | UUID v7 of the created memory |
| `content_hash` | string | SHA-256 hash of the content |

## Example

```json
{
  "content": "User prefers dark mode and larger fonts",
  "memory_type": "episodic",
  "importance": 0.8,
  "tags": ["preferences", "ui"]
}
```
