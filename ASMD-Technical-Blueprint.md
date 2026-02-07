**ASMD**

Agent State & Memory Database

  -------------------------------------

  -------------------------------------

Complete Technical Blueprint

Architecture • Data Model • APIs • Storage Engine • Execution Roadmap

February 2026

*Based on 60+ verified sources across industry research, academic
papers, and production systems*

Table of Contents

Executive Summary

The AI infrastructure landscape in 2026 faces a critical paradox: while
investment in AI exceeds \$200 billion and every enterprise deploys
agentic AI systems, the foundational data layer for these agents remains
fragmented, ad-hoc, and fundamentally broken. Vector databases are being
commoditized into features of existing databases. Agent memory is
duct-taped together from Redis, Postgres, and custom code. State
management is an afterthought. Security is an open wound.

This document presents the complete technical blueprint for ASMD (Agent
State & Memory Database), a purpose-built database whose core primitive
is not the vector or the row, but the full agent cognition lifecycle:
event, state, memory, replay, and governance. ASMD is designed to become
the default \"brain\" infrastructure for every AI agent deployed in
2026-2027 and beyond.

  -----------------------------------------------------------------------
  **THE CORE THESIS**

  **Memory is the new query.** Just as SQL was the interface for
  relational data and vector search was the interface for embeddings,
  ***memory operations*** (remember, recall, forget, share, version,
  protect) should be the native interface for AI agent data. ASMD is the
  first database designed from the ground up around this principle.
  -----------------------------------------------------------------------

+-----------------------------------------------------------------------+
| **KEY DIFFERENTIATORS**                                               |
+-----------------------------------------------------------------------+
| **1. Memory-First API:** REMEMBER, RECALL, FORGET, SHARE, BRANCH,     |
| MERGE as native database operations                                   |
|                                                                       |
| **2. MCP-Native Interface:** First database that IS an MCP server.    |
| Agents connect directly, no middleware                                |
|                                                                       |
| **3. Unified Storage Engine:** Vectors + graph edges + temporal       |
| versions + structured metadata in a single engine                     |
|                                                                       |
| **4. Agent Security Built-In:** Memory integrity verification,        |
| poisoning detection, RBAC on memories, provenance tracking            |
|                                                                       |
| **5. Cognitive Forgetting:** TTL decay, importance-weighted           |
| retention, memory consolidation as first-class primitives             |
|                                                                       |
| **6. OTel-Native Telemetry:** System-of-record for agent runs via     |
| OpenTelemetry GenAI conventions                                       |
+-----------------------------------------------------------------------+

Market Context & Opportunity

The Commoditization of Vectors

Building another vector database in 2026 is entering a red ocean. The
evidence is overwhelming and comes from every direction in the industry.
Vector search has become a feature, not a product category.

  ---------------- ---------------------- ---------------------------------
  **Vendor**       **Vector Capability    **Signal**
                   Added**                

  **Amazon S3      Native vector querying Hyperscaler treating vectors as
  Vectors**        in object storage (GA) commodity storage

  **SQL Server     VECTOR data type,      Enterprise RDBMS absorbing
  2025**           first-class ANN        vectors
                   indexing               

  **MongoDB        Vector Search with     Operational DB with native vector
  Atlas**          pre-filtering pipeline support

  **ClickHouse**   HNSW-based ANN         Analytics DB adding vector as
                   similarity indexes     feature

  **ScyllaDB**     Integrated Vector      NoSQL absorbing vectors
                   Search GA at scale     

  **Neo4j**        Vector search with     Graph DB adding vector layer
                   in-index filters       

  **Oracle 26ai**  AI Database with       Legacy enterprise rebranding
                   native vector search   around AI

  **PostgreSQL +   Open-source vector     Default stack absorbing vectors
  pgvector**       extension, ubiquitous  for free
  ---------------- ---------------------- ---------------------------------

The acquisitions confirm the consolidation: Snowflake acquired Crunchy
Data (PostgreSQL) for \$250M, Databricks acquired Neon (PostgreSQL) for
\$1B, Supabase raised \$100M at \$5B valuation. The message is clear:
vectors are being absorbed into existing database ecosystems. Pure-play
vector databases (Pinecone, Milvus, Weaviate) are fighting for survival
against incumbents who can add vector search as a feature.

The Real Gap: Agent Cognition Infrastructure

While the industry obsesses over vector search, the actual hard problem
for 2026-2027 is entirely different. Agents are long-running, stateful,
multi-step systems that need to remember, learn, collaborate, and be
audited. None of these requirements are served by existing databases.

+-----------------------------------------------------------------------+
| **THE 7 VERIFIED INFRASTRUCTURE GAPS**                                |
+-----------------------------------------------------------------------+
| **Gap 1: Agent Memory Has No Native Database.** Memory is duct-taped  |
| from Redis/Postgres. Mem0 proved demand (26% accuracy boost) but it   |
| is a library wrapping external stores, not a database with CRUD, TTL, |
| access control.                                                       |
|                                                                       |
| **Gap 2: Vector DB Is Commoditized.** Every major database now ships  |
| vector search. Building another vector DB is building a commodity.    |
|                                                                       |
| **Gap 3: Graph + Vector + Temporal Hybrid Required.** GraphRAG shows  |
| dramatic improvements for multi-hop reasoning, but current            |
| implementations require stitching 3-5 systems together. No database   |
| unifies these natively.                                               |
|                                                                       |
| **Gap 4: Temporal/Versioned State Does Not Exist.** Agents create     |
| millions of ephemeral schemas. They need database branching (git for  |
| data), copy-on-write, and scale-to-zero economics. LangGraph          |
| checkpoints are primitive.                                            |
|                                                                       |
| **Gap 5: Multi-Agent Memory Sharing and Access Control.** Multi-agent |
| systems use 15x more tokens than single chat. No database implements  |
| private, shared, and delegated memory scopes.                         |
|                                                                       |
| **Gap 6: Memory Security Is an Open Wound.** OWASP has dedicated      |
| categories for memory poisoning and privilege creep. If agent memory  |
| is wrong, the agent is confidently wrong. No database has built-in    |
| defenses.                                                             |
|                                                                       |
| **Gap 7: No MCP-Native Database Exists.** MCP is the de facto         |
| standard (Anthropic, OpenAI, Google, Microsoft, Linux Foundation).    |
| There are thousands of MCP servers for tools but zero MCP-native      |
| databases.                                                            |
+-----------------------------------------------------------------------+

Market Size & Timing

Global AI investment exceeded \$202.3 billion in 2025, representing 50%
of all venture capital. OpenAI\'s valuation trajectory moved from \$157B
to targeting \$830B in 14 months. Anthropic grew from \$87M to \$7B
annualized revenue in under two years. Forrester predicts most
organizations will have vector databases in production by 2026.
VentureBeat\'s January 2026 prediction: contextual memory becomes table
stakes for operational agentic AI. The infrastructure wave is cresting
now, and whoever builds the purpose-built agent memory database captures
a generational opportunity.

System Architecture

ASMD is designed as a layered architecture where each layer has clear
responsibilities and well-defined interfaces. The architecture
prioritizes native integration over bolt-on features. Every capability
described here is built into the core engine, not added as an extension
or adapter.

Architecture Overview

+-----------------------------------------------------------------------+
| ┌─────────────────────────────────────────────────────────────┐       |
|                                                                       |
| │ ASMD ARCHITECTURE │                                                 |
|                                                                       |
| ├─────────────────────────────────────────────────────────────┤       |
|                                                                       |
| │ LAYER 1: INTERFACE │                                                |
|                                                                       |
| │ ├─ MCP Server (native, primary interface) │                         |
|                                                                       |
| │ ├─ OTel Ingestion (GenAI semantic conventions) │                    |
|                                                                       |
| │ ├─ PostgreSQL Wire Protocol (compatibility) │                       |
|                                                                       |
| │ ├─ REST / gRPC API │                                                |
|                                                                       |
| │ └─ SDKs (Python, TypeScript, Rust, Go) │                            |
|                                                                       |
| ├─────────────────────────────────────────────────────────────┤       |
|                                                                       |
| │ LAYER 2: QUERY ENGINE │                                             |
|                                                                       |
| │ ├─ Memory Operations (REMEMBER/RECALL/FORGET/SHARE) │               |
|                                                                       |
| │ ├─ Hybrid Retrieval (RRF: BM25 + vector + graph) │                  |
|                                                                       |
| │ ├─ Permission-Safe ANN (in-index filtering) │                       |
|                                                                       |
| │ ├─ Replay Engine (exact context reconstruction) │                   |
|                                                                       |
| │ ├─ Causal Debugger (input → output tracing) │                       |
|                                                                       |
| │ └─ Temporal Queries (point-in-time, branches) │                     |
|                                                                       |
| ├─────────────────────────────────────────────────────────────┤       |
|                                                                       |
| │ LAYER 3: MEMORY MODEL │                                             |
|                                                                       |
| │ ├─ Working Memory (session-scoped, ephemeral, fast) │               |
|                                                                       |
| │ ├─ Episodic Memory (timestamped interaction records) │              |
|                                                                       |
| │ ├─ Semantic Memory (facts, knowledge graph) │                       |
|                                                                       |
| │ ├─ Procedural Memory (learned patterns, tool recipes) │             |
|                                                                       |
| │ └─ Scopes: private \| shared \| global \| delegated │               |
|                                                                       |
| ├─────────────────────────────────────────────────────────────┤       |
|                                                                       |
| │ LAYER 4: SECURITY & GOVERNANCE │                                    |
|                                                                       |
| │ ├─ Memory Integrity (hash chains on all writes) │                   |
|                                                                       |
| │ ├─ Poisoning Detection (anomaly detection on mutations) │           |
|                                                                       |
| │ ├─ Provenance Tracking (full lineage per memory) │                  |
|                                                                       |
| │ ├─ RBAC on Memory (agent/user/org/delegation scopes) │              |
|                                                                       |
| │ └─ Immutable Audit Log (OTel-compatible) │                          |
|                                                                       |
| ├─────────────────────────────────────────────────────────────┤       |
|                                                                       |
| │ LAYER 5: LIFECYCLE ENGINE │                                         |
|                                                                       |
| │ ├─ Cognitive Forgetting (TTL, decay, consolidation) │               |
|                                                                       |
| │ ├─ Memory Consolidation (episodic → semantic promotion) │           |
|                                                                       |
| │ ├─ Conflict Resolution (new vs old, merge strategies) │             |
|                                                                       |
| │ ├─ Checkpointing (snapshot + diff, branch / merge) │                |
|                                                                       |
| │ └─ Scale-to-Zero (pay per operation, not per instance) │            |
|                                                                       |
| ├─────────────────────────────────────────────────────────────┤       |
|                                                                       |
| │ LAYER 6: STORAGE ENGINE │                                           |
|                                                                       |
| │ ├─ Embedded Mode: DuckDB-based (edge/local/sandbox) │               |
|                                                                       |
| │ ├─ Distributed Mode: RocksDB/Pebble LSM (cloud) │                   |
|                                                                       |
| │ ├─ Cold Tier: S3-compatible (archived memories) │                   |
|                                                                       |
| │ └─ Sync Engine: local ↔ cloud (CRDTs or log shipping) │             |
|                                                                       |
| └─────────────────────────────────────────────────────────────┘       |
+-----------------------------------------------------------------------+

