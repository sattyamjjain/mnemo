"""Agno (formerly PhiData) integration for Mnemo.

Provides a wrapper that connects Agno agents to Mnemo's MCP server,
giving agents access to persistent memory tools via stdio transport.

Example::

    import asyncio
    from mnemo.agno_memory import MnemoAgnoTools

    async def main():
        mnemo = MnemoAgnoTools(db_path="agent.db")

        async with mnemo.create_tools() as mcp_tools:
            from agno.agent import Agent
            from agno.models.openai import OpenAIChat
            agent = Agent(
                model=OpenAIChat(id="gpt-4o"),
                tools=[mcp_tools],
            )
            await agent.aprint_response("Remember that I prefer dark mode")

    asyncio.run(main())

Requires:
    pip install agno
"""

from __future__ import annotations

from mnemo.mcp_config import MnemoMCPConfig


class MnemoAgnoTools:
    """Agno integration for Mnemo MCP memory server.

    Creates an MCPTools instance that connects to Mnemo via stdio transport,
    compatible with Agno's Agent.

    Args:
        db_path: Path to the DuckDB database file.
        agent_id: Default agent identifier.
        **kwargs: Additional arguments passed to MnemoMCPConfig.
    """

    def __init__(self, db_path: str = "mnemo.db", agent_id: str = "default", **kwargs):
        self._config = MnemoMCPConfig(db_path=db_path, agent_id=agent_id, **kwargs)

    def create_tools(self):
        """Create an Agno MCPTools instance connected to Mnemo.

        Returns an async context manager. Use with ``async with``.

        Returns:
            MCPTools instance (async context manager).
        """
        try:
            from agno.tools.mcp import MCPTools
            from mcp import StdioServerParameters
        except ImportError:
            raise ImportError(
                "agno is required for MnemoAgnoTools. "
                "Install with: pip install agno"
            )

        server_params = StdioServerParameters(
            command=self._config.command,
            args=self._config.build_args(),
            env=self._config.build_env(),
        )

        return MCPTools(server_params=server_params)
