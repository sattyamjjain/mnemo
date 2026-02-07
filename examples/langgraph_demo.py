"""Demo: Using Mnemo as persistent memory for an AI agent workflow.

This example shows how Mnemo can serve as long-term memory storage
for AI agent frameworks. While this is a simplified simulation,
the same pattern applies to LangGraph, CrewAI, or any agent framework.
"""

from mnemo import MnemoClient


def simulate_agent_workflow():
    """Simulate an AI agent that learns and remembers across sessions."""

    client = MnemoClient(
        db_path="agent_memory.mnemo.db",
        agent_id="assistant-v1",
    )

    # Session 1: Agent learns about user
    print("=== Session 1: Learning ===")
    facts = [
        ("Alice is a senior Python developer at Acme Corp", ["user-info"], 0.9),
        ("Alice prefers functional programming patterns", ["preference", "coding"], 0.8),
        ("Alice's timezone is PST (UTC-8)", ["user-info"], 0.7),
        ("The project deadline is March 15th", ["project", "deadline"], 0.95),
    ]

    for content, tags, importance in facts:
        result = client.remember(content, tags=tags, importance=importance)
        print(f"  Learned: {content[:50]}... (id={result['id'][:8]})")

    # Session 2: Agent recalls relevant context
    print("\n=== Session 2: Recall for context ===")
    user_message = "Can you help me refactor this code?"

    # Agent retrieves relevant memories to build context
    context = client.recall("coding preferences and background", limit=3)
    print(f"  Retrieved {context['total']} relevant memories for context:")
    for mem in context["memories"]:
        print(f"    - {mem['content'][:60]}... (score={mem['score']:.2f})")

    # Session 3: Agent updates knowledge
    print("\n=== Session 3: Update knowledge ===")
    deadline_memories = client.recall("project deadline")
    if deadline_memories["total"] > 0:
        old_id = deadline_memories["memories"][0]["id"]
        # Forget old deadline
        client.forget([old_id])
        # Remember updated deadline
        client.remember(
            "The project deadline was extended to April 1st",
            tags=["project", "deadline"],
            importance=0.95,
        )
        print("  Updated project deadline")

    # Verify
    updated = client.recall("project deadline")
    print(f"  Current deadline info: {updated['memories'][0]['content']}")

    # Cleanup
    client.save_index()
    print(f"\n  Total memories: {client.index_size()}")

    import os

    for f in [
        "agent_memory.mnemo.db",
        "agent_memory.mnemo.usearch",
        "agent_memory.mnemo.usearch.mappings.json",
    ]:
        if os.path.exists(f):
            os.remove(f)

    print("\nDone!")


if __name__ == "__main__":
    simulate_agent_workflow()
