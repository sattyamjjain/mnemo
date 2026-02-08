"""LangGraph MCP tools integration for Mnemo.

Provides a wrapper that connects LangGraph agents to Mnemo's MCP server,
exposing all 10 memory tools via MultiServerMCPClient.

This complements the existing ASMDCheckpointer (which provides state
persistence) by giving the agent direct access to memory tools.

Example::

    import asyncio
    from mnemo.langgraph_mcp import MnemoLangGraphTools

    async def main():
        mnemo = MnemoLangGraphTools(db_path="agent.db")
        client = mnemo.create_client()
        tools = await client.get_tools()

        from langgraph.prebuilt import create_react_agent
        from langchain_openai import ChatOpenAI
        agent = create_react_agent(ChatOpenAI(model="gpt-4o"), tools)
        result = await agent.ainvoke(
            {"messages": [{"role": "user", "content": "Remember I prefer dark mode"}]}
        )

    asyncio.run(main())

Requires:
    pip install langgraph langchain-mcp-adapters langchain-openai
"""

from __future__ import annotations

from mnemo.mcp_config import MnemoMCPConfig


class MnemoLangGraphTools:
    """LangGraph MCP integration for Mnemo.

    Creates a MultiServerMCPClient configured to connect to Mnemo's
    MCP server, exposing memory tools as LangChain tools.

    Args:
        db_path: Path to the DuckDB database file.
        agent_id: Default agent identifier.
        **kwargs: Additional arguments passed to MnemoMCPConfig.
    """

    def __init__(self, db_path: str = "mnemo.db", agent_id: str = "default", **kwargs):
        self._config = MnemoMCPConfig(db_path=db_path, agent_id=agent_id, **kwargs)

    def create_client(self):
        """Create a LangGraph MultiServerMCPClient connected to Mnemo.

        Returns:
            MultiServerMCPClient instance. Call ``await client.get_tools()``
            to retrieve LangChain-compatible tool objects.
        """
        try:
            from langchain_mcp_adapters.client import MultiServerMCPClient
        except ImportError:
            raise ImportError(
                "langchain-mcp-adapters is required for MnemoLangGraphTools. "
                "Install with: pip install langchain-mcp-adapters"
            )

        return MultiServerMCPClient(
            {
                "mnemo": {
                    "command": self._config.command,
                    "args": self._config.build_args(),
                    "transport": "stdio",
                },
            }
        )
