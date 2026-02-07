# Mnemo

**MCP-native memory database for AI agents.**

Mnemo (from Greek *mneme* — memory) is an embedded database whose primitives are **REMEMBER**, **RECALL**, **FORGET**, and **SHARE** — exposed as [MCP](https://modelcontextprotocol.io/) tools that any AI agent can connect to directly.

## Quickstart

### 1. Build

```bash
cargo build --release
```

### 2. Configure your AI agent

Add to your MCP client configuration (e.g. Claude Desktop, Cursor, etc.):

```json
{
  "mcpServers": {
    "mnemo": {
      "command": "./target/release/mnemo",
      "args": ["--db-path", "./agent.mnemo.db"],
      "env": {
        "OPENAI_API_KEY": "sk-..."
      }
    }
  }
}
```

### 3. Use it

Your AI agent now has persistent memory with four tools:

| Tool | Description |
|------|-------------|
| `mnemo.remember` | Store a new memory with semantic embeddings |
| `mnemo.recall` | Search memories by semantic similarity |
| `mnemo.forget` | Delete memories (soft or hard delete) |
| `mnemo.share` | Share a memory with another agent |

## Python SDK

```bash
pip install mnemo
```

```python
from mnemo import MnemoClient

client = MnemoClient(
    db_path="agent.mnemo.db",
    openai_api_key="sk-...",  # optional, enables semantic search
)

# Store a memory
result = client.remember("The user prefers dark mode", tags=["preference"])

# Search memories
memories = client.recall("user preferences", limit=5)

# Delete a memory
client.forget([result["id"]])

# Mem0-compatible aliases also available:
# client.add(), client.search(), client.delete()
```

## MCP Tool Reference

### mnemo.remember

Store a new memory. Memories are searchable by semantic similarity.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `content` | string | yes | The memory content to store |
| `memory_type` | string | no | `episodic`, `semantic`, or `procedural` (default: `episodic`) |
| `scope` | string | no | `private`, `shared`, or `public` (default: `private`) |
| `importance` | float | no | 0.0 to 1.0 (default: 0.5) |
| `tags` | string[] | no | Searchable tags |
| `metadata` | object | no | Arbitrary JSON metadata |

### mnemo.recall

Search and retrieve memories by semantic similarity.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `query` | string | yes | Natural language search query |
| `limit` | integer | no | Max results (default: 10, max: 100) |
| `memory_type` | string | no | Filter by memory type |
| `min_importance` | float | no | Minimum importance threshold |
| `tags` | string[] | no | Filter by tags |

### mnemo.forget

Delete one or more memories by ID.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `memory_ids` | string[] | yes | UUIDs of memories to delete |
| `strategy` | string | no | `soft_delete` (default) or `hard_delete` |

### mnemo.share

Share a memory with another agent.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `memory_id` | string | yes | UUID of memory to share |
| `target_agent_id` | string | yes | Agent ID to share with |
| `permission` | string | no | `read`, `write`, or `admin` (default: `read`) |

## CLI Options

```
mnemo [OPTIONS]

Options:
  --db-path <PATH>           Database file path [default: mnemo.db] [env: MNEMO_DB_PATH]
  --openai-api-key <KEY>     OpenAI API key [env: OPENAI_API_KEY]
  --embedding-model <MODEL>  Embedding model [default: text-embedding-3-small] [env: MNEMO_EMBEDDING_MODEL]
  --dimensions <DIM>         Embedding dimensions [default: 1536] [env: MNEMO_DIMENSIONS]
  --agent-id <ID>            Default agent ID [default: default] [env: MNEMO_AGENT_ID]
  --org-id <ID>              Organization ID [env: MNEMO_ORG_ID]
```

## Architecture

```
┌─────────────────────────────────────────────────┐
│                  MCP Client                      │
│           (Claude, GPT, Gemini, etc.)            │
└───────────────────┬─────────────────────────────┘
                    │ STDIO (JSON-RPC)
┌───────────────────▼─────────────────────────────┐
│              mnemo-mcp                           │
│   MnemoServer (rmcp ServerHandler)               │
│   Tools: remember, recall, forget, share         │
└───────────────────┬─────────────────────────────┘
                    │
┌───────────────────▼─────────────────────────────┐
│              mnemo-core                          │
│   MnemoEngine (query orchestrator)               │
│                                                  │
│   ┌──────────┐ ┌──────────┐ ┌───────────────┐  │
│   │ DuckDB   │ │ USearch  │ │ OpenAI        │  │
│   │ Storage  │ │ HNSW     │ │ Embeddings    │  │
│   │          │ │ Index    │ │               │  │
│   └──────────┘ └──────────┘ └───────────────┘  │
└─────────────────────────────────────────────────┘
```

- **DuckDB** — Embedded columnar storage for memory records, ACLs, and metadata
- **USearch** — HNSW-based approximate nearest neighbor index for vector search
- **OpenAI Embeddings** — Pluggable embedding provider (works without API key using noop embeddings)

## Development

```bash
# Run all tests
cargo test --all

# Run integration tests only
cargo test -p mnemo-core --test integration_test

# Build Python SDK
cd python && maturin develop

# Run examples
python examples/basic_memory.py
python examples/langgraph_demo.py
```

## License

Apache-2.0
