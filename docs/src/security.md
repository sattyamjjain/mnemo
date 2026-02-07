# Security

## Encryption

Mnemo supports AES-256-GCM at-rest encryption for memory content. Enable it by setting:

```bash
export MNEMO_ENCRYPTION_KEY=$(openssl rand -hex 32)
mnemo --encryption-key "$MNEMO_ENCRYPTION_KEY" --db-path my.db
```

Content is encrypted before storage and decrypted on recall. The encryption key must be 64 hex characters (32 bytes).

## Access Control Model

Mnemo implements a three-tier access control model:

### 1. Owner Access
The agent that created a memory has full access (read, write, delete, share, delegate).

### 2. ACL-Based Sharing
Explicit access grants via the `share` tool. Each ACL entry specifies:
- Target agent ID
- Permission level (read, write, delete, share, delegate)
- Optional expiration time

### 3. Delegation
Agents can delegate their permissions to others with:
- Scoping (all memories, by ID, or by tag)
- Maximum transitive depth
- Time bounds
- Automatic revocation on expiry

The REST `/v1/delegate` endpoint verifies the caller has `Delegate` permission on each target memory before creating the delegation.

## Hash Chain Integrity

Every memory record includes a SHA-256 hash chain:
- `content_hash = SHA256(content + agent_id + timestamp)`
- `prev_hash` links to the previous record's hash via `SHA256(content_hash + prev_content_hash)`
- The `verify` tool checks the entire chain for tampering
- Hash comparisons use constant-time operations (`subtle::ConstantTimeEq`) to prevent timing side-channels

## Memory Poisoning Detection

Mnemo monitors agent behavior profiles and flags anomalous memory creation:
- Rapid creation rate (burst detection)
- Content length deviation from agent baseline
- Importance score anomalies
- **Prompt injection patterns** — 11 common patterns detected (e.g. "ignore all previous instructions", "override system prompt")

Flagged memories are quarantined and excluded from recall results. The anomaly score threshold is 0.5; prompt injection detection alone scores +0.5.

## Input Validation

- **agent_id**: validated for length (max 256 characters) and allowed characters (alphanumeric, hyphens, underscores, dots)
- **content**: must be non-empty
- **importance**: must be between 0.0 and 1.0

## REST API Security

- **CORS**: configurable origin allowlist via `MNEMO_CORS_ORIGINS` environment variable. Defaults to localhost only (`localhost:3000`, `localhost:8080`). Set to `*` to allow all origins.
- **Body limits**: 2 MB maximum request body size to prevent denial-of-service
- **Error handling**: internal errors are logged server-side; clients receive generic "internal server error" messages

## pgwire Security

- **Authentication**: optional cleartext password authentication (configure via `PgWireConfig.password`)
- **Binding**: defaults to `127.0.0.1:5433` (localhost only)
- For production, deploy behind a TLS-terminating proxy

## Environment Variables

| Variable | Description |
|----------|-------------|
| `MNEMO_ENCRYPTION_KEY` | AES-256-GCM key (64 hex chars) |
| `MNEMO_CORS_ORIGINS` | Comma-separated allowed origins, or `*` |
| `OPENAI_API_KEY` | OpenAI API key for embeddings |

## Best Practices

1. Always set secrets via environment variables, not CLI args
2. Use time-bounded delegations with minimum required permissions
3. Regularly run `verify` to check hash chain integrity
4. Monitor quarantine events for potential poisoning attempts
5. Use PostgreSQL mode with TLS for production deployments
6. Enable encryption for sensitive data with `MNEMO_ENCRYPTION_KEY`
7. Configure `MNEMO_CORS_ORIGINS` explicitly in production
