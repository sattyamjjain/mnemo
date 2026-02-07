# TypeScript SDK

The TypeScript SDK communicates with Mnemo via MCP STDIO, spawning the `mnemo` binary as a child process.

## Installation

```bash
npm install @mnemo/sdk
```

## Usage

```typescript
import { MnemoClient } from '@mnemo/sdk';

const client = new MnemoClient({
  dbPath: 'agent.db',
  agentId: 'my-agent',
});

await client.connect();

// Store a memory
const result = await client.remember({
  content: 'User prefers dark mode',
  importance: 0.8,
  tags: ['preferences'],
});

// Recall memories
const memories = await client.recall({
  query: 'user preferences',
  limit: 5,
});

// Share with another agent
await client.share({
  memory_id: result.id,
  target_agent_id: 'agent-2',
  permission: 'read',
});

// Verify integrity
const verification = await client.verify({
  agent_id: 'my-agent',
});
console.log(`Chain valid: ${verification.valid}`);

await client.close();
```

## API Reference

### Constructor Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `command` | string | `"mnemo"` | Path to mnemo binary |
| `dbPath` | string | `"mnemo.db"` | Database file path |
| `agentId` | string | `"default"` | Default agent ID |
| `orgId` | string | - | Organization ID |
| `openaiApiKey` | string | - | OpenAI API key for embeddings |
| `dimensions` | number | 1536 | Embedding dimensions |

### Methods

All methods return promises and correspond to MCP tools:

- `remember(input)` / `recall(input)` / `forget(input)`
- `share(input)` / `checkpoint(input)` / `branch(input)`
- `merge(input)` / `replay(input)` / `verify(input)` / `delegate(input)`
