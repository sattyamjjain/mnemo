# Mnemo Predictive Issue Analysis

**Date:** 2026-02-07
**Analyzed by:** 4 specialized agents (concurrency, performance, error handling, architecture)
**Codebase:** 9-crate Rust workspace, ~15K LOC

---

## Executive Summary

**Total Issues Found: 42 distinct issues across 4 categories**

| Severity | Count | Key Themes |
|----------|-------|------------|
| CRITICAL | 10 | Data destruction, silent failures, hangs, cascading panics |
| HIGH | 16 | Error swallowing, lock contention, feature gaps, security inconsistencies |
| MEDIUM | 12 | Lock poisoning, unbounded growth, missing configurations |
| LOW | 4 | Overflow edge cases, migration idempotency |

---

## CRITICAL Issues (Fix Immediately)

### C1. Shutdown Saves EMPTY Vector Index -- Actively Destroys Data
**File:** `crates/mnemo-cli/src/main.rs:233-239`
**What:** The shutdown code creates a `new()` empty `UsearchIndex` and saves it over the existing index file. The populated index from the running engine is never referenced.
**Impact:** Every graceful shutdown destroys the vector index. On restart, semantic search returns zero results.
**Fix:** Save the engine's actual index reference, not a fresh empty one.

### C2. OpenAI API Has No HTTP Timeout -- System Hangs Forever
**File:** `crates/mnemo-core/src/embedding/openai.rs:30-37`
**What:** `reqwest::Client::new()` with no timeout. If OpenAI API hangs, the `remember()` call blocks indefinitely while holding the DuckDB mutex, freezing ALL operations across ALL protocols.
**Impact:** A single network hiccup makes the entire Mnemo instance unresponsive.
**Fix:** `reqwest::Client::builder().timeout(Duration::from_secs(30)).connect_timeout(Duration::from_secs(10)).build()`

### C3. Audit Trail Events Silently Discarded (7 locations)
**Files:** `remember.rs:192`, `recall.rs:397`, `forget.rs:237`, `share.rs:121`, `checkpoint.rs:101`, `merge.rs:158`, `branch.rs:96`
**Pattern:** `let _ = engine.storage.insert_event(&event).await;`
**What:** Event insertion failures are completely invisible. No log, no error. The hash chain silently breaks.
**Impact:** SOC2/HIPAA compliance claims undermined. `verify` tool reports broken chains with no way to determine when/why.
**Fix:** At minimum `if let Err(e) = ... { tracing::error!(...) }`. Better: propagate as warning to caller.

### C4. Failed Decryption Silently Returns Ciphertext to User
**File:** `crates/mnemo-core/src/query/recall.rs:344-354`
**What:** Three nested `if let Ok` patterns silently swallow decryption failures. The raw base64-encoded ciphertext is returned as memory content.
**Impact:** User receives garbled data with no indication. HIPAA compliance risk -- encrypted data served as plaintext.
**Fix:** Return error or flag decryption failures explicitly.

### C5. DuckDB Row Deserialization -- 15+ `parse().unwrap()` Calls
**File:** `crates/mnemo-core/src/storage/duckdb.rs:64-1033`
**What:** `Uuid::parse_str(&id_str).unwrap()`, `row.get::<_, String>(3)?.parse().unwrap()` for memory_type, scope, source_type, consolidation_state, and across event/checkpoint/delegation/relation parsing.
**Impact:** A single corrupted database row crashes the server for ALL users. Schema drift between versions will trigger this.
**Fix:** Replace `.unwrap()` with `.map_err(|e| Error::Storage(...))?`

### C6. Hash Chain Race Condition (TOCTOU)
**File:** `crates/mnemo-core/src/query/remember.rs:66-74`
**What:** `get_latest_memory_hash()` and `insert_memory()` are not atomic. Two concurrent `remember()` calls for the same agent create a forked chain.
**Impact:** Chain integrity verification fails. PostgreSQL deployments are immediately vulnerable.
**Fix:** CAS-style retry loop or serializable transaction isolation.

