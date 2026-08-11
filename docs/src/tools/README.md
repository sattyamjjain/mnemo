# MCP Tools Reference

Mnemo registers **21 MCP tools** via the `rmcp` framework. Each is available over
the STDIO transport when running the `mnemo` binary.

Every tool takes a single JSON object argument (the fields below) and returns a
JSON-encoded text result. On failure a tool returns an `isError` result whose
text is the error message rather than throwing. Required arguments are **bold**;
all others are optional with sensible defaults.

> **Role filtering.** When the server is built with
> [`MnemoServer::with_role_filter`](../integrations/mcp-server.md), a caller only
> sees the tools its role is allowed to call in `tools/list`, and a denied
> `tools/call` returns a structured `-32601` (method-not-found) error instead of
> a silent empty result. Without a filter every tool below is visible and
> callable.

The ten core tools also have dedicated pages (linked in the tables). The
remaining eleven are documented inline here.

## Core memory operations

| Tool | Purpose | Key arguments | Returns |
|------|---------|---------------|---------|
| [mnemo.remember](./remember.md) | Store a new memory (semantic + keyword searchable). | **content**; `memory_type`, `scope`, `importance`, `tags`, `metadata`, `ttl_seconds`, `related_to`, `thread_id`, `source_type`, `source_id`, `org_id`, `decay_rate`, `created_by` | `{ id, content_hash, status }` |
| [mnemo.recall](./recall.md) | Search/retrieve memories by strategy (semantic, lexical, hybrid, graph, reconstruct, exact, auto). | **query**; `limit`, `memory_type(s)`, `scope`, `min_importance`, `tags`, `strategy`, `temporal_range`, `org_id`, `recency_half_life_hours`, `hybrid_weights`, `rrf_k`, `as_of`, `explain`, `current_fact_resolver`, `orientation_cache`, `domain_scope` | `{ memories, total }` (plus optional `orientation`, `belief_state`, `explain` fields) |
| [mnemo.forget](./forget.md) | Soft-delete, hard-delete, decay, consolidate, or archive memories by ID or criteria. | **memory_ids**; `strategy`, `criteria` (`max_age_hours`, `min_importance_below`, `memory_type`, `tags`) | `{ forgotten, errors, status }` |
| mnemo.forget_subject | GDPR / DPDPA subject erasure: redact (default, preserves hash chain) or hard-delete every memory tagged `subject:<id>`. | **subject_id**; `strategy`, `agent_id` | `{ subject_id, strategy, matched, forgotten, cascaded_events, errors }` |
| mnemo.provenance | Read write-provenance: who wrote each memory, under what capability, in what session, when. Tamper-evident audit history that survives forgetting. | one of `memory_id`, `principal`, `session_id`; `limit` | one record (by `memory_id`) or an array (by `principal`/`session_id`) of `{ id, memory_id, principal, capability_id, session_id, op, authored_at, content_hash, prev_hash }` |
| mnemo.forget_by_provenance | FORGET BY PROVENANCE: revoke every memory a principal (or session/trace) authored in one call. Targeted remediation, not a wipe — the audit trail survives. | one of `principal`, `session_id`; `strategy` (soft_delete/hard_delete/redact) | `{ forgotten, errors, status }` |
| [mnemo.share](./share.md) | Grant one or more agents access to one or more memories (batch supported). | **memory_id**, **target_agent_id**; `memory_ids`, `target_agent_ids`, `permission`, `expires_in_hours` | `{ acl_ids, memory_ids, shared_with, errors, status }` |
| mnemo.consolidate | Consolidate related memories into one revisable topic document (Infini-Memory), preserving provenance + a hash-chained audit event. | **memory_ids**, **topic_name**; `agent_id`, `summary`, `supersede`, `thread_id`, `metadata` | `{ topic_document_id, topic_name, source_count, version, superseded_id, member_ids, content_hash, consolidation_event_id, revision_event_id, status }` |

## Checkpoint, branch, merge & replay (git-like state)

| Tool | Purpose | Key arguments | Returns |
|------|---------|---------------|---------|
| [mnemo.checkpoint](./checkpoint.md) | Snapshot the current agent state (state, active memories, event cursor). | **thread_id**, **state_snapshot**; `branch_name`, `label`, `metadata` | `{ checkpoint_id, parent_id, branch_name, status }` |
| [mnemo.branch](./branch.md) | Fork state into a new branch from an existing checkpoint. | **thread_id**, **new_branch_name**; `source_checkpoint_id`, `source_branch` | `{ checkpoint_id, branch_name, source_checkpoint_id, status }` |
| [mnemo.merge](./merge.md) | Merge a branch into another (full, cherry-pick, or squash). | **thread_id**, **source_branch**; `target_branch`, `strategy`, `cherry_pick_ids` | `{ checkpoint_id, target_branch, merged_memory_count, status }` |
| [mnemo.replay](./replay.md) | Reconstruct agent context at a checkpoint (state, memories, events up to that point). | **thread_id**; `checkpoint_id`, `branch_name`, `as_of` | `{ id, content, memory_type, created_at, status }` |

