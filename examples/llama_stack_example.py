"""Example: Meta Llama Stack + Mnemo persistent memory.

Llama Stack agents connect to Mnemo via MCP toolgroup registration,
providing persistent memory for Llama-powered agents.

Requirements:
    pip install llama-stack-client
    cargo build --release -p mnemo-cli
    # Start Mnemo with REST API enabled:
    OPENAI_API_KEY=sk-... mnemo --rest-port 8080
    # Start Llama Stack server:
    llama stack run --model meta-llama/Llama-3.3-70B-Instruct
"""

from llama_stack_client import LlamaStackClient, Agent, AgentEventLogger
import uuid

# Connect to Llama Stack server
client = LlamaStackClient(base_url="http://localhost:8321")


def main():
    # Step 1: Register Mnemo as an MCP toolgroup
    # Mnemo must be running with REST API (--rest-port 8080)
    client.toolgroups.register(
        toolgroup_id="mcp::mnemo",
        provider_id="model-context-protocol",
        mcp_endpoint={"uri": "http://localhost:8080/sse"},
    )
    print("Registered Mnemo MCP toolgroup")

    # Step 2: Discover available models
    models = client.models.list()
    llm = next(
        m for m in models
        if m.custom_metadata and m.custom_metadata.get("model_type") == "llm"
    )
    print(f"Using model: {llm.id}")

    # Step 3: Create an agent with Mnemo memory tools
    agent = Agent(
        client,
        model=llm.id,
        instructions=(
            "You are a helpful assistant with persistent memory.\n"
            "Use the memory tools to store and recall information.\n"
            "Always check memory before answering questions about the user."
        ),
        tools=["mcp::mnemo"],
        max_infer_iters=5,
        sampling_params={
            "strategy": {"type": "top_p", "temperature": 0.7, "top_p": 0.95},
            "max_tokens": 2048,
        },
    )

    # Step 4: Create a session
    session_id = agent.create_session(session_name=f"memory-session-{uuid.uuid4().hex[:8]}")

    # Session 1: Store knowledge
    print("\n=== Store Knowledge ===")
    response = agent.create_turn(
        messages=[{
            "role": "user",
            "content": (
                "Remember these facts:\n"
                "1. The user is Bob, a machine learning engineer\n"
                "2. He works at Meta on the Llama team\n"
                "3. He prefers PyTorch over TensorFlow"
            ),
        }],
        session_id=session_id,
        stream=False,
    )
    print(f"Agent: {response}")

    # Session 2: Recall context
    print("\n=== Recall Context ===")
    response = agent.create_turn(
        messages=[{
            "role": "user",
            "content": "What ML framework does the user prefer?",
        }],
        session_id=session_id,
        stream=False,
    )
    print(f"Agent: {response}")


if __name__ == "__main__":
    main()