Layer 1: Interface Layer

The interface layer is what agents and developers interact with. ASMD
exposes five distinct interfaces, each optimized for a different use
case. The critical design decision is that MCP is the primary interface,
not a secondary adapter. This is the single most important
differentiator.

MCP Server (Primary Interface)

ASMD is an MCP server from day one. This means any agent that speaks MCP
(which now includes agents built on Anthropic, OpenAI, Google, and
Microsoft platforms) can connect to ASMD directly and perform memory
operations as MCP tool calls. No middleware, no LangChain glue, no
adapter layers.

The MCP server exposes the following tool categories:

+-----------------------------------------------------------------------+
| \# MCP Tool: asmd.remember                                            |
|                                                                       |
| \# Stores a memory with full metadata                                 |
|                                                                       |
| {                                                                     |
|                                                                       |
| \"name\": \"asmd.remember\",                                          |
|                                                                       |
| \"description\": \"Store a memory in the agent database\",            |
|                                                                       |
| \"inputSchema\": {                                                    |
|                                                                       |
| \"type\": \"object\",                                                 |
|                                                                       |
| \"properties\": {                                                     |
|                                                                       |
| \"content\": { \"type\": \"string\", \"description\": \"The memory    |
| content\" },                                                          |
|                                                                       |
| \"memory_type\": { \"enum\":                                          |
| \[\"episodic\",\"semantic\",\"procedural\",\"working\"\] },           |
|                                                                       |
| \"scope\": { \"enum\": \[\"private\",\"shared\",\"global\"\] },       |
|                                                                       |
| \"importance\": { \"type\": \"number\", \"min\": 0, \"max\": 1.0 },   |
|                                                                       |
| \"ttl_seconds\": { \"type\": \"integer\", \"description\": \"Time to  |
| live\" },                                                             |
|                                                                       |
| \"tags\": { \"type\": \"array\", \"items\": { \"type\": \"string\" }  |
| },                                                                    |
|                                                                       |
| \"related_to\": { \"type\": \"array\", \"description\": \"Memory IDs  |
| to link\" }                                                           |
|                                                                       |
| },                                                                    |
|                                                                       |
| \"required\": \[\"content\"\]                                         |
|                                                                       |
| }                                                                     |
|                                                                       |
| }                                                                     |
+-----------------------------------------------------------------------+

+-----------------------------------------------------------------------+
| \# MCP Tool: asmd.recall                                              |
|                                                                       |
| \# Retrieves memories with hybrid search                              |
|                                                                       |
| {                                                                     |
|                                                                       |
| \"name\": \"asmd.recall\",                                            |
|                                                                       |
| \"inputSchema\": {                                                    |
|                                                                       |
| \"properties\": {                                                     |
|                                                                       |
| \"query\": { \"type\": \"string\" },                                  |
|                                                                       |
| \"memory_types\": { \"type\": \"array\" },                            |
|                                                                       |
| \"temporal_range\": { \"type\": \"object\",                           |
|                                                                       |
| \"properties\": {                                                     |
|                                                                       |
| \"after\": { \"type\": \"string\", \"format\": \"datetime\" },        |
|                                                                       |
| \"before\": { \"type\": \"string\", \"format\": \"datetime\" }        |
|                                                                       |
| }                                                                     |
|                                                                       |
| },                                                                    |
|                                                                       |
| \"scope\": { \"enum\": \[\"private\",\"shared\",\"global\",\"all\"\]  |
| },                                                                    |
|                                                                       |
| \"strategy\": { \"enum\":                                             |
| \[\"semantic\",\"hybrid\",\"graph\",\"exact\",\"auto\"\] },           |
|                                                                       |
| \"max_results\": { \"type\": \"integer\", \"default\": 10 },          |
|                                                                       |
| \"min_importance\": { \"type\": \"number\" }                          |
|                                                                       |
| }                                                                     |
|                                                                       |
| }                                                                     |
|                                                                       |
| }                                                                     |
+-----------------------------------------------------------------------+

+---------------------------------------------------------------------------+
| \# MCP Tool: asmd.forget                                                  |
|                                                                           |
| \# Cognitive forgetting with configurable strategies                      |
|                                                                           |
| {                                                                         |
|                                                                           |
| \"name\": \"asmd.forget\",                                                |
|                                                                           |
| \"inputSchema\": {                                                        |
|                                                                           |
| \"properties\": {                                                         |
|                                                                           |
| \"memory_ids\": { \"type\": \"array\", \"description\": \"Specific        |
| memories\" },                                                             |
|                                                                           |
| \"criteria\": { \"type\": \"object\", \"description\": \"Filter-based     |
| forget\" },                                                               |
|                                                                           |
| \"strategy\": {                                                           |
|                                                                           |
| \"enum\":                                                                 |
| \[\"hard_delete\",\"soft_delete\",\"decay\",\"consolidate\",\"archive\"\] |
|                                                                           |
| }                                                                         |
|                                                                           |
| }                                                                         |
|                                                                           |
| }                                                                         |
|                                                                           |
| }                                                                         |
+---------------------------------------------------------------------------+

+-----------------------------------------------------------------------+
| \# MCP Tool: asmd.share                                               |
|                                                                       |
| \# Share memories between agents with access control                  |
|                                                                       |
| {                                                                     |
|                                                                       |
| \"name\": \"asmd.share\",                                             |
|                                                                       |
| \"inputSchema\": {                                                    |
|                                                                       |
| \"properties\": {                                                     |
|                                                                       |
| \"memory_ids\": { \"type\": \"array\" },                              |
|                                                                       |
| \"target_agents\": { \"type\": \"array\" },                           |
|                                                                       |
| \"permissions\": { \"enum\": \[\"read\",\"read_write\",\"delegate\"\] |
| },                                                                    |
|                                                                       |
| \"expires_at\": { \"type\": \"string\", \"format\": \"datetime\" }    |
|                                                                       |
| }                                                                     |
|                                                                       |
| }                                                                     |
|                                                                       |
| }                                                                     |
+-----------------------------------------------------------------------+

+-----------------------------------------------------------------------+
| \# MCP Tool: asmd.checkpoint / asmd.branch / asmd.replay              |
|                                                                       |
| \# Git-like state management for agent sessions                       |
|                                                                       |
| {                                                                     |
|                                                                       |
| \"name\": \"asmd.checkpoint\",                                        |
|                                                                       |
| \"inputSchema\": {                                                    |
|                                                                       |
| \"properties\": {                                                     |
|                                                                       |
| \"thread_id\": { \"type\": \"string\" },                              |
|                                                                       |
| \"label\": { \"type\": \"string\" },                                  |
|                                                                       |
| \"metadata\": { \"type\": \"object\" }                                |
|                                                                       |
| }                                                                     |
|                                                                       |
| }                                                                     |
|                                                                       |
| }                                                                     |
|                                                                       |
| // asmd.branch - Fork agent state for exploration                     |
|                                                                       |
| // asmd.merge - Merge branch back into main state                     |
|                                                                       |
| // asmd.replay - Reconstruct exact context at checkpoint N            |
+-----------------------------------------------------------------------+

OTel-Native Ingestion

ASMD accepts OpenTelemetry GenAI semantic conventions as a first-class
ingestion format. This means every agent framework that emits OTel
traces (which is rapidly becoming all of them via OpenLLMetry and
similar projects) can pipe their telemetry directly into ASMD. The
database becomes the system-of-record for agent runs: every message,
tool call, tool output, retrieval hit, model response, latency, token
usage, cost, and decision. This is the \"Datadog for agent cognition\"
play.

PostgreSQL Wire Protocol

ASMD speaks the PostgreSQL wire protocol for compatibility. This means
existing tooling, ORMs, database GUIs, and developer muscle memory all
work. You can connect with psql, use SQLAlchemy, use Prisma. But
underneath, the storage engine is ASMD-native, not PostgreSQL. This is
the same strategy that made CockroachDB, Neon, and Supabase successful:
meet developers where they are, then gradually reveal the unique
capabilities.

REST/gRPC API & SDKs

For direct programmatic access, ASMD exposes a REST API and gRPC API
with SDKs in Python, TypeScript, Rust, and Go. The Python SDK is the
priority (most agent frameworks are Python). The SDK provides a
high-level client that wraps MCP tool calls for non-MCP environments.

Data Model

The data model is the heart of ASMD. Every decision here affects query
performance, memory management, and developer experience. The model is
designed around the principle that agent data has fundamentally
different access patterns than traditional application data.

Core Entities

Memory Record

The memory record is the fundamental unit of storage. Every piece of
information an agent remembers, learns, or receives is stored as a
memory record with rich metadata.