### C7. PgVectorIndex Returns Empty -- Recall Broken on PostgreSQL
**File:** `crates/mnemo-postgres/src/pgvector_index.rs:51-64`
**What:** `search()` and `filtered_search()` always return `Ok(Vec::new())`. The recall engine calls these for semantic/hybrid/auto/graph strategies.
**Impact:** PostgreSQL users get zero recall results for semantic queries. Silent correctness bug.
**Fix:** Implement pgvector-backed search in `filtered_search()` or restructure recall to use SQL-based search for PG backend.

### C8. USearch Index -- 22 `RwLock.unwrap()` Calls Cause Cascading Panics
**File:** `crates/mnemo-core/src/index/usearch.rs:41-200`
**What:** All RwLock accesses use `.unwrap()`. One panic while holding a lock poisons it, causing every subsequent operation to panic.
**Impact:** A single vector index error crashes the entire server permanently until restart.
**Fix:** `.unwrap_or_else(|e| e.into_inner())` or convert to `Result<>`.

### C9. Cache Mutex -- 6 `.unwrap()` Calls on std::sync::Mutex
**File:** `crates/mnemo-core/src/cache.rs:38-96`
**What:** Same cascading panic pattern as C8 but for the memory cache.
**Impact:** Cache lock poison makes all remember/recall operations crash.

### C10. `process::exit(0)` Skips All Cleanup
**File:** `crates/mnemo-cli/src/main.rs:214`
**What:** Idle timeout calls `std::process::exit(0)` which terminates immediately -- no destructors, no index save, no flush.
**Impact:** Data loss on idle shutdown.

---

## HIGH Issues (Fix in Next Sprint)

### H1. DuckDB Lock Contention -- All Operations Serialized
**File:** `crates/mnemo-core/src/storage/duckdb.rs:17`
**What:** Single `Arc<Mutex<Connection>>` serializes all reads and writes. 10+ concurrent agents create a bottleneck.
**Threshold:** Noticeable at >10 concurrent operations.

### H2. Delegate Security Inconsistency Across Protocols
**Files:** MCP (`server.rs:500`), REST (`handlers.rs:333-408`), gRPC (`lib.rs:510-568`)
**What:** MCP uses hardcoded `default_agent_id` with no permission check. REST verifies `Delegate` permission. gRPC trusts request blindly.
**Impact:** Protocol-dependent security posture for the same operation.

### H3. Index Cleanup Silently Fails on Forget
**File:** `crates/mnemo-core/src/query/forget.rs:129-147`
**Pattern:** `let _ = engine.index.remove(*id);` and `let _ = ft.remove(*id);`
**Impact:** Deleted memories continue appearing in search results. GDPR "right to be forgotten" violation.

### H4. Relation Insertions Silently Discarded
**Files:** `remember.rs:157`, `lifecycle.rs:272`
**Impact:** User creates relationships via `related_to`, gets success response, but graph links are lost.

### H5. StorageBackend -- Monolithic 31-Method Trait
**File:** `crates/mnemo-core/src/storage/mod.rs:28-91`
**Impact:** New backend requires ~1,400 lines. Feature additions touch all backends simultaneously.

### H6. No Signal Handling (SIGTERM/SIGINT)
**File:** `crates/mnemo-cli/src/main.rs`
**Impact:** Docker stop / k8s pod termination kills process without cleanup. Index never saved.

### H7. `serde_json::to_string_pretty().unwrap()` -- 10 Instances in MCP
**File:** `crates/mnemo-mcp/src/server.rs:109,165,228,295,326,362,408,457,522,549`
**Impact:** NaN in vector scores crashes the MCP server.

### H8. Hash Chain Error Silently Becomes None
**Files:** `remember.rs:163`, `recall.rs:364`, `forget.rs:214`
**Pattern:** `.unwrap_or(None)` on hash lookup failures starts a new chain silently.

### H9. Event Hash Chain Same TOCTOU Race
**File:** `crates/mnemo-core/src/query/remember.rs:163-192`
**Impact:** Same as C6 but for the event chain.

### H10. USearch Mapping Inconsistency on Add Failure
**File:** `crates/mnemo-core/src/index/usearch.rs:51-78`
**What:** If `index.add()` fails after `allocate_key()`, UUID-key mappings are orphaned.

