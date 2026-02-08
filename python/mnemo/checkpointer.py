"""LangGraph checkpoint integration for Mnemo.

Provides ASMDCheckpointer that implements LangGraph's BaseCheckpointSaver
interface, backed by Mnemo's checkpoint/branch/merge system.

Usage::

    from mnemo import MnemoClient
    from mnemo.checkpointer import ASMDCheckpointer

    checkpointer = ASMDCheckpointer(db_path="agent.mnemo.db")
    # Use with LangGraph:
    # graph = create_graph().compile(checkpointer=checkpointer)
"""

from __future__ import annotations

from typing import Any, Iterator, Optional, Sequence, Tuple

from langgraph.checkpoint.base import (
    BaseCheckpointSaver,
    ChannelVersions,
    Checkpoint,
    CheckpointMetadata,
    CheckpointTuple,
)

from mnemo import MnemoClient


class ASMDCheckpointer(BaseCheckpointSaver):
    """LangGraph-compatible checkpoint saver backed by Mnemo.

    This bridges LangGraph's checkpoint interface with Mnemo's
    checkpoint/branch/merge system, providing persistent state
    management with git-like branching.
    """

    def __init__(
        self,
        db_path: str = "mnemo.db",
        agent_id: str = "langgraph",
        **kwargs: Any,
    ) -> None:
        super().__init__()
        self.client = MnemoClient(db_path=db_path, agent_id=agent_id)

    def get_tuple(self, config: dict) -> Optional[CheckpointTuple]:
        """Get a checkpoint tuple for the given config."""
        thread_id = config.get("configurable", {}).get("thread_id", "default")
        checkpoint_id = config.get("configurable", {}).get("checkpoint_id")
        branch = config.get("configurable", {}).get("branch", "main")

        try:
            result = self.client.replay(
                thread_id=thread_id,
                checkpoint_id=checkpoint_id,
                branch_name=branch,
            )
        except Exception:
            return None

        cp = result["checkpoint"]
        return CheckpointTuple(
            config=config,
            checkpoint={
                "v": 1,
                "id": cp["id"],
                "ts": cp.get("created_at", ""),
                "channel_values": {},
                "channel_versions": {},
                "versions_seen": {},
            },
            metadata={"branch": cp.get("branch_name", "main")},
        )

    def put(
        self,
        config: dict,
        checkpoint: Checkpoint,
        metadata: CheckpointMetadata,
        new_versions: ChannelVersions,
    ) -> dict:
        """Save a checkpoint."""
        thread_id = config.get("configurable", {}).get("thread_id", "default")
        branch = config.get("configurable", {}).get("branch", "main")

        state = {
            "channel_values": checkpoint.get("channel_values", {}),
            "channel_versions": checkpoint.get("channel_versions", {}),
            "versions_seen": checkpoint.get("versions_seen", {}),
        }

        result = self.client.checkpoint(
            thread_id=thread_id,
            state_snapshot=state,
            branch_name=branch,
            label=metadata.get("step_type") if isinstance(metadata, dict) else None,
        )

        return {
            "configurable": {
                "thread_id": thread_id,
                "checkpoint_id": result["checkpoint_id"],
            }
        }

    def list(
        self,
        config: Optional[dict] = None,
        *,
        filter: Optional[dict[str, Any]] = None,
        before: Optional[dict] = None,
        limit: Optional[int] = None,
    ) -> Iterator[CheckpointTuple]:
        """List checkpoints, not fully supported — yields empty."""
        return iter([])

    def put_writes(
        self,
        config: dict,
        writes: Sequence[Tuple[str, Any]],
        task_id: str,
    ) -> None:
        """Store intermediate writes (no-op for now)."""
        pass

    def delete_thread(self, config: dict) -> None:
        """Delete all checkpoints and writes for the given thread."""
        thread_id = config.get("configurable", {}).get("thread_id", "default")
        try:
            self.client.forget([thread_id])
        except Exception:
            pass