+-----------------------------------------------------------------------+
| MemoryRecord {                                                        |
|                                                                       |
| // Identity                                                           |
|                                                                       |
| id: UUID                                                              |
|                                                                       |
| agent_id: UUID // Owner agent                                         |
|                                                                       |
| org_id: UUID // Tenant isolation                                      |
|                                                                       |
| thread_id: UUID? // Session context (nullable)                        |
|                                                                       |
| // Content (unified multi-modal)                                      |
|                                                                       |
| content: TEXT // Raw content                                          |
|                                                                       |
| embedding: VECTOR(dims) // Semantic embedding                         |
|                                                                       |
| content_hash: BYTES // Integrity verification                         |
|                                                                       |
| // Classification                                                     |
|                                                                       |
| memory_type: ENUM {                                                   |
|                                                                       |
| working, // Current session, ephemeral, fast                          |
|                                                                       |
| episodic, // Past interactions, timestamped                           |
|                                                                       |
| semantic, // Extracted facts, deduplicated, graph-linked              |
|                                                                       |
| procedural // Learned patterns, tool recipes, workflows               |
|                                                                       |
| }                                                                     |
|                                                                       |
| // Lifecycle                                                          |
|                                                                       |
| importance: FLOAT \[0,1\] // Importance score                         |
|                                                                       |
| access_count: INT // Retrieval count (for decay)                      |
|                                                                       |
| last_accessed: TIMESTAMP // Recency tracking                          |
|                                                                       |
| ttl: DURATION? // Time-to-live (null = permanent)                     |
|                                                                       |
| decay_rate: FLOAT // Forgetting curve parameter                       |
|                                                                       |
| consolidation_state: ENUM {                                           |
|                                                                       |
| active, // In use, not yet consolidated                               |
|                                                                       |
| pending, // Marked for consolidation                                  |
|                                                                       |
| consolidated, // Merged into semantic memory                          |
|                                                                       |
| archived, // Cold storage                                             |
|                                                                       |
| forgotten // Soft-deleted, recoverable                                |
|                                                                       |
| }                                                                     |
|                                                                       |
| // Access Control                                                     |
|                                                                       |
| scope: ENUM { private, shared, global }                               |
|                                                                       |
| acl: ACL\[\] // Agent-level permissions                               |
|                                                                       |
| // Provenance                                                         |
|                                                                       |
| created_at: TIMESTAMP                                                 |
|                                                                       |
| created_by: UUID // Agent or user who created                         |
|                                                                       |
| source_type: ENUM {                                                   |
|                                                                       |
| user_input, tool_output, model_response,                              |
|                                                                       |
| retrieval, consolidation, import                                      |
|                                                                       |
| }                                                                     |
|                                                                       |
| source_ref: UUID? // Link to source event                             |
|                                                                       |
| version: INT // For optimistic concurrency                            |
|                                                                       |
| prev_version_id: UUID? // Version chain                               |
|                                                                       |
| // Graph Links (stored in graph index)                                |
|                                                                       |
| relations: Relation\[\] // Typed edges to other memories              |
|                                                                       |
| // Metadata                                                           |
|                                                                       |
| tags: TEXT\[\]                                                        |
|                                                                       |
| metadata: JSONB // Extensible key-value                               |
|                                                                       |
| }                                                                     |
+-----------------------------------------------------------------------+

Agent Event

The agent event is the immutable truth source. Every interaction an
agent has is recorded as an event. Events are append-only, never
modified, and form the basis for replay and audit.

+-----------------------------------------------------------------------+
| AgentEvent {                                                          |
|                                                                       |
| id: UUID                                                              |
|                                                                       |
| agent_id: UUID                                                        |
|                                                                       |
| thread_id: UUID                                                       |
|                                                                       |
| run_id: UUID // Execution run identifier                              |
|                                                                       |
| parent_event_id: UUID? // Causal chain                                |
|                                                                       |
| // Event Classification                                               |
|                                                                       |
| event_type: ENUM {                                                    |
|                                                                       |
| user_message, assistant_message, tool_call,                           |
|                                                                       |
| tool_result, retrieval_query, retrieval_result,                       |
|                                                                       |
| memory_write, memory_read, checkpoint,                                |
|                                                                       |
| branch, merge, error, decision                                        |
|                                                                       |
| }                                                                     |
|                                                                       |
| // Content                                                            |
|                                                                       |
| payload: JSONB // Event-specific data                                 |
|                                                                       |
| embedding: VECTOR? // Optional semantic index                         |
|                                                                       |
| // Telemetry (OTel-compatible)                                        |
|                                                                       |
| trace_id: TEXT // OTel trace ID                                       |
|                                                                       |
| span_id: TEXT // OTel span ID                                         |
|                                                                       |
| model: TEXT? // LLM model used                                        |
|                                                                       |
| tokens_input: INT?                                                    |
|                                                                       |
| tokens_output: INT?                                                   |
|                                                                       |
| latency_ms: INT?                                                      |
|                                                                       |
| cost_usd: FLOAT?                                                      |
|                                                                       |
| // Temporal                                                           |
|                                                                       |
| timestamp: TIMESTAMP // Wall clock time                               |
|                                                                       |
| logical_clock: INT // Lamport timestamp for ordering                  |
|                                                                       |
| // Integrity                                                          |
|                                                                       |
| content_hash: BYTES                                                   |
|                                                                       |
| prev_hash: BYTES // Hash chain (tamper detection)                     |
|                                                                       |
| }                                                                     |
+-----------------------------------------------------------------------+

Checkpoint

Checkpoints capture the complete state of an agent at a point in time.
They enable resume, branch, and replay operations. The design follows a
snapshot-plus-diff model to minimize storage while enabling fast
restoration.

+-----------------------------------------------------------------------+
| Checkpoint {                                                          |
|                                                                       |
| id: UUID                                                              |
|                                                                       |
| thread_id: UUID                                                       |
|                                                                       |
| agent_id: UUID                                                        |
|                                                                       |
| parent_id: UUID? // Previous checkpoint (DAG)                         |
|                                                                       |
| branch_name: TEXT // \'main\', \'exploration-1\', etc.                |
|                                                                       |
| // State                                                              |
|                                                                       |
| state_snapshot: JSONB // Full agent state at this point               |
|                                                                       |
| state_diff: JSONB? // Delta from parent (for efficiency)              |
|                                                                       |
| memory_refs: UUID\[\] // Active memory IDs at this point              |
|                                                                       |
| event_cursor: UUID // Last event included                             |
|                                                                       |
| // Metadata                                                           |
|                                                                       |
| label: TEXT? // Human-readable label                                  |
|                                                                       |
| created_at: TIMESTAMP                                                 |
|                                                                       |
| metadata: JSONB                                                       |
|                                                                       |
| }                                                                     |
+-----------------------------------------------------------------------+

Relation (Graph Edges)

Relations are typed, directed edges between memory records. They form
the knowledge graph that enables multi-hop reasoning. Relations are
stored natively in the storage engine, not in a separate graph database.

+-----------------------------------------------------------------------+
| Relation {                                                            |
|                                                                       |
| id: UUID                                                              |
|                                                                       |
| source_id: UUID // From memory                                        |
|                                                                       |
| target_id: UUID // To memory                                          |
|                                                                       |
| relation_type: TEXT // \'causes\', \'contradicts\', \'supports\',     |
|                                                                       |
| // \'derived_from\', \'related_to\', etc.                             |
|                                                                       |
| weight: FLOAT // Strength of relationship                             |
|                                                                       |
| created_at: TIMESTAMP                                                 |
|                                                                       |
| metadata: JSONB                                                       |
|                                                                       |
| }                                                                     |
+-----------------------------------------------------------------------+

Access Control List

+-----------------------------------------------------------------------+
| ACL {                                                                 |
|                                                                       |
| memory_id: UUID                                                       |
|                                                                       |
| principal_type: ENUM { agent, user, org, role }                       |
|                                                                       |
| principal_id: UUID                                                    |
|                                                                       |
| permission: ENUM { read, write, delete, share, delegate }             |
|                                                                       |
| granted_by: UUID                                                      |
|                                                                       |
| expires_at: TIMESTAMP?                                                |
|                                                                       |
| }                                                                     |
+-----------------------------------------------------------------------+

Memory Scoping Model

Multi-agent memory coordination is a first-class concern in ASMD. The
scoping model defines four levels of memory visibility, each with
distinct access patterns and isolation guarantees.

  --------------- ---------------- ----------------------- ---------------------
  **Scope**       **Visibility**   **Use Case**            **Access Pattern**

  **Private**     Only the owning  Personal user           Fast, no contention,
                  agent            preferences, session    local to agent
                                   context, internal       
                                   reasoning               

  **Shared**      Explicitly named Team/swarm              Requires ACL check,
                  agents           collaboration, hand-off supports delegation
                                   context between agents  chains

  **Global**      All agents in an Company policies,       Read-heavy,
                  org              product knowledge base, write-restricted,
                                   shared facts            cached aggressively

  **Delegated**   Temporarily      Agent A grants Agent B  Time-bounded,
                  granted          read access to specific revocable, audited
                                   memories for a task     
  --------------- ---------------- ----------------------- ---------------------

Query Engine

The query engine is what makes ASMD \"AI-native\" rather than just
another database with vector support. It provides six distinct query
modes, each optimized for a different agent data access pattern.

Hybrid Retrieval with RRF

Reciprocal Rank Fusion (RRF) is a first-class query operator. Every
RECALL operation can combine multiple retrieval strategies in a single
query plan.

