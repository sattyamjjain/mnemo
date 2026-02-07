# mnemo.forget

Remove or decay memories using various strategies.

## Input Schema

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `memory_ids` | string[] | yes | UUIDs of memories to forget |
| `agent_id` | string | no | Agent identifier |
| `strategy` | string | no | Forget strategy (see below) |
| `criteria` | object | no | Filter criteria for bulk forget |

## Strategies

| Strategy | Description |
|----------|-------------|
| `soft_delete` | Mark as deleted (default, recoverable) |
| `hard_delete` | Permanently remove from storage |
| `decay` | Reduce importance using Ebbinghaus decay curve |
| `consolidate` | Merge into a semantic summary |
| `archive` | Move to cold storage |

## Criteria (for bulk forget)

| Field | Type | Description |
|-------|------|-------------|
| `max_age_hours` | number | Only forget memories older than this |
| `min_importance_below` | number | Only forget memories below this importance |
| `tags` | string[] | Only forget memories with these tags |

## Response

| Field | Type | Description |
|-------|------|-------------|
| `forgotten` | string[] | UUIDs of successfully forgotten memories |
| `errors` | array | `{ id, error }` for any failures |
