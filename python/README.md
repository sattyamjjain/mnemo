# `mnemo-db`

PyPI distribution of [Mnemo](https://github.com/sattyamjjain/mnemo) — an MCP-native memory database for AI agents.

```bash
pip install mnemo-db
```

The package directory and import path are still `mnemo` — only the PyPI distribution name has the `-db` suffix because the unqualified `mnemo` is held by an unrelated 2021 notebook project. Your code keeps saying:

```python
from mnemo import MnemoClient
```

## Quick start

```python
from mnemo import MnemoClient

client = MnemoClient(db_path="agent.mnemo.db", agent_id="agent-1")

# Three primitives carry most workflows.
result = client.remember("The user prefers dark mode", tags=["preference"])
memories = client.recall("user preferences", limit=5)
client.forget([result["id"]])

# Mem0-compatible aliases also work:
# client.add(...), client.search(...), client.delete(...)
```

## What's in the box

| Surface | What |
| :-- | :-- |
| `MnemoClient` | Native PyO3 binding to the Rust engine — DuckDB storage, USearch HNSW vector index, Tantivy full-text index, hybrid retrieval, hash-chained audit log |
| `MnemoMemoryToolServer` | Anthropic `memory_20250818` 6-op handler. `pip install 'mnemo-db[anthropic-memory-tool]'` |
| `MnemoLettaShared` | Letta-style Conversations adapter for shared agent memory |
| `S3Workspace` / `CloudflareR2Workspace` | OpenAI Agents SDK GA snapshot store backends. `pip install 'mnemo-db[openai-sandbox-s3]'` or `[openai-sandbox-r2]` |
| `MnemoAgentMemory` (OpenAI), `Mem0Compat`, `ASMDCheckpointer` (LangGraph), 12 more | Drop-in framework integrations. Install the matching extra. |

## Optional extras

```bash
pip install 'mnemo-db[langgraph]'              # LangGraph checkpoint
pip install 'mnemo-db[crewai]'                 # CrewAI memory
pip install 'mnemo-db[openai-agents]'          # OpenAI Agents SDK
pip install 'mnemo-db[claude]'                 # Claude Agent SDK
pip install 'mnemo-db[anthropic-memory-tool]'  # memory_20250818
pip install 'mnemo-db[openai-sandbox-s3]'      # S3 workspace backend
pip install 'mnemo-db[openai-sandbox-r2]'      # Cloudflare R2 workspace backend
pip install 'mnemo-db[benchmark]'              # LoCoMo / LongMemEval harness
```

Each extra fails closed if not installed — `from mnemo import MnemoMemoryToolServer` will raise `ImportError` cleanly when `[anthropic-memory-tool]` isn't on the path.

## License + source

Source: <https://github.com/sattyamjjain/mnemo>. Apache-2.0.

Documentation: <https://github.com/sattyamjjain/mnemo#readme>.
Issues: <https://github.com/sattyamjjain/mnemo/issues>.
