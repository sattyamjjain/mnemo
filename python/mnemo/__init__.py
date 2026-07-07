"""Mnemo — MCP-native memory database for AI agents.

Provides a Python interface to Mnemo's memory operations:
REMEMBER, RECALL, FORGET, SHARE, CHECKPOINT, BRANCH, MERGE, REPLAY.

Example::

    from mnemo import MnemoClient

    client = MnemoClient(db_path="agent.mnemo.db")
    result = client.remember("The user prefers dark mode")
    memories = client.recall("user preferences")
"""

__version__ = "0.5.11"
__all__: list[str] = []

# The native PyO3 extension is optional at import time — users who only need
# the framework adapter surfaces (e.g. config builders, file bridges) can
# import submodules without having run `maturin develop`. Any adapter that
# actually requires MnemoClient will raise when it tries to instantiate it.
try:
    from mnemo._mnemo import MnemoClient  # type: ignore[attr-defined]

    __all__.append("MnemoClient")
except ImportError:
    MnemoClient = None  # type: ignore[assignment]

# Shared MCP configuration (always available)
from mnemo.mcp_config import MnemoMCPConfig

__all__.append("MnemoMCPConfig")

# Optional framework integrations (fail gracefully if deps not installed)
try:
    from mnemo.checkpointer import ASMDCheckpointer, MnemoCheckpointer

    __all__.extend(["ASMDCheckpointer", "MnemoCheckpointer"])
except ImportError:
    pass

try:
    from mnemo.openai_agents import MnemoAgentMemory

    __all__.append("MnemoAgentMemory")
except ImportError:
    pass

try:
    from mnemo.mem0_compat import Mem0Compat

    __all__.append("Mem0Compat")
except ImportError:
    pass

try:
    from mnemo.google_adk import MnemoADKToolset

    __all__.append("MnemoADKToolset")
except ImportError:
    pass

try:
    from mnemo.agno_memory import MnemoAgnoTools

    __all__.append("MnemoAgnoTools")
except ImportError:
    pass

try:
    from mnemo.pydantic_ai_memory import MnemoPydanticToolset

    __all__.append("MnemoPydanticToolset")
except ImportError:
    pass

try:
    from mnemo.autogen_memory import MnemoAutoGenWorkbench

    __all__.append("MnemoAutoGenWorkbench")
except ImportError:
    pass

try:
    from mnemo.smolagents_memory import MnemoSmolagentsTools

    __all__.append("MnemoSmolagentsTools")
except ImportError:
    pass

try:
    from mnemo.strands_memory import MnemoStrandsClient

    __all__.append("MnemoStrandsClient")
except ImportError:
    pass

try:
    from mnemo.semantic_kernel_memory import MnemoSKPlugin

    __all__.append("MnemoSKPlugin")
except ImportError:
    pass

try:
    from mnemo.langgraph_mcp import MnemoLangGraphTools

    __all__.append("MnemoLangGraphTools")
except ImportError:
    pass

try:
    from mnemo.llama_stack_memory import register_mnemo_toolgroup

    __all__.append("register_mnemo_toolgroup")
except ImportError:
    pass

try:
    from mnemo.dspy_tools import create_mnemo_tools

    __all__.append("create_mnemo_tools")
except ImportError:
    pass

try:
    from mnemo.camel_memory import create_mnemo_camel_tools

    __all__.append("create_mnemo_camel_tools")
except ImportError:
    pass

try:
    from mnemo.claude_agent_sdk import MnemoClaudeMemory

    __all__.append("MnemoClaudeMemory")
except ImportError:
    pass

try:
    from mnemo.openai_sessions import MnemoSessionStore

    __all__.append("MnemoSessionStore")
except ImportError:
    pass

try:
    from mnemo.openai_sessions_ga import MnemoSnapshotStore, SnapshotRef

    __all__.extend(["MnemoSnapshotStore", "SnapshotRef"])
except ImportError:
    pass

try:
    from mnemo.anthropic_memory_tool import MnemoMemoryToolServer

    __all__.append("MnemoMemoryToolServer")
except ImportError:
    pass

try:
    from mnemo.letta_adapter import MnemoLettaShared

    __all__.append("MnemoLettaShared")
except ImportError:
    pass
