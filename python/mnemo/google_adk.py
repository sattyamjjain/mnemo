"""Google ADK (Agent Development Kit) integration for Mnemo.

Provides a wrapper that connects Google ADK agents to Mnemo's MCP server,
giving agents access to persistent memory tools.

Example::

    import asyncio
    from mnemo.google_adk import MnemoADKToolset

    async def main():
        mnemo = MnemoADKToolset(db_path="agent.db")
        toolset = mnemo.create_toolset()

        from google.adk.agents import LlmAgent
        agent = LlmAgent(
            model="gemini-2.0-flash",
            name="memory_agent",
            instruction="You have persistent memory. Use remember/recall tools.",
            tools=[toolset],
        )

    asyncio.run(main())

Requires:
    pip install google-adk
"""

from __future__ import annotations

from typing import Optional

from mnemo.mcp_config import MnemoMCPConfig


class MnemoADKToolset:
    """Google ADK integration for Mnemo MCP memory server.

    Creates a McpToolset that connects to Mnemo via stdio transport,
    compatible with Google ADK's LlmAgent.

    Args:
        db_path: Path to the DuckDB database file.
        agent_id: Default agent identifier.
        tool_filter: Optional list of tool names to expose (e.g. ["remember", "recall"]).
        **kwargs: Additional arguments passed to MnemoMCPConfig.
    """

    def __init__(
        self,
        db_path: str = "mnemo.db",
        agent_id: str = "default",
        tool_filter: Optional[list[str]] = None,
        **kwargs,
    ):
        self._config = MnemoMCPConfig(db_path=db_path, agent_id=agent_id, **kwargs)
        self._tool_filter = tool_filter

    def create_toolset(self):
        """Create a Google ADK McpToolset connected to Mnemo.

        Returns:
            McpToolset instance ready to pass to LlmAgent's tools parameter.
        """
        try:
            from google.adk.tools.mcp_tool.mcp_toolset import McpToolset
            from google.adk.tools.mcp_tool.mcp_session_manager import StdioConnectionParams
            from mcp.client.stdio import StdioServerParameters
        except ImportError:
            raise ImportError(
                "google-adk is required for MnemoADKToolset. "
                "Install with: pip install google-adk"
            )

        server_params = StdioServerParameters(
            command=self._config.command,
            args=self._config.build_args(),
            env=self._config.build_env(),
        )

        kwargs = {
            "connection_params": StdioConnectionParams(server_params=server_params),
        }
        if self._tool_filter:
            kwargs["tool_filter"] = self._tool_filter

        return McpToolset(**kwargs)
