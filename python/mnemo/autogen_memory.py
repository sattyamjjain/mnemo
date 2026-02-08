"""Microsoft AutoGen integration for Mnemo.

Provides a wrapper that connects AutoGen agents to Mnemo's MCP server,
giving agents access to persistent memory tools via McpWorkbench.

Example::

    import asyncio
    from mnemo.autogen_memory import MnemoAutoGenWorkbench

    async def main():
        mnemo = MnemoAutoGenWorkbench(db_path="agent.db")

        async with mnemo.create_workbench() as workbench:
            from autogen_agentchat.agents import AssistantAgent
            from autogen_ext.models.openai import OpenAIChatCompletionClient

            agent = AssistantAgent(
                "memory_agent",
                model_client=OpenAIChatCompletionClient(model="gpt-4o"),
                workbench=workbench,
            )
            from autogen_agentchat.ui import Console
            await Console(agent.run_stream(task="Remember that I prefer dark mode"))

    asyncio.run(main())

Requires:
    pip install autogen-agentchat "autogen-ext[openai,mcp]"
"""

from __future__ import annotations

from mnemo.mcp_config import MnemoMCPConfig


class MnemoAutoGenWorkbench:
    """AutoGen integration for Mnemo MCP memory server.

    Creates a McpWorkbench that connects to Mnemo via stdio transport,
    compatible with AutoGen's AssistantAgent workbench parameter.

    Args:
        db_path: Path to the DuckDB database file.
        agent_id: Default agent identifier.
        read_timeout: MCP read timeout in seconds.
        **kwargs: Additional arguments passed to MnemoMCPConfig.
    """

    def __init__(
        self,
        db_path: str = "mnemo.db",
        agent_id: str = "default",
        read_timeout: int = 60,
        **kwargs,
    ):
        self._config = MnemoMCPConfig(db_path=db_path, agent_id=agent_id, **kwargs)
        self._read_timeout = read_timeout

    def create_workbench(self):
        """Create an AutoGen McpWorkbench connected to Mnemo.

        Returns an async context manager. Use with ``async with``.

        Returns:
            McpWorkbench instance (async context manager).
        """
        try:
            from autogen_ext.tools.mcp import McpWorkbench, StdioServerParams
        except ImportError:
            raise ImportError(
                "autogen-ext[mcp] is required for MnemoAutoGenWorkbench. "
                "Install with: pip install 'autogen-ext[mcp]'"
            )

        server_params = StdioServerParams(
            command=self._config.command,
            args=self._config.build_args(),
            read_timeout_seconds=self._read_timeout,
        )

        return McpWorkbench(server_params)
