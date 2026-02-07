# Security

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

## Hash Chain Integrity

Every memory record includes a SHA-256 hash chain:
- `content_hash = SHA256(content + agent_id + memory_type + ...)`
- `prev_hash` links to the previous record's hash
- The `verify` tool checks the entire chain for tampering

## Memory Poisoning Detection

Mnemo monitors agent behavior profiles and flags anomalous memory creation:
- Rapid creation rate (burst detection)
- Content deviation from agent baseline
- Importance score anomalies

Flagged memories are quarantined and excluded from recall results.

## Best Practices

1. Always set `OPENAI_API_KEY` via environment variable, not CLI args
2. Use time-bounded delegations with minimum required permissions
3. Regularly run `verify` to check hash chain integrity
4. Monitor quarantine events for potential poisoning attempts
5. Use PostgreSQL mode with TLS for production deployments
