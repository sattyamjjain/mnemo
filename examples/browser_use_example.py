"""Example: BrowserUse + Mnemo persistent memory.

BrowserUse handles web browsing; Mnemo persists findings across sessions.
The agent browses the web and stores key information in memory.

Requirements:
    pip install browser-use langchain-openai mnemo
    cargo build --release -p mnemo-cli
    export OPENAI_API_KEY=sk-...
"""

import asyncio

from browser_use import Agent as BrowserAgent, Browser
from langchain_openai import ChatOpenAI

from mnemo import MnemoClient

# Initialize Mnemo for persistent storage
client = MnemoClient(db_path="browser_demo.db", agent_id="browser-agent")


async def browse_and_remember():
    """Browse the web and store findings in Mnemo."""

    browser = Browser()
    llm = ChatOpenAI(model="gpt-4o")

    # Step 1: Browse and gather information
    print("=== Browsing ===")
    agent = BrowserAgent(
        task="Go to github.com/sattyamjjain/mnemo and find the number of stars and the project description.",
        llm=llm,
        browser=browser,
    )
    result = await agent.run()
    findings = str(result)
    print(f"Browser found: {findings[:200]}...\n")

    # Step 2: Store findings in Mnemo
    print("=== Storing in Memory ===")
    memory = client.remember(
        content=f"Research findings from GitHub: {findings}",
        tags=["research", "github", "mnemo"],
        importance=0.8,
    )
    print(f"Stored memory: {memory['id']}\n")

    # Step 3: Later, recall findings without browsing again
    print("=== Recalling from Memory ===")
    recall = client.recall("GitHub project information", limit=5)
    for mem in recall.get("memories", []):
        print(f"  [{mem['score']:.2f}] {mem['content'][:100]}...")


async def multi_session_research():
    """Research across multiple sessions, persisting between them."""

    browser = Browser()
    llm = ChatOpenAI(model="gpt-4o")

    topics = [
        "AI agent memory systems comparison 2026",
        "MCP Model Context Protocol adoption statistics",
        "Rust in AI infrastructure projects",
    ]

    for topic in topics:
        print(f"\n=== Researching: {topic} ===")

        # Check if we already have info in memory
        existing = client.recall(topic, limit=1)
        if existing.get("memories") and existing["memories"][0].get("score", 0) > 0.7:
            print(f"  Found in memory: {existing['memories'][0]['content'][:100]}...")
            continue

        # Browse for new information
        agent = BrowserAgent(
            task=f"Search for '{topic}' and summarize the top 3 findings.",
            llm=llm,
            browser=browser,
            max_actions_per_step=4,
        )
        result = await agent.run(max_steps=10)

        # Store in Mnemo
        client.remember(
            content=f"Research on '{topic}': {str(result)}",
            tags=["research", "browsing"],
            importance=0.8,
        )
        print(f"  Stored new research findings")

    # Final summary from memory
    print("\n=== All Stored Research ===")
    all_research = client.recall("research findings", limit=10)
    for mem in all_research.get("memories", []):
        print(f"  [{mem['score']:.2f}] {mem['content'][:100]}...")


if __name__ == "__main__":
    asyncio.run(browse_and_remember())
