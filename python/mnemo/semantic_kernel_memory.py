"""Microsoft Semantic Kernel integration for Mnemo.

Provides a wrapper that connects Semantic Kernel agents to Mnemo's MCP
server via MCPStdioPlugin.

Example::

    import asyncio
    from mnemo.semantic_kernel_memory import MnemoSKPlugin

    async def main():
        mnemo = MnemoSKPlugin(db_path="agent.db")

        async with mnemo.create_plugin() as plugin:
            from semantic_kernel import Kernel
            kernel = Kernel()
            kernel.add_plugin(plugin)
            # Use kernel with ChatCompletionAgent

    asyncio.run(main())

Requires:
    pip install semantic-kernel
"""

from __future__ import annotations

from mnemo.mcp_config import MnemoMCPConfig


class MnemoSKPlugin:
    """Semantic Kernel integration for Mnemo MCP memory server.

    Creates an MCPStdioPlugin that connects to Mnemo,
    compatible with Semantic Kernel's Kernel.add_plugin().

    Args:
        db_path: Path to the DuckDB database file.
        agent_id: Default agent identifier.
        **kwargs: Additional arguments passed to MnemoMCPConfig.
    """

    def __init__(self, db_path: str = "mnemo.db", agent_id: str = "default", **kwargs):
        self._config = MnemoMCPConfig(db_path=db_path, agent_id=agent_id, **kwargs)

    def create_plugin(self):
        """Create a Semantic Kernel MCPStdioPlugin connected to Mnemo.

        Returns an async context manager. Use with ``async with``.

        Returns:
            MCPStdioPlugin instance (async context manager).
        """
        try:
            from semantic_kernel.connectors.mcp import MCPStdioPlugin
        except ImportError:
            raise ImportError(
                "semantic-kernel is required for MnemoSKPlugin. "
                "Install with: pip install semantic-kernel"
            )

        return MCPStdioPlugin(
            name="mnemo",
            description="Persistent memory database for AI agents",
            command=self._config.command,
            args=self._config.build_args(),
            env=self._config.build_env(),
        )
