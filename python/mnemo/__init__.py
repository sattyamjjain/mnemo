"""Mnemo — MCP-native memory database for AI agents.

Provides a Python interface to Mnemo's memory operations:
REMEMBER, RECALL, FORGET, SHARE, CHECKPOINT, BRANCH, MERGE, REPLAY.

Example::

    from mnemo import MnemoClient

    client = MnemoClient(db_path="agent.mnemo.db")
    result = client.remember("The user prefers dark mode")
    memories = client.recall("user preferences")
"""

from mnemo._mnemo import MnemoClient

__all__ = ["MnemoClient"]
__version__ = "0.2.0"

# Optional framework integrations (fail gracefully if deps not installed)
try:
    from mnemo.checkpointer import ASMDCheckpointer

    __all__.append("ASMDCheckpointer")
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
