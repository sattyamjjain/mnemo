"""Basic Mnemo memory example: REMEMBER → RECALL → FORGET cycle."""

from mnemo import MnemoClient


def main():
    # Initialize client (uses noop embeddings without OPENAI_API_KEY)
    client = MnemoClient(
        db_path="example.mnemo.db",
        agent_id="example-agent",
    )

    # REMEMBER: Store some memories
    print("=== REMEMBER ===")
    m1 = client.remember(
        "The user's name is Alice and she prefers dark mode",
        tags=["user-preference"],
        importance=0.9,
    )
    print(f"Stored memory: {m1['id']}")

    m2 = client.remember(
        "Alice uses Python 3.12 for her main projects",
        tags=["user-preference", "tech-stack"],
        importance=0.7,
    )
    print(f"Stored memory: {m2['id']}")

    m3 = client.remember(
        "Team standup is at 9:30 AM every weekday",
        memory_type="procedural",
        tags=["schedule"],
        importance=0.6,
    )
    print(f"Stored memory: {m3['id']}")

    # RECALL: Search memories
    print("\n=== RECALL ===")
    result = client.recall("What are the user's preferences?", limit=5)
    print(f"Found {result['total']} memories:")
    for mem in result["memories"]:
        print(f"  [{mem['score']:.2f}] {mem['content']}")

    result = client.recall("programming language", tags=["tech-stack"])
    print(f"\nFiltered recall: {result['total']} memories")

    # FORGET: Remove a memory
    print("\n=== FORGET ===")
    forget_result = client.forget([m3["id"]])
    print(f"Forgotten: {forget_result['forgotten']}")

    # Verify it's gone
    result = client.recall("standup meeting schedule")
    print(f"After forget, found {result['total']} memories about standup")

    # Save index for next time
    client.save_index()
    print(f"\nIndex size: {client.index_size()} vectors")
    print("Done!")

    # Cleanup example database
    import os

    for f in ["example.mnemo.db", "example.mnemo.usearch", "example.mnemo.usearch.mappings.json"]:
        if os.path.exists(f):
            os.remove(f)


if __name__ == "__main__":
    main()
