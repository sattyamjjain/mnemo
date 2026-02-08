"""Hugging Face smolagents integration for Mnemo.

Provides a wrapper that connects smolagents CodeAgent/ToolCallingAgent
to Mnemo's MCP server via ToolCollection.

Example::

    from mnemo.smolagents_memory import MnemoSmolagentsTools

    mnemo = MnemoSmolagentsTools(db_path="agent.db")

    with mnemo.create_tool_collection() as tool_collection:
        from smolagents import CodeAgent, OpenAIServerModel
        agent = CodeAgent(
            tools=[*tool_collection.tools],
            model=OpenAIServerModel(model_id="gpt-4o"),
        )
        agent.run("Remember that the user prefers dark mode")

Requires:
    pip install 'smolagents[mcp]'
"""

from __future__ import annotations

from mnemo.mcp_config import MnemoMCPConfig


class MnemoSmolagentsTools:
    """Smolagents integration for Mnemo MCP memory server.

    Creates a ToolCollection from Mnemo's MCP server,
    compatible with smolagents CodeAgent and ToolCallingAgent.

    Args:
        db_path: Path to the DuckDB database file.
        agent_id: Default agent identifier.
        trust_remote_code: Whether to trust remote tool code.
        **kwargs: Additional arguments passed to MnemoMCPConfig.
    """

    def __init__(
        self,
        db_path: str = "mnemo.db",
        agent_id: str = "default",
        trust_remote_code: bool = True,
        **kwargs,
    ):
        self._config = MnemoMCPConfig(db_path=db_path, agent_id=agent_id, **kwargs)
        self._trust_remote_code = trust_remote_code

    def create_tool_collection(self):
        """Create a smolagents ToolCollection from Mnemo's MCP server.

        Returns a context manager. Use with ``with``.

        Returns:
            ToolCollection context manager.
        """
        try:
            from smolagents import ToolCollection
            from mcp import StdioServerParameters
        except ImportError:
            raise ImportError(
                "smolagents[mcp] is required for MnemoSmolagentsTools. "
                "Install with: pip install 'smolagents[mcp]'"
            )

        server_params = StdioServerParameters(
            command=self._config.command,
            args=self._config.build_args(),
            env=self._config.build_env(),
        )

        return ToolCollection.from_mcp(
            server_params,
            trust_remote_code=self._trust_remote_code,
        )
