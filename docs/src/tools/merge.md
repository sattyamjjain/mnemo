# mnemo.merge

Merge a branch back into the main agent memory state.

## Input Schema

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `branch_name` | string | yes | Branch to merge |
| `agent_id` | string | no | Agent identifier |

## Response

| Field | Type | Description |
|-------|------|-------------|
| `merged` | number | Count of merged records |
| `conflicts` | number | Count of conflicts detected |
| `status` | string | `merged` |
