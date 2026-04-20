"""OpenAI Agents SDK + Mnemo Session store — crash and resume demo.

This example shows how an agent conversation can survive a process crash
and resume from Mnemo-persisted state. The Agents SDK ``Session`` protocol
stores each turn, so we don't need an explicit checkpoint/replay step —
the next process just re-opens the session and keeps going.

Run::

    maturin develop  # inside python/, once
    export OPENAI_API_KEY=sk-...
    python examples/openai_agents_snapshot_example.py

Requires::

    pip install mnemo[openai-agents]
"""

from __future__ import annotations

import asyncio
import os
import sys
from pathlib import Path

from mnemo.openai_sessions import MnemoSessionStore


async def run_turn(session: MnemoSessionStore, user_msg: str) -> str:
    """Send one turn through the Agents SDK. Raises if the runtime is missing."""
    try:
        from agents import Agent, Runner  # type: ignore[import-not-found]
    except ImportError as exc:  # pragma: no cover
        raise SystemExit(
            "openai-agents is not installed. Run: pip install mnemo[openai-agents]"
        ) from exc

    agent = Agent(
        name="SupportBot",
        instructions=(
            "You are a support assistant. Keep answers short. "
            "Refer to earlier turns in the conversation when relevant."
        ),
    )
    result = await Runner.run(agent, user_msg, session=session)
    return str(result.final_output)


async def main() -> None:
    db = Path(os.environ.get("MNEMO_DB_PATH", "snapshot_demo.mnemo.db"))
    session_id = os.environ.get("MNEMO_SESSION_ID", "snapshot-demo")

    store = MnemoSessionStore(
        db_path=str(db),
        agent_id="snapshot-demo",
        session_id=session_id,
        openai_api_key=os.environ.get("OPENAI_API_KEY"),
    )

    step = int(sys.argv[1]) if len(sys.argv) > 1 else 1
    print(f"\n=== step {step} (session_id={session_id}) ===")
    prior = await store.get_items()
    print(f"prior turns: {len(prior)}")

    if step == 1:
        reply = await run_turn(store, "I can't log in. My email is alice@example.com.")
        print(f"assistant: {reply}")
        print("\nSimulating crash — exiting with code 42.")
        sys.exit(42)
    elif step == 2:
        reply = await run_turn(store, "What was the email I gave you?")
        print(f"assistant: {reply}")
        print("\nExpected: agent should reference alice@example.com from step 1.")
    elif step == 3:
        print("Clearing session …")
        await store.clear_session()
        remaining = await store.get_items()
        print(f"remaining turns after clear: {len(remaining)}")
    else:  # pragma: no cover
        raise SystemExit(f"unknown step: {step}")


if __name__ == "__main__":
    asyncio.run(main())
