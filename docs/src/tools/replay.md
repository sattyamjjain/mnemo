# mnemo.replay

Replay events that occurred after a given checkpoint.

## Input Schema

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `checkpoint_id` | string | yes | Checkpoint to replay from |
| `agent_id` | string | no | Agent identifier |

## Response

| Field | Type | Description |
|-------|------|-------------|
| `events` | array | List of AgentEvent objects |
| `count` | number | Number of events replayed |