## Delegation & verification

| Tool | Purpose | Key arguments | Returns |
|------|---------|---------------|---------|
| [mnemo.delegate](./delegate.md) | Grant scoped, time-bounded (optionally re-delegable) access to your memories. | **delegate_id**, **permission**; `memory_ids`, `tags`, `max_depth`, `expires_in_hours` | `{ delegation_id, delegator, delegate, permission, status }` |
| [mnemo.verify](./verify.md) | Verify per-record hash-chain integrity; detect tampered/corrupted records. | `agent_id`, `thread_id` | `{ valid, total_records, verified_records, first_broken_at, error_message, status }` |
| mnemo.trajectory_audit | GEM-aligned trajectory-correctness audit (arXiv:2605.26252): unregulated growth, missing semantic revision, capacity-driven forgetting, read-only retrieval. | `agent_id`, `thread_id`, `active_bank_ceiling`, `fact_key`, `named_forget_strategies` | `{ report, all_ok }` |

## Attention state (arXiv:2605.18226)

Requires the server to be built with `MnemoServer::with_attention_state`;
otherwise both tools return an error result.

| Tool | Purpose | Key arguments | Returns |
|------|---------|---------------|---------|
| mnemo.attention_state.put | Store a precomputed, opaque attention-state blob under `(agent_id, prefix_hash)`. | **agent_id**, **prefix_hash**, **state_blob_hex**; `model`, `ttl_seconds` | `{ id, agent_id, prefix_hash, model, ttl_seconds, created_at }` |
| mnemo.attention_state.get | Fetch the most-recent attention-state record for `(agent_id, prefix_hash)`. | **agent_id**, **prefix_hash** | record `{ id, agent_id, prefix_hash, model, state_blob_hex, ttl_seconds, created_at }` or `null` on miss |

## Agent-controlled memory — `mem_*` family (AutoMEM)

A flat, agent-managed store: nothing is written unless the agent explicitly
calls `mem_write`. Entries are tagged `agent-managed` and are only visible to
`mem_read` (not the general `recall` pipeline).

| Tool | Purpose | Key arguments | Returns |
|------|---------|---------------|---------|
| mnemo.mem_write | Persist an entry the agent decided is worth keeping. | **content**; `tags`, `importance`, `memory_type`, `metadata`, `agent_id`, `org_id` | `{ id, content_hash, store, status }` |
| mnemo.mem_read | Read back only the agent's own `agent-managed` entries. | **query**; `limit`, `tags`, `agent_id`, `org_id` | `{ memories, total, store }` |
| mnemo.mem_revise | Supersede a stale agent-managed entry with a corrected one (newest wins). | **id**, **content**; `tags`, `importance`, `agent_id`, `org_id` | `{ id, revises, content_hash, store, status }` |
| mnemo.mem_forget | Drop an agent-managed entry (soft by default; `hard=true` for permanent). | **id**; `hard`, `agent_id` | `{ forgotten, errors, status }` |

## Plan / experience memory (DocTrace)

Caches successful retrieval/reasoning plans for replay. Requires the server's
experience-memory mode to be enabled.

| Tool | Purpose | Key arguments | Returns |
|------|---------|---------------|---------|
| mnemo.remember_plan | Cache a successful plan (query, ordered steps, chunk ids, outcome score in [0,1]). Below-threshold plans are not stored. | **query**, **steps**, **chunk_ids**, **outcome_score**; `scope`, `agent_id`, `org_id` | `{ id, signature, stored, status }` |
| mnemo.recall_plan | Replay the best cached plan whose query signature matches above a threshold (default 0.7). RBAC-gated. | **query**; `similarity_threshold`, `agent_id`, `org_id` | `{ plan, candidates_considered, hit }` |

## A note on audit-log export

`mnemo.export_audit_log` is referenced by the manifest schema but is **not** one
of the 21 registered tools above. The audit-log export capability itself already
exists today as a library API:
[`mnemo_compliance::export_audit_log(events, format, signer)`](../compliance/eu-ai-act.md)
(with `verify_ndjson_signed`), which produces a signed NDJSON / EU-AI-Office CSV
bundle from the hash-chained event log.

The earlier **capability-lease** design (per-read lease tokens gating
`forget_subject` / audit-log export) was **removed as dead code** — it was never
wired, and on the stdio transport a single-operator lease is ceremony rather than
isolation. The design is captured in
[#126](https://github.com/sattyamjjain/mnemo/issues/126) for a future
multi-caller (authenticated) transport where it has real value.
