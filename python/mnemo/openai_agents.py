"""OpenAI Agents SDK integration for Mnemo.

Provides a wrapper that connects OpenAI Agents SDK to Mnemo's MCP server,
giving agents access to persistent memory tools (remember, recall, forget,
share, checkpoint, branch, merge, replay, verify, delegate).

Example::

    import asyncio
    from agents import Agent, Runner
    from mnemo.openai_agents import MnemoAgentMemory

    async def main():
        async with MnemoAgentMemory(db_path="agent.db") as memory:
            agent = Agent(
                name="MemoryAgent",
                instructions="You have access to persistent memory tools.",
                mcp_servers=memory.mcp_servers,
            )
            result = await Runner.run(agent, "Remember that the user likes dark mode")
            print(result.final_output)

    asyncio.run(main())

Requires:
    pip install mnemo[openai-agents]
"""

from __future__ import annotations

import shutil
from typing import Optional


class MnemoAgentMemory:
    """OpenAI Agents SDK integration for Mnemo MCP memory server.

    Spawns a Mnemo MCP server as a subprocess and exposes it as an MCP
    server that the OpenAI Agents SDK can connect to.

    Args:
        db_path: Path to the DuckDB database file.
        agent_id: Default agent identifier.
        org_id: Optional organization identifier.
        openai_api_key: OpenAI API key for embeddings (optional).
        embedding_model: Embedding model name.
        dimensions: Embedding dimensions.
        command: Path to the mnemo binary (auto-detected if not provided).
    """

    def __init__(
        self,
        db_path: str = "mnemo.db",
        agent_id: str = "default",
        org_id: Optional[str] = None,
        openai_api_key: Optional[str] = None,
        embedding_model: str = "text-embedding-3-small",
        dimensions: int = 1536,
        command: Optional[str] = None,
    ):
        self.db_path = db_path
        self.agent_id = agent_id
        self.org_id = org_id
        self.openai_api_key = openai_api_key
        self.embedding_model = embedding_model
        self.dimensions = dimensions
        self.command = command or shutil.which("mnemo") or "mnemo"
        self._server = None

    def _build_args(self) -> list[str]:
        args = [
            "--db-path", self.db_path,
            "--agent-id", self.agent_id,
            "--embedding-model", self.embedding_model,
            "--dimensions", str(self.dimensions),
        ]
        if self.org_id:
            args.extend(["--org-id", self.org_id])
        if self.openai_api_key:
            args.extend(["--openai-api-key", self.openai_api_key])
        return args

    def _create_server(self):
        try:
            from agents.mcp import MCPServerStdio
        except ImportError:
            raise ImportError(
                "openai-agents is required for MnemoAgentMemory. "
                "Install with: pip install mnemo[openai-agents]"
            )

        self._server = MCPServerStdio(
            params={
                "command": self.command,
                "args": self._build_args(),
            },
            name="mnemo",
        )
        return self._server

    @property
    def mcp_servers(self) -> list:
        """Return list of MCP servers for the OpenAI Agents SDK Agent constructor."""
        if self._server is None:
            self._create_server()
        return [self._server]

    async def __aenter__(self):
        if self._server is None:
            self._create_server()
        await self._server.__aenter__()
        return self

    async def __aexit__(self, exc_type, exc_val, exc_tb):
        if self._server is not None:
            await self._server.__aexit__(exc_type, exc_val, exc_tb)
