"""Claude Agent SDK + Mnemo — MCP + memory-file bridge demo.

Shows two things:

1. Attaching Mnemo as an MCP server via ``ClaudeAgentOptions.mcp_servers``.
2. Materializing recent memories into ``.claude/memory/*.md`` so Claude
   Opus 4.7's Auto Memory can read/write them directly, with a watchdog
   observer persisting edits back into Mnemo.

Run::

    maturin develop  # inside python/, once
    pip install mnemo-db[claude]
    python examples/claude_agent_sdk_example.py

Requires::

    pip install mnemo-db[claude] claude-agent-sdk
"""

from __future__ import annotations

import asyncio
import os
from pathlib import Path

from mnemo.claude_agent_sdk import MnemoClaudeMemory


async def main() -> None:
    memory_dir = Path(".claude/memory")
    db_path = os.environ.get("MNEMO_DB_PATH", "claude_demo.mnemo.db")

    async with MnemoClaudeMemory(
        db_path=db_path,
        agent_id="claude-demo",
        memory_dir=memory_dir,
        openai_api_key=os.environ.get("OPENAI_API_KEY"),
    ) as memory:
        written = memory.materialize(query="recent work", limit=25)
        print(f"materialized {len(written)} memory files under {memory_dir}")

        try:
            memory.watch()
            print("watchdog started — edit any .md under .claude/memory to sync back")
        except RuntimeError as exc:
            print(f"watch() unavailable: {exc}")

        try:
            from claude_agent_sdk import (  # type: ignore[import-not-found]
                ClaudeAgentOptions,
                ClaudeSDKClient,
            )
        except ImportError:
            print(
                "\n`claude-agent-sdk` not installed; skipping live agent run. "
                "Install with: pip install claude-agent-sdk"
            )
            return

        options = ClaudeAgentOptions(
            mcp_servers={"mnemo": memory.mcp_server_config},
            allowed_tools=[
                "mcp__mnemo__recall",
                "mcp__mnemo__remember",
            ],
        )
        async with ClaudeSDKClient(options=options) as client:
            await client.query(
                "Recall what I've been working on recently, summarize in two bullets."
            )
            async for msg in client.receive_response():
                print(msg)


if __name__ == "__main__":
    asyncio.run(main())