+-----------------------------------------------------------------------+
| \# Single query that fuses multiple retrieval strategies              |
|                                                                       |
| results = asmd.recall(                                                |
|                                                                       |
| query = \'What does the client prefer for delivery?\',                |
|                                                                       |
| strategy = \'hybrid\',                                                |
|                                                                       |
| hybrid_config = {                                                     |
|                                                                       |
| \'semantic\': { weight: 0.4, model: \'default\' }, \# Vector          |
| similarity                                                            |
|                                                                       |
| \'lexical\': { weight: 0.2, analyzer: \'bm25\' }, \# Keyword match    |
|                                                                       |
| \'graph\': { weight: 0.25, hops: 2 }, \# Graph traversal              |
|                                                                       |
| \'recency\': { weight: 0.15, decay: \'exponential\' } \# Time decay   |
|                                                                       |
| },                                                                    |
|                                                                       |
| fusion = \'rrf\', \# Reciprocal Rank Fusion                           |
|                                                                       |
| filters = {                                                           |
|                                                                       |
| \'scope\': \[\'private\', \'shared\'\],                               |
|                                                                       |
| \'memory_type\': \[\'semantic\', \'episodic\'\],                      |
|                                                                       |
| \'min_importance\': 0.3                                               |
|                                                                       |
| },                                                                    |
|                                                                       |
| max_results = 10                                                      |
|                                                                       |
| )                                                                     |
+-----------------------------------------------------------------------+

The key architectural decision is that filters are applied inside the
ANN index, not as a post-filter. This follows the direction pioneered by
Neo4j (filters inside the index) and MongoDB (pre-filtering pipeline).
Post-filtering is fundamentally broken for multi-tenant workloads
because it overfetches and discards, creating unpredictable latency and
cost.

Replay Queries

Replay is a first-class query type. Given a run ID or checkpoint ID,
ASMD can reconstruct the exact context the agent had at any point in
time. This is critical for audit, debugging, and reproducibility.

+-----------------------------------------------------------------------+
| \# Reconstruct exact agent context at a specific point                |
|                                                                       |
| context = asmd.replay(                                                |
|                                                                       |
| thread_id = \'thread_abc123\',                                        |
|                                                                       |
| checkpoint_id = \'cp_456\', \# Or: at_event=\'evt_789\'               |
|                                                                       |
| include = \[\'messages\', \'tool_calls\', \'memories_accessed\',      |
|                                                                       |
| \'retrieval_results\', \'state_snapshot\'\]                           |
|                                                                       |
| )                                                                     |
|                                                                       |
| \# Returns the complete context: what the agent saw, what it          |
| retrieved,                                                            |
|                                                                       |
| \# what memories it read, what tools it called, and what state it was |
| in.                                                                   |
|                                                                       |
| \# Every field is immutable and hash-verified.                        |
+-----------------------------------------------------------------------+

Causal Debugging

Causal debugging answers the question: \"which specific input caused
this specific output?\" This is built on the event DAG (directed acyclic
graph) where every event has a parent_event_id linking cause to effect.

+-----------------------------------------------------------------------+
| \# Trace causal chain: what led to this decision?                     |
|                                                                       |
| chain = asmd.trace_causality(                                         |
|                                                                       |
| event_id = \'evt_final_response\',                                    |
|                                                                       |
| depth = 10,                                                           |
|                                                                       |
| include_tool_calls = True,                                            |
|                                                                       |
| include_retrievals = True                                             |
|                                                                       |
| )                                                                     |
|                                                                       |
| \# Returns: \[evt_user_msg\] -\> \[evt_retrieval\] -\>                |
| \[evt_tool_call\]                                                     |
|                                                                       |
| \# -\> \[evt_tool_result\] -\> \[evt_memory_read\] -\>                |
| \[evt_response\]                                                      |
|                                                                       |
| \# Each link shows what data flowed between events.                   |
+-----------------------------------------------------------------------+

Temporal Queries

ASMD supports point-in-time queries and branch-based time-travel. This
is inspired by LanceDB\'s table versioning and the git-like branching
pattern identified in the research.

