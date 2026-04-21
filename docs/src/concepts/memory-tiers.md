# Memory tiers

Mnemo follows the Letta / MemGPT pattern of giving each memory record a
*tier* — a coarse class that tells the engine how to treat it for
expiry, decay, and consolidation. Four tiers are defined:

* **`Working`** — session-scoped. Auto-expires after
  `engine.ttl_working_seconds` (default 3600 s) when the caller doesn't
  supply an explicit `expires_at`. Treat this as the "scratchpad" tier
  for within-conversation working memory.
* **`Procedural`** — system prompts, tool definitions, decision-logic
  snippets. Importance is clamped on write to
  `engine.procedural_importance_floor` (default 0.8) so these never
  decay below recall visibility.
* **`Semantic`** — facts, user preferences, long-lived knowledge.
  Current default behaviour; no special handling beyond the normal
  decay / consolidation pipeline.
* **`Episodic`** — interaction logs. Carries `thread_id` /
  `session_id` as the scoping identifier; the prime target for the
  reflection pass's semantic dedup and stale archival phases.

## Shape in the data model (honest note)

**`MemoryTier` is a type alias for the existing `MemoryType` enum**, not
a separate schema field. v0.2.0's initial Task-8 spec called for a new
`tier: MemoryTier` field on `MemoryRecord`; we shipped the type-alias
shape instead because `MemoryType` already had the same four variants
— a redundant second column would have been churn with no runtime
benefit. The practical effect is identical: callers can pass `tier=` (a
`MemoryTier` value) to `remember` and the engine applies the per-tier
behaviour based on `memory_type`.

```rust
use mnemo_core::model::memory::{MemoryTier, MemoryType};
// These are the same type; MemoryTier is literally `pub type MemoryTier = MemoryType;`
let t: MemoryType = MemoryTier::Working;
```

Any downstream code that was relying on a separate `tier` field for v0.1.1
forward compatibility will compile against `memory_type`.

## Engine knobs

`MnemoEngine` exposes two builder methods for tuning tier behaviour:

```rust
let engine = MnemoEngine::new(...)
    .with_ttl_working_seconds(1800)           // 30-minute Working TTL
    .with_procedural_importance_floor(0.9);   // raise Procedural floor
```

The constants `DEFAULT_TTL_WORKING_SECONDS` and
`DEFAULT_PROCEDURAL_IMPORTANCE_FLOOR` are exported from
`mnemo_core::query` so callers can reference the shipping defaults.

## Recall semantics

All four tiers participate in the same recall pipeline
(`auto`/`vector_only`/`hybrid_rrf`/`graph_boosted`/`lexical`). Use
`tags=["tier:procedural"]` or the existing `memory_type` filter to
constrain to a specific tier; the engine does not currently apply a
recall-time boost for `Working` or cap Procedural to read-only.

## Out of scope

Letta's moving-between-tiers heuristics (Working → Semantic after N
accesses; Episodic → Semantic via reflection) are not implemented — the
v0.3.1 reflection pass only touches `Episodic` consolidation. A
tier-promotion pipeline is queued for v0.4.0.