### H11. Memory Update Failures Silently Discarded During Consolidation
**File:** `crates/mnemo-core/src/query/lifecycle.rs:277`
**Impact:** Memory duplication over time from re-processing.

### H12. Invalid Enum Values Silently Ignored (30+ locations)
**Files:** MCP server.rs, REST handlers.rs, gRPC lib.rs
**Pattern:** `.and_then(|s| s.parse().ok())` -- user typos silently produce defaults.

### H13. DuckDB `list_memories` -- thread_id Filter Never Applied
**File:** `crates/mnemo-core/src/storage/duckdb.rs:268-281`
**What:** `where_clause` is computed before `thread_id` condition is added to `conditions` vec. thread_id filtering is silently broken.

### H14. REST Server Failure Silently Swallowed
**File:** `crates/mnemo-cli/src/main.rs:165-167`
**What:** REST server runs in `tokio::spawn`. Port-in-use error panics the task silently. MCP starts fine, REST never starts, no user feedback.

### H15. OTLP Ingest Bypasses Hash Chain
**File:** `crates/mnemo-rest/src/handlers.rs:639`
**What:** Events inserted directly via `engine.storage` with `prev_hash: None`, breaking the audit chain.

### H16. No Retry Logic for OpenAI API
**File:** `crates/mnemo-core/src/embedding/openai.rs:57-63`
**Impact:** Transient 429/503 errors cause permanent operation failure.

---

## MEDIUM Issues (Track for Future Sprint)

### M1. Cache Grows Without Bound on Eviction Failure
**File:** `crates/mnemo-core/src/cache.rs:50-77`

### M2. Protocol Feature Gaps (temporal_range, recency_half_life, ForgetCriteria)
**Files:** REST and gRPC missing features available in MCP.

### M3. No Versioned Schema Migrations
**Files:** `crates/mnemo-core/src/storage/migrations.rs`, `crates/mnemo-postgres/src/migrations.rs`

### M4. MnemoEngine All-Public Fields (No Encapsulation)
**File:** `crates/mnemo-core/src/query/mod.rs:50-225`

### M5. Timestamp Stored as String -- Fragile Cross-Timezone Comparison
**What:** `record.created_at > *as_of` does string comparison. Breaks with different timezone offsets.

### M6. Missing Error Enum Variants (Encryption, Timeout, Configuration, etc.)
**File:** `crates/mnemo-core/src/error.rs`

### M7. Permission Check Error Defaults to "Deny"
**File:** `crates/mnemo-core/src/query/recall.rs:500`

### M8. No Timeout on ONNX Inference (spawn_blocking)
**File:** `crates/mnemo-core/src/embedding/onnx.rs:235-324`

### M9. pgwire No Graceful Shutdown
**File:** `crates/mnemo-pgwire/src/lib.rs:79-93`

### M10. RwLock Read-Write Upgrade Deadlock in USearch
**File:** `crates/mnemo-core/src/index/usearch.rs:51-78`

### M11. Hardcoded Magic Numbers in Recall Path
**File:** `crates/mnemo-core/src/query/recall.rs` (oversampling 3x, max_hops=2, decay=0.5, half_life=168h, max_ids=10000)

### M12. Feature Flag CI Coverage -- Untested Combinations
**Files:** Various Cargo.toml feature gates

---

## LOW Issues

### L1. USearch next_key Overflow (u64)
### L2. Migration ALTER TABLE Errors Silently Ignored (Intentional)
### L3. Access Tracking Silently Discarded
### L4. Scattered Configuration (3 different mechanisms)

---

## Top 5 Remediation Priorities

| # | Issue | Effort | Why First |
|---|-------|--------|-----------|
| 1 | C1 -- Empty index save | 30 min | Actively destroying data on every shutdown |
| 2 | C2 -- OpenAI timeout | 15 min | Single network blip freezes entire system |
| 3 | C5 -- parse().unwrap() | 2 hrs | One corrupt row crashes everything |
| 4 | C3 -- Silent event discard | 1 hr | Undermines audit/compliance core feature |
| 5 | C7 -- PgVector no-op | 1-2 days | PostgreSQL recall completely broken |
