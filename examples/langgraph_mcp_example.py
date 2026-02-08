"""Example: LangGraph + Mnemo via MCP tools.

LangGraph agents connect to Mnemo via langchain-mcp-adapters,
exposing all 10 memory tools as LangChain tools within a graph agent.

Requirements:
    pip install langgraph langchain-mcp-adapters langchain-openai
    cargo build --release -p mnemo-cli
    export OPENAI_API_KEY=sk-...
"""

import asyncio

from langchain_openai import ChatOpenAI
from langchain_mcp_adapters.client import MultiServerMCPClient
from langgraph.prebuilt import create_react_agent


async def main():
    model = ChatOpenAI(model="gpt-4o")

    # Connect to Mnemo via MCP stdio
    client = MultiServerMCPClient(
        {
            "mnemo": {
                "command": "mnemo",
                "args": ["--db-path", "langgraph_demo.db", "--agent-id", "langgraph-agent"],
                "transport": "stdio",
            },
        }
    )

    # Get all Mnemo tools as LangChain tools
    tools = await client.get_tools()
    print(f"Available tools: {[t.name for t in tools]}")

    # Create a ReAct agent with memory tools
    agent = create_react_agent(model, tools)

    # Session 1: Store knowledge
    print("\n=== Store Knowledge ===")
    result = await agent.ainvoke(
        {
            "messages": [
                {
                    "role": "user",
                    "content": (
                        "Remember these facts:\n"
                        "1. The user Alice is a Python developer\n"
                        "2. She works at Acme Corp\n"
                        "3. The project deadline is March 15th"
                    ),
                }
            ]
        }
    )
    print(f"Agent: {result['messages'][-1].content}\n")

    # Session 2: Recall and reason
    print("=== Recall and Reason ===")
    result = await agent.ainvoke(
        {
            "messages": [
                {"role": "user", "content": "What do you know about Alice's work?"}
            ]
        }
    )
    print(f"Agent: {result['messages'][-1].content}\n")

    # Session 3: Complex query
    print("=== Complex Query ===")
    result = await agent.ainvoke(
        {
            "messages": [
                {
                    "role": "user",
                    "content": "When is the deadline and who is working on it?",
                }
            ]
        }
    )
    print(f"Agent: {result['messages'][-1].content}")


# Custom StateGraph with memory tools
async def with_state_graph():
    from langgraph.graph import StateGraph, MessagesState, START
    from langgraph.prebuilt import ToolNode, tools_condition
    from langchain.chat_models import init_chat_model

    model = init_chat_model("openai:gpt-4o")

    client = MultiServerMCPClient(
        {"mnemo": {"command": "mnemo", "args": ["--db-path", "lg.db"], "transport": "stdio"}}
    )
    tools = await client.get_tools()

    def call_model(state: MessagesState):
        return {"messages": model.bind_tools(tools).invoke(state["messages"])}

    builder = StateGraph(MessagesState)
    builder.add_node("agent", call_model)
    builder.add_node("tools", ToolNode(tools))
    builder.add_edge(START, "agent")
    builder.add_conditional_edges("agent", tools_condition)
    builder.add_edge("tools", "agent")
    graph = builder.compile()

    result = await graph.ainvoke(
        {"messages": [{"role": "user", "content": "Remember I prefer dark mode"}]}
    )
    print(result["messages"][-1].content)


if __name__ == "__main__":
    asyncio.run(main())
