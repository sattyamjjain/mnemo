# mnemo.share

Grant another agent access to a memory.

## Input Schema

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `memory_id` | string | yes | UUID of memory to share |
| `target_agent_id` | string | yes | Agent to share with |
| `target_agent_ids` | string[] | no | Share with multiple agents at once |
| `agent_id` | string | no | Sharing agent (uses server default) |
| `permission` | string | no | `read`, `write`, `delete`, `share`, `delegate` (default: `read`) |
| `expires_in_hours` | number | no | ACL expiration time |

## Response

| Field | Type | Description |
|-------|------|-------------|
| `acl_id` | string | UUID of the created ACL entry |
| `shared_with` | string[] | Agents the memory was shared with |
| `status` | string | `shared` |
