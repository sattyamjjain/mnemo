"""Meta Llama Stack integration for Mnemo.

Provides helpers for registering Mnemo as an MCP toolgroup in Llama Stack,
giving agents access to persistent memory tools.

Example::

    from mnemo.llama_stack_memory import register_mnemo_toolgroup

    from llama_stack_client import LlamaStackClient
    client = LlamaStackClient(base_url="http://localhost:8321")

    # Register Mnemo's MCP server as a toolgroup
    register_mnemo_toolgroup(client, mcp_endpoint="http://localhost:8080/sse")

    # Use in an agent
    from llama_stack_client import Agent
    models = client.models.list()
    llm = next(m for m in models if m.custom_metadata
               and m.custom_metadata.get("model_type") == "llm")
    agent = Agent(client, model=llm.id, tools=["mcp::mnemo"])

Requires:
    pip install llama-stack-client
    # Mnemo must be running with REST API enabled (--rest-port)
"""

from __future__ import annotations


def register_mnemo_toolgroup(
    client,
    mcp_endpoint: str = "http://localhost:8080/sse",
    toolgroup_id: str = "mcp::mnemo",
):
    """Register Mnemo as an MCP toolgroup in Llama Stack.

    Llama Stack's MCP integration works via HTTP endpoints, so Mnemo
    must be running with its REST API enabled (``--rest-port``).

    Args:
        client: LlamaStackClient instance.
        mcp_endpoint: URL of Mnemo's MCP-over-SSE endpoint.
        toolgroup_id: Toolgroup ID to register under.
    """
    client.toolgroups.register(
        toolgroup_id=toolgroup_id,
        provider_id="model-context-protocol",
        mcp_endpoint={"uri": mcp_endpoint},
    )
