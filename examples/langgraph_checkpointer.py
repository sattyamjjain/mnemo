"""Five-line LangGraph 1.x checkpoint integration with Mnemo.

Pairs LangGraph's `StateGraph` with `MnemoCheckpointer` so every
state transition lands in Mnemo's checkpoint / branch / merge store
with the HMAC envelope chain + offline-replayable provenance the
rest of the mnemo surface already provides.

Run after `pip install mnemo-db[langgraph]` (matches the optional
extra registered in `python/pyproject.toml`).

The full LangGraph 1.x ``BaseCheckpointSaver`` coverage is documented
inside `mnemo/checkpointer.py` — primaries (`put` / `get_tuple` /
`delete_thread`) are real; `list` + `put_writes` are stubs today.
"""

from __future__ import annotations

from typing import TypedDict

# LangGraph 1.x — minimal smoke example. Replace the toy node with
# your real agent step in production.
from langgraph.graph import END, START, StateGraph  # type: ignore[import-not-found]

from mnemo.checkpointer import MnemoCheckpointer


class State(TypedDict):
    counter: int


def increment(state: State) -> State:
    return {"counter": state["counter"] + 1}


def main() -> None:
    # 1. Build a LangGraph state graph.
    graph = StateGraph(State)
    graph.add_node("increment", increment)
    graph.add_edge(START, "increment")
    graph.add_edge("increment", END)

    # 2. Compile with `MnemoCheckpointer` — every node transition
    #    persists a checkpoint into the local `agent.mnemo.db`.
    checkpointer = MnemoCheckpointer(db_path="agent.mnemo.db", agent_id="example-agent")
    app = graph.compile(checkpointer=checkpointer)

    # 3. Invoke; the `thread_id` in `configurable` scopes the
    #    checkpoint stream and lets future runs resume / replay.
    config = {"configurable": {"thread_id": "session-1", "branch": "main"}}
    final = app.invoke({"counter": 0}, config=config)
    print(f"final state: {final}")

    # 4. Each LangGraph thread maps to a mnemo `thread_id`; recall
    #    the latest checkpoint via the typed Mnemo API to verify
    #    the bridge wrote what the agent saw.
    tup = checkpointer.get_tuple(config)
    if tup is not None:
        print(f"latest checkpoint id: {tup.checkpoint['id']}")
        print(f"branch: {tup.metadata['branch']}")


if __name__ == "__main__":
    main()
