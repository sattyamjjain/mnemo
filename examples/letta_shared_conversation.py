"""Two agents sharing a memory stream via Letta-style Conversations.

Run with:

    python examples/letta_shared_conversation.py

The example uses an in-process Mnemo backend so it doesn't need any
API keys or services. Swap ``MnemoClient(...)`` in for a production
backend (DuckDB or Postgres) once you're past the demo stage.
"""

from __future__ import annotations

from mnemo import MnemoClient
from mnemo.letta_adapter import MnemoLettaShared


def main() -> None:
    client = MnemoClient(db_path=":memory:", agent_id="orchestrator")
    shared = MnemoLettaShared(
        client=client,
        conversation_id="design-review-2026-04-25",
    )

    shared.attach("agent-architect")
    shared.attach("agent-reviewer")
    print("Participants:", shared.list_participants())

    shared.write(
        "Initial proposal: split the API into a v1 and v2 prefix.",
        source_agent_id="agent-architect",
    )
    shared.write(
        "Concern: deprecation timeline for v1 is unclear.",
        source_agent_id="agent-reviewer",
    )

    print("\nFull stream:")
    for m in shared.read():
        print(f"  [{m.source_agent_id}] {m.content}")

    print("\nReviewer-only:")
    for m in shared.read(from_agent="agent-reviewer"):
        print(f"  [{m.source_agent_id}] {m.content}")

    overlap = shared.overlapping_writes_within(seconds=300.0)
    if overlap:
        print(f"\n{len(overlap)} cross-participant write(s) inside the 5-minute window.")


if __name__ == "__main__":
    main()
