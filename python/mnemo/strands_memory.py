"""AWS Strands Agents integration for Mnemo.

Provides a wrapper that connects Strands Agents to Mnemo's MCP server,
giving agents access to persistent memory tools via MCPClient.

Example::

    from mnemo.strands_memory import MnemoStrandsClient

    mnemo = MnemoStrandsClient(db_path="agent.db")
    mcp_client = mnemo.create_client()

    with mcp_client:
        from strands import Agent
        agent = Agent(tools=mcp_client.list_tools_sync())
        response = agent("Remember that the user prefers dark mode")

Requires:
    pip install strands-agents strands-agents-tools
"""

from __future__ import annotations

from mnemo.mcp_config import MnemoMCPConfig


class MnemoStrandsClient:
    """Strands Agents integration for Mnemo MCP memory server.

    Creates an MCPClient that connects to Mnemo via stdio transport,
    compatible with Strands Agent's tools parameter.

    Args:
        db_path: Path to the DuckDB database file.
        agent_id: Default agent identifier.
        **kwargs: Additional arguments passed to MnemoMCPConfig.
    """

    def __init__(self, db_path: str = "mnemo.db", agent_id: str = "default", **kwargs):
        self._config = MnemoMCPConfig(db_path=db_path, agent_id=agent_id, **kwargs)

    def create_client(self):
        """Create a Strands MCPClient connected to Mnemo.

        Returns:
            MCPClient instance (context manager).
        """
        try:
            from strands.tools.mcp import MCPClient
            from mcp import stdio_client, StdioServerParameters
        except ImportError:
            raise ImportError(
                "strands-agents is required for MnemoStrandsClient. "
                "Install with: pip install strands-agents strands-agents-tools"
            )

        server_params = StdioServerParameters(
            command=self._config.command,
            args=self._config.build_args(),
            env=self._config.build_env(),
        )

        return MCPClient(lambda: stdio_client(server_params))
