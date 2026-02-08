"""Shared MCP configuration for Mnemo integrations.

Provides a base helper that handles binary detection, CLI argument
construction, and environment setup for connecting any MCP-compatible
agent framework to Mnemo's stdio MCP server.
"""

from __future__ import annotations

import os
import shutil
from typing import Optional


class MnemoMCPConfig:
    """Configuration for connecting to Mnemo via MCP stdio transport.

    Encapsulates binary detection and CLI argument construction so that
    every framework integration shares the same setup logic.

    Args:
        db_path: Path to the DuckDB database file.
        agent_id: Default agent identifier.
        org_id: Optional organization identifier.
        openai_api_key: OpenAI API key for embeddings.
        embedding_model: Embedding model name.
        dimensions: Embedding dimensions.
        command: Path to the mnemo binary (auto-detected if not provided).
        encryption_key: AES-256-GCM key (64-char hex).
        postgres_url: PostgreSQL connection URL (switches backend).
        rest_port: Start REST API alongside MCP on this port.
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
        encryption_key: Optional[str] = None,
        postgres_url: Optional[str] = None,
        rest_port: Optional[int] = None,
    ):
        self.db_path = db_path
        self.agent_id = agent_id
        self.org_id = org_id
        self.openai_api_key = openai_api_key or os.environ.get("OPENAI_API_KEY")
        self.embedding_model = embedding_model
        self.dimensions = dimensions
        self.command = command or shutil.which("mnemo") or "mnemo"
        self.encryption_key = encryption_key
        self.postgres_url = postgres_url
        self.rest_port = rest_port

    def build_args(self) -> list[str]:
        """Build CLI argument list for the mnemo binary."""
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
        if self.encryption_key:
            args.extend(["--encryption-key", self.encryption_key])
        if self.postgres_url:
            args.extend(["--postgres-url", self.postgres_url])
        if self.rest_port:
            args.extend(["--rest-port", str(self.rest_port)])
        return args

    def build_env(self) -> dict[str, str]:
        """Build environment variables dict (inherits current env)."""
        env = dict(os.environ)
        if self.openai_api_key:
            env["OPENAI_API_KEY"] = self.openai_api_key
        return env
