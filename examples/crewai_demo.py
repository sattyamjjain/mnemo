"""Demo: Using Mnemo as shared memory for CrewAI-style agents.

This example shows how multiple agents can share memories through
Mnemo's ASMDMemory backend, enabling collaborative knowledge building.
"""

from mnemo.crewai_memory import ASMDMemory
import os


def simulate_crew_workflow():
    """Simulate a CrewAI-style multi-agent workflow with shared memory."""

    memory = ASMDMemory(db_path="crew_demo.mnemo.db", scope="shared")

    # Agent 1: Researcher gathers information
    print("=== Agent 1: Researcher ===")
    research_facts = [
        ("The target market size for AI agents is $50B by 2028", ["research", "market"]),
        ("Key competitors: LangChain, CrewAI, AutoGen", ["research", "competition"]),
        ("Enterprise customers prefer on-premise deployments", ["research", "preference"]),
    ]

    for content, tags in research_facts:
        result = memory.add(content, tags=tags, importance=0.8)
        print(f"  Stored: {content[:50]}...")

    # Agent 2: Analyst retrieves and builds on shared knowledge
    print("\n=== Agent 2: Analyst ===")
    market_info = memory.search("market size and competition", limit=5)
    print(f"  Found {len(market_info)} relevant memories:")
    for mem in market_info:
        print(f"    - {mem['content'][:60]}...")

    # Analyst adds insights based on shared research
    memory.add(
        "Given $50B TAM and enterprise preference, focus on hybrid cloud offering",
        tags=["analysis", "strategy"],
        importance=0.9,
    )
    print("  Added strategic insight")

    # Agent 3: Writer uses all shared knowledge
    print("\n=== Agent 3: Writer ===")
    all_context = memory.search("strategy and market", limit=10)
    print(f"  Building report from {len(all_context)} memories")
    for mem in all_context:
        print(f"    [{','.join(mem.get('tags', []))}] {mem['content'][:60]}...")

    # Cleanup
    for f in [
        "crew_demo.mnemo.db",
        "crew_demo.mnemo.usearch",
        "crew_demo.mnemo.usearch.mappings.json",
    ]:
        if os.path.exists(f):
            os.remove(f)

    print("\nDone!")


if __name__ == "__main__":
    simulate_crew_workflow()
