"""Pydantic AI integration for Mnemo.

Provides a wrapper that connects Pydantic AI agents to Mnemo's MCP server,
giving agents access to persistent memory tools via the toolsets parameter.

Example::

    import asyncio
    from mnemo.pydantic_ai_memory import MnemoPydanticToolset

    async def main():
        mnemo = MnemoPydanticToolset(db_path="agent.db")
        server = mnemo.create_server()

        from pydantic_ai import Agent
        agent = Agent("openai:gpt-4o", toolsets=[server])

        async with agent:
            result = await agent.run("Remember that the user prefers dark mode")
            print(result.output)

    asyncio.run(main())

Requires:
    pip install pydantic-ai
"""

from __future__ import annotations

from mnemo.mcp_config import MnemoMCPConfig


class MnemoPydanticToolset:
    """Pydantic AI integration for Mnemo MCP memory server.

    Creates an MCPServerStdio toolset that connects to Mnemo,
    compatible with Pydantic AI's Agent toolsets parameter.

    Args:
        db_path: Path to the DuckDB database file.
        agent_id: Default agent identifier.
        timeout: MCP server connection timeout in seconds.
        **kwargs: Additional arguments passed to MnemoMCPConfig.
    """

    def __init__(
        self,
        db_path: str = "mnemo.db",
        agent_id: str = "default",
        timeout: int = 30,
        **kwargs,
    ):
        self._config = MnemoMCPConfig(db_path=db_path, agent_id=agent_id, **kwargs)
        self._timeout = timeout

    def create_server(self):
        """Create a Pydantic AI MCPServerStdio connected to Mnemo.

        Returns:
            MCPServerStdio instance to pass to Agent's toolsets parameter.
        """
        try:
            from pydantic_ai.mcp import MCPServerStdio
        except ImportError:
            raise ImportError(
                "pydantic-ai is required for MnemoPydanticToolset. "
                "Install with: pip install pydantic-ai"
            )

        return MCPServerStdio(
            self._config.command,
            args=self._config.build_args(),
            env=self._config.build_env(),
            timeout=self._timeout,
        )
