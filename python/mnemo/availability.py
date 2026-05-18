"""Availability probes for the Mnemo Python stack.

Use these when you need to decide *whether* to instantiate an adapter
before paying the import cost, or to surface an actionable error to
end users when the native PyO3 extension hasn't been built yet.

Exports
-------
``is_native_available()``
    Fast boolean probe — did ``mnemo._mnemo`` import cleanly?
``native_build_hint()``
    Human-readable message telling the user how to fix a missing native
    extension on their platform.
``installed_adapters()``
    Dict of adapter name → status (``"available"`` / ``"missing: <reason>"``)
    computed by probing every framework adapter listed in
    ``mnemo/__init__.py``.
``MnemoClientUnavailable``
    Typed exception raised by adapters when they try to instantiate a
    ``MnemoClient`` and the native extension isn't there. Replaces the
    opaque ``AttributeError: 'NoneType' ...`` the v0.3.0 adapters used
    to produce.
"""

from __future__ import annotations

import importlib
import importlib.util
import platform
import sys

__all__ = [
    "MnemoClientUnavailable",
    "is_native_available",
    "native_build_hint",
    "installed_adapters",
]


class MnemoClientUnavailable(RuntimeError):
    """Raised when an adapter tries to use the native `MnemoClient` but
    the PyO3 extension isn't importable.

    Carries the `native_build_hint()` message so callers can forward it
    to the user without re-computing it.
    """

    def __init__(self, reason: str | None = None) -> None:
        self.hint = native_build_hint()
        msg = reason or "mnemo native extension (_mnemo.*.so) not found"
        super().__init__(f"{msg}. {self.hint}")


def is_native_available() -> bool:
    """Return ``True`` when ``mnemo._mnemo`` imports; ``False`` otherwise.

    Cheap — uses `importlib.util.find_spec` without actually loading the
    module.
    """
    return importlib.util.find_spec("mnemo._mnemo") is not None


def native_build_hint() -> str:
    """Platform-aware hint explaining how to build the native extension."""
    python_tag = f"python{sys.version_info.major}.{sys.version_info.minor}"
    mach = platform.machine()
    system = platform.system()
    return (
        f"Build with `cd python && maturin develop --release` "
        f"(interpreter: {python_tag}, platform: {system}/{mach}). "
        f"Then re-import mnemo — the extension loads from "
        f"~/Library/Python/{sys.version_info.major}.{sys.version_info.minor}/... "
        f"or your active virtualenv's site-packages."
        if system == "Darwin"
        else f"Build with `cd python && maturin develop --release` "
        f"(interpreter: {python_tag}, platform: {system}/{mach}). "
        f"The compiled _mnemo.*.so lands in your active site-packages."
    )


# Names we probe for; each must match an entry in `mnemo/__init__.py`'s
# lazy try-except lattice.
_ADAPTER_MODULES: tuple[tuple[str, str], ...] = (
    ("MnemoClient", "mnemo._mnemo"),
    ("MnemoCheckpointer", "mnemo.checkpointer"),
    ("ASMDCheckpointer", "mnemo.checkpointer"),
    ("MnemoAgentMemory", "mnemo.openai_agents"),
    ("Mem0Compat", "mnemo.mem0_compat"),
    ("MnemoADKToolset", "mnemo.google_adk"),
    ("MnemoAgnoTools", "mnemo.agno_memory"),
    ("MnemoPydanticToolset", "mnemo.pydantic_ai_memory"),
    ("MnemoAutoGenWorkbench", "mnemo.autogen_memory"),
    ("MnemoSmolagentsTools", "mnemo.smolagents_memory"),
    ("MnemoStrandsClient", "mnemo.strands_memory"),
    ("MnemoSKPlugin", "mnemo.semantic_kernel_memory"),
    ("MnemoLangGraphTools", "mnemo.langgraph_mcp"),
    ("register_mnemo_toolgroup", "mnemo.llama_stack_memory"),
    ("create_mnemo_tools", "mnemo.dspy_tools"),
    ("create_mnemo_camel_tools", "mnemo.camel_memory"),
    ("MnemoClaudeMemory", "mnemo.claude_agent_sdk"),
    ("MnemoSessionStore", "mnemo.openai_sessions"),
    ("MnemoSnapshotStore", "mnemo.openai_sessions_ga"),
)


def installed_adapters() -> dict[str, str]:
    """Probe every adapter listed in ``mnemo/__init__.py`` and return a
    ``name -> status`` dict.

    Status is either ``"available"`` or ``"missing: <reason>"``. The
    reason carries the first line of the import error, which is usually
    enough to diagnose (e.g. ``"No module named 'agents'"`` for the
    openai-agents adapter on a vanilla install).
    """
    out: dict[str, str] = {}
    for name, module in _ADAPTER_MODULES:
        try:
            importlib.import_module(module)
        except Exception as exc:  # noqa: BLE001
            out[name] = f"missing: {type(exc).__name__}: {str(exc).splitlines()[0]}"
            continue
        out[name] = "available"
    return out


def doctor(stream=None) -> int:
    """Print a human-readable availability report.

    Returns 0 when the native extension is available and every *core*
    adapter imports cleanly (MnemoClient + MCP config). Returns 1
    otherwise. Opt-in adapters (OpenAI Agents, LangGraph, CrewAI, ...)
    missing only logs a notice — they're shipped behind extras.
    """
    out = stream if stream is not None else sys.stdout
    print("mnemo doctor", file=out)
    print("=" * 40, file=out)
    print(f"Python: {sys.version.split()[0]}", file=out)
    print(f"Platform: {platform.system()}/{platform.machine()}", file=out)
    native = is_native_available()
    print(f"Native (`mnemo._mnemo`): {'OK' if native else 'MISSING'}", file=out)
    if not native:
        print(f"  {native_build_hint()}", file=out)
    print(file=out)
    print("Adapter probe:", file=out)
    adapters = installed_adapters()
    width = max(len(n) for n in adapters)
    for name, status in sorted(adapters.items()):
        marker = "OK" if status == "available" else "--"
        print(f"  [{marker}] {name.ljust(width)}  {status}", file=out)
    core_ok = native and adapters.get("MnemoClient") == "available"
    return 0 if core_ok else 1
