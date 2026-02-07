# mnemo.branch

Create a named branch from a checkpoint for isolated memory experimentation.

## Input Schema

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `checkpoint_id` | string | yes | Base checkpoint UUID |
| `branch_name` | string | yes | Name for the branch |

## Response

| Field | Type | Description |
|-------|------|-------------|
| `branch_name` | string | The created branch name |
| `base_checkpoint` | string | The checkpoint it branched from |
| `status` | string | `branched` |
