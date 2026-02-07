# Quick Start

## Installation

### From source

```bash
cargo install --path crates/mnemo-cli
```

### With Docker

```bash
docker pull ghcr.io/mnemo-ai/mnemo:latest
docker run -v mnemo-data:/data ghcr.io/mnemo-ai/mnemo:latest
```

## Running

### Basic (embedded DuckDB, noop embeddings)

```bash
mnemo --db-path my-agent.db
```

### With OpenAI embeddings

```bash
export OPENAI_API_KEY=sk-...
mnemo --db-path my-agent.db
```

### With PostgreSQL backend

```bash
mnemo --postgres-url "postgres://$POSTGRES_USER:$POSTGRES_PASSWORD@localhost/mnemo"
```

### With REST API

```bash
mnemo --db-path my-agent.db --rest-port 8080
```

## Claude Desktop Integration

Add to your `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "mnemo": {
      "command": "mnemo",
      "args": ["--db-path", "/path/to/memory.db"],
      "env": {
        "OPENAI_API_KEY": "sk-..."
      }
    }
  }
}
```

## Python SDK

```bash
pip install mnemo
```

```python
from mnemo import MnemoClient

client = MnemoClient(db_path="agent.db")
result = client.remember("The user prefers dark mode")
memories = client.recall("user preferences")
```

## First Operations

Once running, the agent (or you via MCP client) can:

1. **Store a memory**: `mnemo.remember` with content and optional metadata
2. **Retrieve memories**: `mnemo.recall` with a natural language query
3. **Share with other agents**: `mnemo.share` to grant access
4. **Verify integrity**: `mnemo.verify` to check hash chain consistency
