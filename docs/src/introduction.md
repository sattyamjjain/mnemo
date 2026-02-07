# Introduction

Mnemo is an MCP-native memory database for AI agents. It provides persistent, structured memory with semantic search, access control, hash-chain verification, and multi-agent collaboration primitives.

## Key Features

- **10 MCP Tools**: remember, recall, forget, share, checkpoint, branch, merge, replay, verify, delegate
- **Hybrid Retrieval**: Vector similarity (USearch/pgvector) + BM25 full-text (Tantivy) + recency + graph signals fused via Reciprocal Rank Fusion
- **Access Control**: Owner-based permissions, ACL sharing, transitive delegation with time bounds
- **Integrity Verification**: SHA-256 hash chains over memory records with tamper detection
- **Git-like State Management**: Checkpoint, branch, merge, and replay agent memory states
- **Cognitive Forgetting**: Ebbinghaus decay curves, consolidation, archival strategies
- **Memory Poisoning Detection**: Anomaly scoring with automatic quarantine
- **Multiple Backends**: DuckDB (embedded) or PostgreSQL (distributed)
- **REST API**: Full HTTP API alongside MCP stdio transport
- **SDKs**: Python (with LangGraph, CrewAI, OpenAI Agents integrations), TypeScript, Go

## Use Cases

- **Agent Memory**: Give LLM agents persistent memory across conversations
- **Multi-Agent Collaboration**: Share memories between agents with fine-grained permissions
- **Audit Trails**: Immutable event logs with hash-chain integrity verification
- **Knowledge Management**: Store, retrieve, and organize agent-generated knowledge

## Architecture

Mnemo is built in Rust for performance and safety. The workspace contains:

| Crate | Purpose |
|-------|---------|
| `mnemo-core` | Storage, indexing, query engine, models |
| `mnemo-mcp` | MCP server (rmcp 0.14) |
| `mnemo-cli` | Binary with CLI args |
| `mnemo-postgres` | PostgreSQL storage backend |
| `mnemo-rest` | Axum REST API |
| `python/` | PyO3 Python bindings |
