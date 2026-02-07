# mnemo.delegate

Delegate permissions to another agent with optional scoping and time bounds.

## Input Schema

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `delegate_id` | string | yes | Agent to delegate to |
| `permission` | string | yes | `read`, `write`, `delete`, `share`, `delegate` |
| `memory_ids` | string[] | no | Scope to specific memories |
| `tags` | string[] | no | Scope to memories with these tags |
| `max_depth` | number | no | Maximum transitive delegation depth (default 0) |
| `expires_in_hours` | number | no | Delegation expiration time |

## Scoping

If `memory_ids` is provided, the delegation applies only to those specific memories. If `tags` is provided, it applies to memories matching those tags. If neither is provided, the delegation applies to all memories.

## Transitive Delegation

When `max_depth > 0`, the delegate can further delegate to other agents, up to the specified depth.

## Response

| Field | Type | Description |
|-------|------|-------------|
| `delegation_id` | string | UUID of the delegation |
| `status` | string | `delegated` |
