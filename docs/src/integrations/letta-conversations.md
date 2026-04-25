# Letta Conversations-style shared memory

Letta's [Letta-Code release (2026-04-06)](https://www.letta.com/blog/letta-code)
introduced a Conversations API where multiple agents share a single
memory stream rather than each maintaining its own.

Mnemo v0.4.0-rc1 ships
[`MnemoLettaShared`](https://github.com/sattyamjjain/mnemo/blob/main/python/mnemo/letta_adapter.py)
— the same shape (`attach` / `detach` / `read` / `write` /
`list_participants`) backed by Mnemo memories rather than a remote
Letta service. That keeps shared state on Mnemo's audit log + hash
chain + ACL surface even when the agents are running through Letta's
orchestration.

## Install

`MnemoLettaShared` lives in the core `mnemo` package — no extra
needed:

```bash
pip install mnemo-db
```

## Quick start

```python
from mnemo import MnemoClient
from mnemo.letta_adapter import MnemoLettaShared

client = MnemoClient(db_path="conversation.mnemo.db", agent_id="orchestrator")
shared = MnemoLettaShared(
    client=client,
    conversation_id="design-review-2026-04-25",
)

shared.attach("agent-architect")
shared.attach("agent-reviewer")

shared.write(
    "Initial proposal: split the API into v1 / v2 prefixes.",
    source_agent_id="agent-architect",
)
shared.write(
    "Concern: deprecation timeline for v1 is unclear.",
    source_agent_id="agent-reviewer",
)

for msg in shared.read():
    print(f"[{msg.source_agent_id}] {msg.content}")
```

## Storage shape

* **Each shared message** is one Mnemo `MemoryRecord` with two tags:
  * `conversation:<id>` — every record in the conversation carries this.
  * `participant:<source_agent_id>` — the author.
* **Participants list** is a single Mnemo record tagged
  `conversation:<id>` + `meta:participants`, body = a JSON list of
  agent IDs. Updated on every `attach` / `detach`.

This keeps the conversation audit-log-replayable: every write is a
hash-chained Mnemo memory, every participant change is a discrete
write the operator can replay.

## Conflict policy

The adapter does **not** pre-resolve conflicts at write time. When
two participants `write` overlapping content within 60 seconds, both
records land in Mnemo and the existing
[`ResolutionStrategy::EvidenceWeighted`](../concepts/conflict-resolution.md)
scorer ranks them at recall time. Pre-resolving at write time would
amount to silently dropping one participant's contribution — the
exact failure mode shared memory is supposed to avoid.

To inspect cross-participant overlaps for operator review:

```python
for earlier, later in shared.overlapping_writes_within(seconds=60.0):
    print(f"{earlier.source_agent_id} → {later.source_agent_id}: "
          f"{earlier.content[:50]}... / {later.content[:50]}...")
```

## Read semantics

```python
# Full stream, time-ordered.
shared.read()

# Filter by author.
shared.read(from_agent="agent-reviewer")

# Forward a query through Mnemo's hybrid retrieval (vector + BM25).
shared.read(query="deprecation timeline", limit=10)
```

`read()` excludes the `meta:participants` metadata record
automatically, so callers see only real messages.

## Why a Mnemo-backed adapter rather than a Letta-API client

The blog post called for a `MnemoLettaShared` *adapter*, not a
*client*. The shape — `attach` / `detach` / `read` / `write` — is
useful by itself: any time multiple agents need a shared, audited,
queryable history, this adapter does the job without needing a Letta
account or API key. If you also use Letta's orchestrator, point its
agents at this adapter as their memory backend and the conversation
state is portable.

## Sources

* [Letta-Code release (2026-04-06)](https://www.letta.com/blog/letta-code)
* [Letta — Benchmarking AI Agent Memory](https://www.letta.com/blog/benchmarking-ai-agent-memory)
* [`letta_adapter.py` source](https://github.com/sattyamjjain/mnemo/blob/main/python/mnemo/letta_adapter.py)
* [Example: `examples/letta_shared_conversation.py`](https://github.com/sattyamjjain/mnemo/blob/main/examples/letta_shared_conversation.py)
