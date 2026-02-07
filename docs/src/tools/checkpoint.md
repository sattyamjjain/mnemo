# mnemo.checkpoint

Create a named snapshot of the current agent memory state.

## Input Schema

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `agent_id` | string | no | Agent identifier |
| `label` | string | no | Human-readable label for the checkpoint |

## Response

| Field | Type | Description |
|-------|------|-------------|
| `checkpoint_id` | string | UUID of the checkpoint |
| `label` | string | The label (if provided) |
| `created_at` | string | ISO timestamp |
