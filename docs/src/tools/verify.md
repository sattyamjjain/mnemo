# mnemo.verify

Verify the SHA-256 hash chain integrity of memory records.

## Input Schema

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `agent_id` | string | no | Agent to verify (uses server default) |
| `thread_id` | string | no | Verify only a specific thread |

## Response

| Field | Type | Description |
|-------|------|-------------|
| `valid` | boolean | Whether the chain is intact |
| `total_records` | number | Total records checked |
| `verified_records` | number | Records that passed verification |
| `first_broken_at` | string | UUID of first broken record (if any) |
| `error_message` | string | Description of the integrity violation |
| `status` | string | `verified` or `integrity_violation` |