+-----------------------------------------------------------------------+
| \# Point-in-time: what did the agent know at timestamp X?             |
|                                                                       |
| memories = asmd.recall(                                               |
|                                                                       |
| query = \'client preferences\',                                       |
|                                                                       |
| as_of = \'2026-01-15T10:30:00Z\' \# Time-travel                       |
|                                                                       |
| )                                                                     |
|                                                                       |
| \# Branch: fork state for exploration                                 |
|                                                                       |
| branch_id = asmd.branch(                                              |
|                                                                       |
| thread_id = \'thread_abc\',                                           |
|                                                                       |
| branch_name = \'what-if-scenario-1\'                                  |
|                                                                       |
| )                                                                     |
|                                                                       |
| \# Agent operates on the branch (writes don\'t affect main)           |
|                                                                       |
| asmd.remember(content=\'experimental insight\', thread_id=branch_id)  |
|                                                                       |
| \# Merge branch results back if valuable                              |
|                                                                       |
| asmd.merge(                                                           |
|                                                                       |
| source_branch = \'what-if-scenario-1\',                               |
|                                                                       |
| target_branch = \'main\',                                             |
|                                                                       |
| strategy = \'cherry_pick\', \# or \'full_merge\', \'squash\'          |
|                                                                       |
| memory_ids = \[\'mem_useful_1\', \'mem_useful_2\'\]                   |
|                                                                       |
| )                                                                     |
+-----------------------------------------------------------------------+

Storage Engine

The storage engine is where the hard engineering happens. ASMD operates
in two modes: embedded (for edge, local, sandbox use) and distributed
(for cloud production). Both modes share the same API and data model,
differing only in the underlying storage backend. The design follows the
principle that you should not build a storage engine from scratch unless
you absolutely have to.

Storage Architecture Decision

+-----------------------------------------------------------------------+
| **CRITICAL DESIGN DECISION: BUILD ON PROVEN FOUNDATIONS**             |
+-----------------------------------------------------------------------+
| Building a storage engine from scratch is a 3-5 year endeavor that    |
| has killed many startups. ASMD uses a layered approach: build the     |
| memory/query/lifecycle layers as the product differentiator, and use  |
| proven storage engines underneath. The value is in the memory model   |
| and agent-native interface, not in reinventing B-trees.               |
|                                                                       |
| **Phase 1 (Months 1-6):** Use SQLite/DuckDB (embedded) and PostgreSQL |
| (distributed) as storage backends. Ship the product.                  |
|                                                                       |
| **Phase 2 (Months 6-12):** Replace hot paths with custom storage      |
| where benchmarks show bottlenecks. Likely: custom vector index,       |
| custom event log.                                                     |
|                                                                       |
| **Phase 3 (Year 2+):** Consider full custom storage engine            |
| (RocksDB/Pebble-based LSM) only after achieving PMF and having the    |
| engineering resources to justify it.                                  |
+-----------------------------------------------------------------------+

Embedded Mode (DuckDB-based)

The embedded mode targets agent sandboxes, developer laptops, edge
devices, and single-agent deployments. It runs in-process with zero
external dependencies.

  --------------- -------------------------- -----------------------------
  **Component**   **Technology**             **Rationale**

  **Core          DuckDB (embedded,          Zero-dependency, fast
  Storage**       columnar)                  analytics on event logs,
                                             growing ecosystem

  **Vector        usearch or hnswlib         Lightweight, fast HNSW
  Index**         (in-process)               implementation, no external
                                             service

  **Graph Index** In-memory adjacency        Simple, fast for small-medium
                  lists + DuckDB join tables graphs, no Neo4j dependency

  **Full-Text**   DuckDB FTS extension or    BM25 scoring for hybrid
                  tantivy (Rust)             retrieval

  **File Format** Single .asmd file          Portable, easy to
                  (SQLite-style)             copy/backup/version-control
  --------------- -------------------------- -----------------------------

Distributed Mode (Cloud)

The distributed mode targets production multi-agent deployments,
enterprise workloads, and managed cloud service.

  --------------- ---------------------------- ---------------------------
  **Component**   **Technology**               **Rationale**

  **Event Log**   PostgreSQL (Phase 1) /       Proven durability, ACID,
                  Custom LSM (Phase 3)         replication. Upgrade path
                                               to custom.

  **Vector        pgvector (Phase 1) /         Start proven, upgrade to
  Index**         DiskANN-style (Phase 3)      disk-friendly ANN for cost
                                               efficiency

  **Graph Index** PostgreSQL recursive CTEs +  Surprisingly performant for
                  adjacency (P1) / Custom (P3) 2-3 hop queries at moderate
                                               scale

  **Checkpoint    PostgreSQL JSONB + diff      Snapshot + diff model,
  Store**         compression                  efficient for branching

  **Cold          S3-compatible object storage Archived memories, cold
  Storage**                                    vectors, cost-optimized

  **Cache Layer** Redis / DragonflyDB          Working memory cache, hot
                                               checkpoint state

  **Message       NATS or Redis Streams        Event distribution,
  Queue**                                      real-time sync, OTel
                                               ingestion
  --------------- ---------------------------- ---------------------------

Unified Page Structure

The key innovation in ASMD\'s storage design is co-locating related data
types on the same storage pages. When a memory record is written, its
text content, embedding vector, graph edges, and metadata are stored
together, not scattered across separate indexes. This means a single
memory retrieval requires one I/O operation instead of four. The design
uses a variant of the PAX (Partition Attributes Across) page layout.

+-----------------------------------------------------------------------+
| \# Conceptual page layout for a memory record                         |
|                                                                       |
| Page {                                                                |
|                                                                       |
| header: page_id, page_type, record_count, free_space                  |
|                                                                       |
| slot_array: \[offset_1, offset_2, \...\]                              |
|                                                                       |
| record_1: {                                                           |
|                                                                       |
| fixed_fields: id, agent_id, memory_type, importance, timestamps       |
|                                                                       |
| var_fields: content (inline if \<2KB, else pointer to overflow)       |
|                                                                       |
| vector_field: embedding (fixed-size, aligned for SIMD)                |
|                                                                       |
| graph_field: \[edge_type, target_id, weight\] x N (inline adjacency)  |
|                                                                       |
| metadata: JSONB (inline if \<512B, else pointer)                      |
|                                                                       |
| }                                                                     |
|                                                                       |
| }                                                                     |
+-----------------------------------------------------------------------+

This co-location strategy eliminates the join penalty that plagues
bolt-on architectures. When Neo4j does a vector search and then
traverses a graph edge, it crosses two different storage systems. When
ASMD does the same operation, it reads from the same page. At scale
(millions of memories, thousands of concurrent agents), this difference
is the moat.

Sync Engine: Local to Cloud

ASMD supports bidirectional sync between embedded and distributed modes.
This enables offline-first operation for edge devices and agent
sandboxes, with eventual consistency to the cloud.

The sync protocol uses a log-shipping approach: the embedded node
maintains a local write-ahead log (WAL) of all mutations. When
connectivity is available, the WAL is shipped to the cloud node.
Conflicts are resolved using last-writer-wins for memory metadata and
union-merge for graph edges. For critical use cases, ASMD supports
explicit conflict resolution callbacks.

Security & Governance

Agent memory security is not a feature; it is a survival requirement.
OWASP now has dedicated threat categories for agentic AI: memory
poisoning, privilege creep, and tool misuse. If an agent\'s memory is
compromised, the agent is consistently and confidently wrong. ASMD
builds security into every layer.

Memory Integrity Verification

Every memory write is integrity-protected using a hash chain. When a
memory is created, its content is hashed. When it is modified, the new
hash includes the previous hash, creating a tamper-evident chain
(similar to a blockchain but without consensus overhead). This means any
unauthorized modification of memory is detectable.

+-----------------------------------------------------------------------+
| \# Hash chain for memory integrity                                    |
|                                                                       |
| memory.content_hash = SHA256(                                         |
|                                                                       |
| memory.content + memory.agent_id + memory.timestamp +                 |
|                                                                       |
| memory.prev_version_hash // Chain to previous version                 |
|                                                                       |
| )                                                                     |
|                                                                       |
| \# Verification: recompute and compare                                |
|                                                                       |
| def verify_memory(memory):                                            |
|                                                                       |
| expected = SHA256(memory.content + \...)                              |
|                                                                       |
| return expected == memory.content_hash                                |
+-----------------------------------------------------------------------+

Memory Poisoning Detection

ASMD includes a poisoning detection system that uses statistical anomaly
detection on memory mutations. The system maintains a baseline profile
of normal memory patterns for each agent (write frequency, content
distribution, embedding drift). When a write deviates significantly from
the baseline, it is flagged for quarantine.

Quarantined memories are stored but not included in RECALL results until
explicitly approved. This prevents a single compromised interaction from
corrupting the agent\'s entire knowledge base. The quarantine system is
configurable per agent and per memory type, with sensitivity levels
tunable by the operator.

Role-Based Access Control (RBAC)

ASMD implements fine-grained RBAC on memory. Every memory has an ACL
(Access Control List) that specifies which agents, users, and roles can
read, write, delete, share, or delegate access. Permissions are enforced
inside the query engine, meaning an agent physically cannot read
memories it does not have access to. This is not a filter applied after
retrieval; it is a predicate pushed into the ANN index.

+-----------------------------------------------------------------------+
| \# Permission enforcement inside ANN search                           |
|                                                                       |
| \# The index only returns results the requesting agent can see        |
|                                                                       |
| results = vector_index.search(                                        |
|                                                                       |
| query_vector = embed(query),                                          |
|                                                                       |
| pre_filter = {                                                        |
|                                                                       |
| \'acl.principal_id\': { \'\$in\': \[agent_id, agent_role, org_id\] }, |
|                                                                       |
| \'scope\': { \'\$in\': allowed_scopes }                               |
|                                                                       |
| },                                                                    |
|                                                                       |
| top_k = 10                                                            |
|                                                                       |
| )                                                                     |
|                                                                       |
| \# No overfetch-and-discard. Permission is part of the index scan.    |
+-----------------------------------------------------------------------+

Provenance Tracking

Every memory in ASMD has a complete provenance chain: who created it,
from what interaction, using what model, based on what inputs, and how
it has been modified since creation. This enables the \"why did you
answer that?\" audit query that enterprises require. The provenance
chain is built on top of the immutable event log, making it
tamper-evident and reconstructible.

Immutable Audit Log

All security-relevant operations (memory creation, modification,
deletion, sharing, permission changes, checkpoint operations) are
recorded in an append-only audit log. The audit log is OTel-compatible
and can be exported to external SIEM systems. The log itself is
integrity-protected using hash chains, ensuring it cannot be tampered
with after the fact.

Memory Lifecycle Engine

The lifecycle engine manages the birth, evolution, consolidation, and
death of memories. This is the most novel component of ASMD, directly
inspired by cognitive science research on human memory systems. Without
lifecycle management, agent memory grows without bound, retrieval
quality degrades, and storage costs explode.

Cognitive Forgetting

Forgetting is a first-class database operation in ASMD. The FORGET
command supports five distinct strategies, each appropriate for
different situations.

  ----------------- ------------------------------ -----------------------------
  **Strategy**      **Behavior**                   **When to Use**

  **hard_delete**   Permanently removes memory and Compliance (GDPR right to
                    all versions                   erasure), security incidents

  **soft_delete**   Marks as forgotten, excludable User requests, outdated
                    from RECALL, recoverable       information

  **decay**         Gradually reduces importance   Session context, transient
                    score over time using a        observations, low-value
                    forgetting curve               memories

  **consolidate**   Merges multiple episodic       Repeated interactions about
                    memories into a single         the same topic, pattern
                    semantic fact                  extraction

  **archive**       Moves to cold storage,         Historical data, old
                    excluded from hot queries,     sessions, cost optimization
                    retrievable on demand          
  ----------------- ------------------------------ -----------------------------

The decay strategy uses a modified Ebbinghaus forgetting curve where
importance decreases over time but is boosted by access (retrieval)
events. The formula is configurable per memory type.

+-----------------------------------------------------------------------+
| \# Forgetting curve with access-based reinforcement                   |
|                                                                       |
| effective_importance = base_importance \* e\^(-decay_rate \*          |
| time_since_creation)                                                  |
|                                                                       |
| \+ access_boost \* ln(1 + access_count)                               |
|                                                                       |
| \# Memory is auto-archived when effective_importance \< threshold     |
|                                                                       |
| \# Memory is auto-forgotten when effective_importance \<              |
| forget_threshold                                                      |
|                                                                       |
| \# Both thresholds are configurable per agent and per memory_type     |
+-----------------------------------------------------------------------+

Memory Consolidation

Consolidation is the process of promoting episodic memories (specific
interactions) into semantic memories (general facts). This is directly
analogous to how the human hippocampus consolidates short-term memories
into long-term knowledge during sleep.

ASMD runs consolidation as a background process. It identifies clusters
of episodic memories about the same topic (using embedding similarity),
extracts the common facts, creates a new semantic memory, and marks the
episodic memories as consolidated. The semantic memory links back to the
source episodics via provenance relations.

+-----------------------------------------------------------------------+
| \# Consolidation pipeline (background process)                        |
|                                                                       |
| 1\. Identify episodic memory clusters                                 |
|                                                                       |
| (embedding similarity \> threshold within time window)                |
|                                                                       |
| 2\. Extract common facts from cluster                                 |
|                                                                       |
| (LLM-powered extraction or pattern matching)                          |
|                                                                       |
| 3\. Create semantic memory with extracted facts                       |
|                                                                       |
| \- Link to source episodics via \'derived_from\' relations            |
|                                                                       |
| \- Set importance = max(source importances)                           |
|                                                                       |
| \- Set consolidation_state = \'active\'                               |
|                                                                       |
| 4\. Mark source episodics as \'consolidated\'                         |
|                                                                       |
| \- Reduce their importance (they\'re now redundant)                   |
|                                                                       |
| \- They remain queryable but de-prioritized in RECALL                 |
|                                                                       |
| 5\. Update graph: new semantic node inherits relations                |
|                                                                       |
| from consolidated episodics (union-merge)                             |
+-----------------------------------------------------------------------+

Conflict Resolution

When new information contradicts existing memories, ASMD needs a
resolution strategy. The system supports three modes: last-writer-wins
(default for low-stakes), explicit resolution (agent or user decides),
and evidence-weighted (the memory with more supporting evidence wins).
Conflicting memories can also be preserved simultaneously with a
\"contradicts\" relation, letting the agent or downstream logic decide
which to trust.

Scale-to-Zero Economics

ASMD is designed for ephemeral agent workloads. When an agent session
ends, its working memory is checkpointed and the compute resources are
released. There is no idle instance burning money. The pricing model is
per memory operation (write, read, search) rather than per
instance-hour. This is critical for the economics of multi-agent systems
where thousands of agents may be active for seconds and idle for hours.

Framework Integrations

ASMD is designed to be the default backend for every major agent
framework. The integration strategy is: make it trivially easy to adopt,
and provide capabilities that the framework\'s built-in storage cannot
match.

LangGraph Integration

LangGraph uses a checkpointer interface for state persistence. ASMD
provides a drop-in checkpointer that replaces the built-in
SQLite/Postgres checkpointers with full ASMD capabilities (branching,
replay, memory views).

+-----------------------------------------------------------------------+
| from asmd import ASMDCheckpointer                                     |
|                                                                       |
| \# Drop-in replacement for LangGraph\'s built-in checkpointer         |
|                                                                       |
| checkpointer = ASMDCheckpointer(                                      |
|                                                                       |
| connection_string=\'asmd://localhost:5433/mydb\',                     |
|                                                                       |
| \# ASMD-specific features:                                            |
|                                                                       |
| enable_memory_views=True, \# Auto-extract memories from state         |
|                                                                       |
| enable_replay=True, \# Full replay capability                         |
|                                                                       |
| consolidation_interval=\'1h\', \# Auto-consolidate episodic memories  |
|                                                                       |
| )                                                                     |
|                                                                       |
| graph = StateGraph(AgentState)                                        |
|                                                                       |
| graph.add_node(\'agent\', agent_node)                                 |
|                                                                       |
| \# \... build graph \...                                              |
|                                                                       |
| app = graph.compile(checkpointer=checkpointer)                        |
|                                                                       |
| \# Now every agent run is fully persisted, replayable, and            |
| memory-managed                                                        |
+-----------------------------------------------------------------------+

CrewAI Integration

+-----------------------------------------------------------------------+
| from asmd import ASMDMemory                                           |
|                                                                       |
| \# ASMD as CrewAI\'s memory backend                                   |
|                                                                       |
| memory = ASMDMemory(                                                  |
|                                                                       |
| connection_string=\'asmd://localhost:5433/mydb\',                     |
|                                                                       |
| scope=\'shared\', \# Crew members share memory                        |
|                                                                       |
| )                                                                     |
|                                                                       |
| crew = Crew(                                                          |
|                                                                       |
| agents=\[researcher, writer, editor\],                                |
|                                                                       |
| memory=memory, \# All agents share ASMD memory                        |
|                                                                       |
| )                                                                     |
+-----------------------------------------------------------------------+

OpenAI Agents SDK Integration

+-----------------------------------------------------------------------+
| \# ASMD as MCP server for OpenAI agents                               |
|                                                                       |
| \# The agent connects to ASMD via MCP and uses it as a tool           |
|                                                                       |
| agent = Agent(                                                        |
|                                                                       |
| name=\'research-agent\',                                              |
|                                                                       |
| tools=\[                                                              |
|                                                                       |
| \# ASMD exposed as MCP tools                                          |
|                                                                       |
| MCPServer(\'asmd://localhost:5433/mydb\'),                            |
|                                                                       |
| \]                                                                    |
|                                                                       |
| )                                                                     |
|                                                                       |
| \# The agent can now call asmd.remember, asmd.recall, asmd.forget     |
|                                                                       |
| \# as natural tool calls in its workflow                              |
+-----------------------------------------------------------------------+

Mem0-Compatible API

ASMD provides a Mem0-compatible API surface. Existing Mem0 users can
migrate with minimal code changes but gain persistence, access control,
replay, and governance they cannot get from Mem0 today.

+-----------------------------------------------------------------------+
| from asmd import Mem0Compat                                           |
|                                                                       |
| \# Drop-in replacement for mem0 client                                |
|                                                                       |
| m = Mem0Compat(connection_string=\'asmd://localhost:5433/mydb\')      |
|                                                                       |
| \# Same API as Mem0                                                   |
|                                                                       |
| m.add(\'User prefers dark mode\', user_id=\'user_123\',               |
| agent_id=\'agent_1\')                                                 |
|                                                                       |
| results = m.search(\'user preferences\', user_id=\'user_123\')        |
|                                                                       |
| \# But now you also get:                                              |
|                                                                       |
| \# - Persistent storage (not in-memory)                               |
|                                                                       |
| \# - Access control (agent_2 can\'t read agent_1\'s private memories) |
|                                                                       |
| \# - Temporal queries (what did we know last week?)                   |
|                                                                       |
| \# - Provenance (where did this memory come from?)                    |
|                                                                       |
| \# - Consolidation (automatic fact extraction from repeated           |
| interactions)                                                         |
+-----------------------------------------------------------------------+

Technology Stack Decisions

Every technology choice is made with two criteria: what gets us to
production fastest (Phase 1), and what gives us the best performance
ceiling when we need it (Phase 3). No premature optimization.

Primary Language: Rust

The core database engine is written in Rust. This is a non-negotiable
decision for a database project in 2026. Rust provides memory safety
without garbage collection (critical for predictable latency), zero-cost
abstractions, excellent concurrency primitives, and a growing ecosystem
of database-oriented libraries (DuckDB bindings, RocksDB bindings,
vector search libraries). Every successful new database of the last five
years has been Rust or C++, and Rust\'s safety guarantees dramatically
reduce the operational risk of memory corruption bugs that plague C++
database projects.

SDK Languages

  ---------------- -------------- -------------------------------------------
  **Language**     **Priority**   **Rationale**

  **Python**       P0 (Day 1)     95%+ of agent frameworks are Python.
                                  LangGraph, CrewAI, OpenAI SDK all Python.

  **TypeScript**   P1 (Month 2)   Vercel AI SDK, web-based agent builders,
                                  growing agent ecosystem in TS.

  **Rust**         P1 (Month 2)   Native integration for embedded mode,
                                  systems-level users.

  **Go**           P2 (Month 4)   Enterprise adoption, Kubernetes ecosystem,
                                  infrastructure teams.
  ---------------- -------------- -------------------------------------------

Dependency Decisions

  --------------- ---------------- --------------------- -------------------
  **Component**   **Phase 1        **Phase 3 Upgrade     **Rationale**
                  Choice**         Path**                

  **Embedded      DuckDB via       Custom columnar store Zero-dependency,
  Store**         duckdb-rs                              fast, great DX

  **Distributed   PostgreSQL 17    Custom LSM            Proven, ecosystem,
  Store**                          (Pebble/RocksDB)      wire compat

  **Vector        usearch          Custom DiskANN impl.  Good perf, upgrade
  Index**         (embedded) /                           when bottleneck
                  pgvector (dist.)                       

  **Graph Index** Adjacency        Custom graph engine   Simple, fast for
                  tables +                               2-3 hop queries
                  recursive CTE                          

  **Full-Text     tantivy (Rust    Integrated into       Mature Rust FTS
  Search**        BM25)            custom store          library

  **Cache**       Redis /          Integrated memory     Hot path for
                  DragonflyDB      cache                 working memory

  **Message       NATS             Integrated event bus  Lightweight, fast,
  Queue**                                                cloud-native

  **Embedding     Pluggable        Optional built-in     Model-agnostic by
  Model**         (OpenAI, local)  ONNX                  design
  --------------- ---------------- --------------------- -------------------

90-Day Execution Roadmap

The roadmap is organized into three 30-day sprints, each delivering a
shippable milestone. The guiding principle is: ship early, learn fast,
build the moat incrementally. Do not try to build the entire
architecture before shipping anything.

+-----------------------------------------------------------------------+
| **SPRINT PHILOSOPHY**                                                 |
+-----------------------------------------------------------------------+
| **Sprint 1:** Ship an MCP memory server that any agent can connect    |
| to. This is the wedge.                                                |
|                                                                       |
| **Sprint 2:** Add hybrid retrieval, checkpointing, and framework      |
| integrations. This is the product.                                    |
|                                                                       |
| **Sprint 3:** Add security, lifecycle management, and cloud           |
| deployment. This is the platform.                                     |
+-----------------------------------------------------------------------+

Sprint 1: The MCP Memory Server (Days 1-30)

**Goal:** Ship a working MCP server that any agent can connect to and
use for persistent memory. One-line setup. This is your launch
differentiator and the single most important milestone.

Week 1-2: Foundation

+:-----:+------------------------------------------------------------------+
| **1** | **Initialize Rust project with workspace structure**             |
|       |                                                                  |
|       | Crates: asmd-core (storage), asmd-mcp (MCP server),              |
|       | asmd-sdk-python (Python bindings). Use cargo workspace for       |
|       | monorepo. Set up CI/CD on GitHub Actions.                        |
+-------+------------------------------------------------------------------+

+:-----:+------------------------------------------------------------------+
| **2** | **Implement MemoryRecord storage on DuckDB**                     |
|       |                                                                  |
|       | Create the memory record schema. Implement CRUD operations.      |
|       | Store embeddings as binary blobs (upgrade to vector index in     |
|       | Sprint 2). Include content_hash computation on every write.      |
+-------+------------------------------------------------------------------+

+:-----:+------------------------------------------------------------------+
| **3** | **Build MCP server skeleton**                                    |
|       |                                                                  |
|       | Use the MCP Rust SDK (or implement the JSON-RPC transport        |
|       | layer). Expose asmd.remember and asmd.recall as MCP tools. This  |
|       | is the minimum to demonstrate the concept.                       |
+-------+------------------------------------------------------------------+

+:-----:+------------------------------------------------------------------+
| **4** | **Python SDK with Mem0-compatible surface**                      |
|       |                                                                  |
|       | Wrap the MCP client in a Python package. Provide add(),          |
|       | search(), get(), delete() methods that map to MCP tool calls.    |
|       | Publish to PyPI as asmd-python.                                  |
+-------+------------------------------------------------------------------+

Week 3-4: Make It Real

+:-----:+------------------------------------------------------------------+
| **5** | **Implement FORGET with soft_delete and hard_delete strategies** |
|       |                                                                  |
|       | The memory lifecycle starts here. Soft delete marks memories as  |
|       | forgotten but keeps them recoverable. Hard delete purges         |
|       | completely.                                                      |
+-------+------------------------------------------------------------------+

+:-----:+------------------------------------------------------------------+
| **6** | **Implement SHARE with basic ACL**                               |
|       |                                                                  |
|       | Support private and shared scopes. Implement permission checks   |
|       | on RECALL. This is minimal multi-agent support.                  |
+-------+------------------------------------------------------------------+

+:-----:+------------------------------------------------------------------+
| **7** | **Add basic semantic search**                                    |
|       |                                                                  |
|       | Integrate usearch for in-process HNSW vector index. RECALL now   |
|       | returns semantically similar memories, not just exact matches.   |
|       | This is where the product becomes useful.                        |
+-------+------------------------------------------------------------------+

+:-----:+------------------------------------------------------------------+
| **8** | **End-to-end demo: LangGraph agent with ASMD memory**            |
|       |                                                                  |
|       | Build a demo agent using LangGraph that connects to ASMD via     |
|       | MCP. Show persistent memory across sessions, basic recall, and   |
|       | forget. Record a demo video.                                     |
+-------+------------------------------------------------------------------+

+-----------------------------------------------------------------------+
| **SPRINT 1 DELIVERABLES**                                             |
+-----------------------------------------------------------------------+
| Working MCP server with REMEMBER, RECALL, FORGET, SHARE operations    |
|                                                                       |
| DuckDB-backed embedded storage with semantic vector search            |
|                                                                       |
| Python SDK on PyPI (Mem0-compatible API surface)                      |
|                                                                       |
| LangGraph integration demo                                            |
|                                                                       |
| GitHub repo (open source), README, quickstart guide                   |
|                                                                       |
| Demo video showing persistent agent memory in action                  |
+-----------------------------------------------------------------------+

Sprint 2: The Agent Database (Days 31-60)

**Goal:** Transform the memory server into a real database with hybrid
retrieval, checkpointing, event logging, and framework integrations.
This is when the product becomes sticky.

Week 5-6: Query Engine

+:-----:+------------------------------------------------------------------+
| **1** | **Implement RRF hybrid retrieval**                               |
|       |                                                                  |
|       | Combine vector similarity (usearch), BM25 lexical search         |
|       | (tantivy), and recency scoring. Expose as strategy=\'hybrid\' in |
|       | RECALL. This is a major differentiator.                          |
+-------+------------------------------------------------------------------+

+:-----:+------------------------------------------------------------------+
| **2** | **Implement graph relations and graph traversal**                |
|       |                                                                  |
|       | Add Relation entity. Support typed edges between memories.       |
|       | Implement 1-2 hop graph traversal in RECALL. This enables        |
|       | knowledge graph queries.                                         |
+-------+------------------------------------------------------------------+

+:-----:+------------------------------------------------------------------+
| **3** | **Implement permission-safe ANN (in-index filtering)**           |
|       |                                                                  |
|       | Add pre-filtering to the vector index so multi-tenant retrieval  |
|       | is efficient. No overfetch-and-discard.                          |
+-------+------------------------------------------------------------------+

Week 7-8: State Management & Integrations

+:-----:+------------------------------------------------------------------+
| **4** | **Implement AgentEvent log and OTel ingestion**                  |
|       |                                                                  |
|       | Build the immutable event log. Accept OTel GenAI convention      |
|       | traces. Every agent interaction is now recorded.                 |
+-------+------------------------------------------------------------------+

+:-----:+------------------------------------------------------------------+
| **5** | **Implement Checkpoint, Branch, Merge, Replay**                  |
|       |                                                                  |
|       | Full git-like state management. Agents can checkpoint, fork      |
|       | state for exploration, merge results back, and replay exact      |
|       | context at any point.                                            |
+-------+------------------------------------------------------------------+

+:-----:+------------------------------------------------------------------+
| **6** | **LangGraph checkpointer (drop-in replacement)**                 |
|       |                                                                  |
|       | Ship ASMDCheckpointer that works as a drop-in for LangGraph\'s   |
|       | checkpointer interface. One-line migration for existing          |
|       | LangGraph users.                                                 |
+-------+------------------------------------------------------------------+

+:-----:+------------------------------------------------------------------+
| **7** | **CrewAI and OpenAI Agents SDK integrations**                    |
|       |                                                                  |
|       | Ship integration packages for the two other major agent          |
|       | frameworks. Each should be one-line setup.                       |
+-------+------------------------------------------------------------------+

+-----------------------------------------------------------------------+
| **SPRINT 2 DELIVERABLES**                                             |
+-----------------------------------------------------------------------+
| Hybrid retrieval engine (RRF: semantic + lexical + graph + recency)   |
|                                                                       |
| Knowledge graph with typed relations and multi-hop traversal          |
|                                                                       |
| Immutable event log with OTel ingestion                               |
|                                                                       |
| Git-like checkpointing: branch, merge, replay                         |
|                                                                       |
| Drop-in integrations: LangGraph, CrewAI, OpenAI Agents SDK            |
|                                                                       |
| Permission-safe ANN (in-index filtering for multi-tenancy)            |
|                                                                       |
| Benchmark results: retrieval latency, write throughput, memory usage  |
+-----------------------------------------------------------------------+

Sprint 3: The Platform (Days 61-90)

**Goal:** Add enterprise-grade security, memory lifecycle automation,
distributed mode, and prepare for managed cloud launch. This is when the
product becomes a platform.

Week 9-10: Security & Lifecycle

+:-----:+------------------------------------------------------------------+
| **1** | **Implement hash chain integrity on all memory writes**          |
|       |                                                                  |
|       | Every memory write computes a content hash that chains to the    |
|       | previous version. Tamper detection is now built into the storage |
|       | layer.                                                           |
+-------+------------------------------------------------------------------+

+:-----:+------------------------------------------------------------------+
| **2** | **Implement memory poisoning detection**                         |
|       |                                                                  |
|       | Build baseline profiling for per-agent memory patterns. Flag     |
|       | anomalous writes for quarantine. Configurable sensitivity        |
|       | thresholds.                                                      |
+-------+------------------------------------------------------------------+

+:-----:+------------------------------------------------------------------+
| **3** | **Implement cognitive forgetting (decay + consolidation)**       |
|       |                                                                  |
|       | Background process: decay reduces importance over time,          |
|       | consolidation promotes episodic clusters into semantic facts.    |
|       | This is the most novel feature.                                  |
+-------+------------------------------------------------------------------+

+:-----:+------------------------------------------------------------------+
| **4** | **Implement full RBAC with delegation**                          |
|       |                                                                  |
|       | Fine-grained permissions: read, write, delete, share, delegate.  |
|       | Delegation chains (agent A grants agent B temporary access).     |
|       | Time-bounded permissions.                                        |
+-------+------------------------------------------------------------------+

Week 11-12: Distributed Mode & Cloud

+:-----:+------------------------------------------------------------------+
| **5** | **PostgreSQL-backed distributed mode**                           |
|       |                                                                  |
|       | Implement the same API over PostgreSQL for production            |
|       | multi-agent deployments. Use pgvector for vectors, recursive     |
|       | CTEs for graph traversal.                                        |
+-------+------------------------------------------------------------------+

+:-----:+------------------------------------------------------------------+
| **6** | **Local-to-cloud sync**                                          |
|       |                                                                  |
|       | Implement log-shipping from embedded DuckDB to cloud PostgreSQL. |
|       | Enable offline-first operation with eventual consistency.        |
+-------+------------------------------------------------------------------+

+:-----:+------------------------------------------------------------------+
| **7** | **Docker image and Kubernetes Helm chart**                       |
|       |                                                                  |
|       | Ship a production-ready Docker image and Helm chart. One-command |
|       | deployment on any Kubernetes cluster.                            |
+-------+------------------------------------------------------------------+

+:-----:+------------------------------------------------------------------+
| **8** | **Documentation, benchmarks, and launch preparation**            |
|       |                                                                  |
|       | Comprehensive docs site. Benchmark suite showing ASMD vs. raw    |
|       | Postgres + pgvector + Redis. Blog post: \"Why Agent Memory is a  |
|       | Database Problem.\" Product Hunt / HN launch plan.               |
+-------+------------------------------------------------------------------+

+-----------------------------------------------------------------------+
| **SPRINT 3 DELIVERABLES**                                             |
+-----------------------------------------------------------------------+
| Memory integrity verification (hash chains) and poisoning detection   |
|                                                                       |
| Cognitive forgetting: TTL decay, importance curves,                   |
| auto-consolidation                                                    |
|                                                                       |
| Full RBAC with delegation chains and time-bounded permissions         |
|                                                                       |
| PostgreSQL-backed distributed mode for production                     |
|                                                                       |
| Local-to-cloud sync (offline-first with eventual consistency)         |
|                                                                       |
| Docker image + Kubernetes Helm chart                                  |
|                                                                       |
| Documentation site, benchmark suite, and launch materials             |
+-----------------------------------------------------------------------+

Post-90-Day Roadmap

Months 4-6: Growth & PMF

Focus on developer adoption, community building, and product-market fit.
Ship the managed cloud service (hosted ASMD). Build integrations with
additional frameworks (AutoGen, Semantic Kernel, Haystack). Launch the
\"Memory Engineering\" blog series to own the narrative. Target 1,000
GitHub stars and 100 production users.

Months 7-12: Scale & Monetize

Launch ASMD Cloud as a managed service with usage-based pricing (per
memory operation). Begin replacing storage engine hot paths with custom
implementations where benchmarks justify it. Implement DiskANN-style
vector indexing for cost efficiency at scale. Build the admin dashboard
for enterprise customers: memory usage, security alerts, consolidation
metrics, cost tracking.

Year 2: Platform & Ecosystem

ASMD becomes the standard infrastructure layer for agent memory.
Third-party plugins and extensions. Enterprise features: SOC2
compliance, HIPAA, data residency. Consider physical AI / robotics
extensions (MCAP format support, time-series optimized storage,
high-rate sensor ingestion). Raise Series A to fund the full custom
storage engine build.

Benchmark & Validation Plan

Benchmarks are not an afterthought. They are the primary evidence that
ASMD delivers on its promises. The benchmark suite is designed to
measure the specific operations that matter for agent workloads, not
generic database benchmarks.

Core Benchmarks

  ------------------------ ----------------------------- ------------------- ----------------
  **Benchmark**            **What It Measures**          **Target**          **Comparison**

  **Memory Write           REMEMBER ops/sec with hash    \>10K ops/sec       Mem0, raw
  Throughput**             chain                         (embedded), \>50K   Postgres, Redis
                                                         ops/sec             
                                                         (distributed)       

  **Hybrid Recall          RRF retrieval                 p99 \< 50ms at 1M   Pinecone,
  Latency**                (vector+BM25+graph+recency)   memories            pgvector,
                                                                             Weaviate

  **Permission-Safe ANN**  Vector search with in-index   \< 2x overhead vs   Mongo
                           ACL filtering                 unfiltered ANN      pre-filter,
                                                                             Neo4j filtered

  **Checkpoint/Restore**   Snapshot + restore agent      \< 10ms for 1MB     LangGraph SQLite
                           state                         state               checkpointer

  **Replay                 Rebuild full context at       \< 100ms for        No direct
  Reconstruction**         checkpoint N                  1K-event session    comparison
                                                                             exists

  **Memory Consolidation** Episodic cluster detection +  Process 10K         No direct
                           semantic extraction           memories in \< 60s  comparison
                                                                             exists

  **Graph Traversal**      2-hop relation query across   \< 20ms at 100K     Neo4j, NetworkX
                           memory graph                  nodes               

  **Concurrent Agents**    Throughput under 100          Linear scaling to   Custom
                           concurrent agent sessions     100 agents          Postgres+Redis
                                                                             setups
  ------------------------ ----------------------------- ------------------- ----------------

Quality Benchmarks

Beyond performance, ASMD must prove that its memory management actually
improves agent quality. These benchmarks measure the end-to-end impact
on agent behavior.

  --------------------- ------------------------------ -------------------
  **Benchmark**         **Methodology**                **Target**

  **Retrieval           Compare RECALL results vs      NDCG@10 \> 0.8
  Accuracy**            ground-truth relevant memories 
                        using standard IR metrics      
                        (NDCG, MRR)                    

  **Memory Poisoning    Inject adversarial memories,   \> 95% detection,
  Detection Rate**      measure detection rate and     \< 5% FPR
                        false positive rate            

  **Consolidation       Compare LLM-extracted semantic \> 85% F1 score
  Quality**             facts vs human-annotated       
                        ground truth                   

  **Agent Task Accuracy Run standard agent benchmarks  \> 20% improvement
  with ASMD**           (GAIA, SWE-bench) with and     (matching Mem0\'s
                        without ASMD memory            26% claim)
  --------------------- ------------------------------ -------------------

Competitive Positioning

  ------------------------------ ---------------------- ------------------------------
  **Competitor**                 **What They Do**       **ASMD Advantage**

  **Pinecone/Milvus/Weaviate**   Pure-play vector       Vectors are a feature, not a
                                 databases              product. ASMD adds memory
                                                        lifecycle, graph, state,
                                                        security.

  **Neo4j**                      Graph database +       ASMD is agent-memory-first,
                                 bolt-on vectors        not graph-first. Native memory
                                                        operations, not Cypher.

  **Mem0**                       Memory library         ASMD is a real database with
                                 wrapping external      persistence, ACL, replay,
                                 stores                 governance. Mem0 is a library.

  **Graphiti (Zep)**             Graph memory requiring ASMD is standalone. No
                                 Neo4j/FalkorDB         external graph DB dependency.
                                                        Integrated storage.

  **Cognee**                     Graph+vector memory    ASMD is an engine, not
                                 adapter layer          adapters. No
                                                        NetworkX/FalkorDB/Neo4j
                                                        dependency.

  **PostgreSQL + extensions**    pgvector + recursive   ASMD provides the memory
                                 CTEs + custom          model, lifecycle, and
                                                        agent-native API that Postgres
                                                        extensions never will.

  **LangGraph checkpointers**    Basic state            ASMD adds branching, replay,
                                 persistence            memory views, consolidation,
                                                        security on top.

  **Redis**                      Fast cache for session ASMD provides durability,
                                 state                  versioning, semantic search,
                                                        and governance. Redis is
                                                        volatile.
  ------------------------------ ---------------------- ------------------------------

+-----------------------------------------------------------------------+
| **THE MOAT**                                                          |
+-----------------------------------------------------------------------+
| **Short-term moat (Year 1):** First-mover as MCP-native memory        |
| database. One-line integration with major agent frameworks. Developer |
| experience and time-to-value that nobody else offers.                 |
|                                                                       |
| **Medium-term moat (Year 2):** Unified storage engine where vector +  |
| graph + temporal are co-located on the same pages. This is a deep     |
| engineering advantage that cannot be replicated by bolting extensions |
| onto existing databases.                                              |
|                                                                       |
| **Long-term moat (Year 3+):** Network effects from being the          |
| standard. If every LangGraph/CrewAI/OpenAI agent uses ASMD for        |
| memory, switching costs become enormous. The memory data itself is    |
| the lock-in.                                                          |
+-----------------------------------------------------------------------+

Honest Risk Assessment

Every ambitious project has kill risks. Acknowledging them is not
weakness; it is how you build contingency plans. Here are the risks that
could kill ASMD, ranked by severity.

  --------------------- -------------- ----------------- ---------------------------------
  **Risk**              **Severity**   **Probability**   **Mitigation**

  **PostgreSQL          HIGH           MEDIUM            Compete on memory model and
  extensions get good                                    agent-native API, not raw
  enough (pgvector +                                     storage. Postgres will never ship
  AGE + temporal)**                                      REMEMBER/RECALL/FORGET as native
                                                         operations.

  **AWS/GCP/Azure ship  HIGH           HIGH (18-24mo)    Be open source and the standard
  managed agent memory                                   before they ship. They will build
  service**                                              on your design, just as AWS built
                                                         on Redis/Elasticsearch.

  **Agent era stalls or MEDIUM         LOW               The infrastructure gap is real
  fragments**                                            regardless of which framework
                                                         wins. Memory is needed even if
                                                         agents are simpler than expected.

  **Storage engine      MEDIUM         MEDIUM            Phase 1 uses proven backends.
  performance ceiling**                                  Only build custom storage after
                                                         PMF and with data on actual
                                                         bottlenecks.

  **Mem0 or Graphiti    LOW            MEDIUM            They are libraries, not database
  pivots to become a                                     companies. Building a database is
  database**                                             a fundamentally different
                                                         engineering challenge. 18-month
                                                         head start.

  **Open-source         MEDIUM         MEDIUM            Follow the proven open-core
  sustainability**                                       model: open-source engine,
                                                         managed cloud for revenue. This
                                                         is how every successful database
                                                         company of the last decade
                                                         operated.
  --------------------- -------------- ----------------- ---------------------------------

Go-to-Market Strategy

Phase 1: Developer Adoption (Months 1-6)

The primary GTM motion is bottom-up developer adoption, identical to how
MongoDB, Redis, Supabase, and Neon grew. The strategy is: be the best
open-source tool for a real problem, build community, and let developers
pull you into their companies.

**Own the narrative:** \"Memory Engineering\" was coined in 2025 by the
MongoDB team. Write the definitive blog series, give the conference
talks, build the benchmarks. Be the authority on agent memory as an
infrastructure problem.

**Target communities:** LangChain/LangGraph Discord (150K+ members),
CrewAI community, r/LocalLLaMA, AI engineering meetups, agent
hackathons.

**Content strategy:** Weekly blog posts, monthly deep-dive papers,
comparison benchmarks (ASMD vs. Postgres+pgvector+Redis), integration
tutorials, \"Build an agent with perfect memory in 5 minutes\" guides.

Phase 2: Enterprise Pipeline (Months 6-12)

Once developer adoption reaches critical mass (target: 2,000 GitHub
stars, 500 weekly active users), enterprise accounts will self-qualify.
The enterprise pitch is: \"Your agents are making decisions based on
memory that has no audit trail, no access control, no integrity
verification, and no governance. ASMD fixes this.\" Target industries:
financial services (audit requirements), healthcare (HIPAA compliance),
and enterprise SaaS (multi-tenant agent deployments).

Pricing Model

  ---------------- --------------- ----------------------------------------
  **Tier**         **Price**       **Includes**

  **Open Source**  Free forever    Full engine, embedded mode, all MCP/SDK
                                   features, community support

  **Cloud          \$29/month      Managed cloud, 1M memory operations,
  Starter**                        100K memories stored, 3 agent slots

  **Cloud Pro**    \$199/month     10M operations, 1M memories, unlimited
                                   agents, email support, SLA

  **Enterprise**   Custom          Dedicated infrastructure, SOC2, HIPAA,
                                   SSO, audit exports, dedicated support
  ---------------- --------------- ----------------------------------------

Recommended Team Structure

Building a database is one of the hardest engineering projects. The
minimum viable team for the first 90 days is three to four people. Here
is the recommended structure.

  ---------------------- ------------------------- -------------------------
  **Role**               **Focus**                 **Key Skills**

  **Founding Engineer 1  Rust core engine,         Rust, systems
  (Storage)**            DuckDB/Postgres           programming, database
                         integration, vector       internals, experience
                         index, storage page       with LSM trees or
                         layout                    columnar stores

  **Founding Engineer 2  MCP server, Python SDK,   Rust + Python, MCP
  (API/Integrations)**   LangGraph/CrewAI          protocol, agent framework
                         integrations, REST/gRPC   expertise, API design
                         API                       

  **Founding Engineer 3  Query engine (RRF, graph  Rust, information
  (Query/Security)**     traversal), RBAC,         retrieval, security
                         integrity, poisoning      engineering, graph
                         detection                 algorithms

  **Founder/Product      Architecture decisions,   Technical depth, product
  (You)**                roadmap prioritization,   sense, communication,
                         developer community,      ability to write and ship
                         content, fundraising      code
  ---------------------- ------------------------- -------------------------

Conclusion

The opportunity is real, verified, and time-bounded. The AI
infrastructure landscape in 2026 has a glaring gap: no purpose-built
database for agent memory, state, and governance. Every production
deployment is duct-taping 3-5 different systems together. The research
is clear (60+ verified sources), the demand is proven (Mem0\'s traction,
\$200B+ AI investment), and the timing is now (MCP standardization,
contextual memory becoming table stakes).

ASMD is not another vector database. It is not a memory library. It is
the first database designed from the ground up for the agent cognition
lifecycle. Its primitives are REMEMBER, RECALL, FORGET, SHARE, BRANCH,
MERGE. Its interface is MCP-native. Its storage co-locates vectors,
graphs, temporal versions, and metadata on the same pages. Its security
is built in, not bolted on.

The 90-day plan is aggressive but achievable: ship the MCP memory server
in 30 days, add hybrid retrieval and framework integrations in 60 days,
add security and distributed mode in 90 days. The first milestone (day
30) is the single most important: a working MCP server that any agent
can connect to in one line and use for persistent, searchable,
governable memory.

  -----------------------------------------------------------------------
  **THE FIRST MOVE**

  **Build the MCP-native memory server. Ship it in 30 days.** Make it the
  easiest way for any agent to have persistent memory. Everything else
  builds from there. The team that ships this first wins the category.
  -----------------------------------------------------------------------

\-\--

*End of Document*
