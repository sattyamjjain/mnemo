"""Example: DSPy + Mnemo persistent memory.

DSPy agents use Mnemo via plain Python tool functions within
a ReAct module, providing persistent memory for optimizable agents.

Requirements:
    pip install dspy mnemo
    export OPENAI_API_KEY=sk-...
"""

import dspy

from mnemo import MnemoClient

# Configure DSPy
lm = dspy.LM("openai/gpt-4o")
dspy.configure(lm=lm)

# Initialize Mnemo client
client = MnemoClient(db_path="dspy_demo.db", agent_id="dspy-agent")


# Define memory tools as plain functions (DSPy pattern)
def remember(content: str, tags: str = "") -> str:
    """Store information in persistent memory for later retrieval.

    Args:
        content: The information to remember.
        tags: Comma-separated tags for categorization.

    Returns:
        Confirmation with the memory ID.
    """
    tag_list = [t.strip() for t in tags.split(",")] if tags else None
    result = client.remember(content=content, tags=tag_list)
    return f"Stored memory: {result['id']}"


def recall(query: str) -> str:
    """Search persistent memory for relevant information.

    Args:
        query: Natural language search query.

    Returns:
        Matching memories as formatted text.
    """
    result = client.recall(query=query, limit=5)
    memories = result.get("memories", [])
    if not memories:
        return "No memories found matching the query."
    return "\n".join(
        f"[{m.get('score', 0):.2f}] {m.get('content', '')}" for m in memories
    )


def forget(memory_id: str) -> str:
    """Remove a specific memory by its ID.

    Args:
        memory_id: UUID of the memory to forget.

    Returns:
        Confirmation of deletion.
    """
    result = client.forget([memory_id])
    return f"Forgot: {result.get('forgotten', [])}"


def main():
    # Create a ReAct agent with memory tools
    agent = dspy.ReAct(
        "question -> answer: str",
        tools=[remember, recall, forget],
        max_iters=5,
    )

    # Session 1: Store knowledge
    print("=== Store Knowledge ===")
    result = agent(
        question="Remember that Alice is a Python developer at TechCorp "
        "who prefers functional programming."
    )
    print(f"Answer: {result.answer}\n")

    # Session 2: Recall context
    print("=== Recall Context ===")
    result = agent(question="What do you know about Alice's job?")
    print(f"Answer: {result.answer}\n")

    # Session 3: Complex reasoning
    print("=== Complex Reasoning ===")
    result = agent(
        question="Based on what you know about Alice, "
        "suggest a good Python framework for her."
    )
    print(f"Answer: {result.answer}")

    # Cleanup
    import os
    for f in ["dspy_demo.db", "dspy_demo.usearch", "dspy_demo.usearch.mappings.json"]:
        if os.path.exists(f):
            os.remove(f)


if __name__ == "__main__":
    main()
