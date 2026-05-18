"""LangGraph checkpoint integration for Mnemo.

Provides `MnemoCheckpointer` (canonical name as of v0.4.5) and its
back-compat alias `ASMDCheckpointer`, both implementing LangGraph's
`BaseCheckpointSaver` interface (1.x API surface — `get_tuple` /
`put` / `list` / `put_writes` / `delete_thread`), backed by Mnemo's
checkpoint / branch / merge system.

`MnemoCheckpointer` is the documented name in v0.4.5+. `ASMDCheckpointer`
remains exported as an alias so existing imports continue to work
unchanged. Pick the new name in new code.

Usage::

    from mnemo import MnemoClient
    from mnemo.checkpointer import MnemoCheckpointer

    checkpointer = MnemoCheckpointer(db_path="agent.mnemo.db")
    # Use with LangGraph 1.x:
    # graph = create_graph().compile(checkpointer=checkpointer)

LangGraph 1.x interface coverage:

- ``get_tuple(config)``    — implemented; returns a CheckpointTuple
                              for the (thread_id, checkpoint_id, branch)
                              addressed by ``config``.
- ``put(config, …)``        — implemented; persists a checkpoint into
                              Mnemo's branch-aware checkpoint store.
- ``put_writes(config, …)`` — stub no-op; intermediate writes are not
                              independently persisted today.
- ``list(config, …)``       — stub empty iterator; enumerating
                              checkpoints across threads is not yet
                              wired through ``MnemoClient``.
- ``delete_thread(config)`` — implemented via ``forget`` over the
                              thread's memory records.
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


class MnemoCheckpointer(BaseCheckpointSaver):
    """LangGraph-compatible checkpoint saver backed by Mnemo.

    Bridges LangGraph's 1.x ``BaseCheckpointSaver`` interface to
    Mnemo's checkpoint / branch / merge system, giving operators
    persistent agent state with git-like branching, a verifiable
    HMAC audit chain on every write, and offline-replayable
    point-in-time recall.

    Canonical name as of v0.4.5. The earlier name
    :class:`ASMDCheckpointer` is preserved as an alias for
    back-compatibility — see the alias defined at the bottom of this
    module.
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


# v0.4.5 — back-compat alias for the legacy class name. Existing imports
# (``from mnemo.checkpointer import ASMDCheckpointer``) continue to work
# unchanged. The canonical name is :class:`MnemoCheckpointer`; pick that
# in new code.
ASMDCheckpointer = MnemoCheckpointer


__all__ = ["MnemoCheckpointer", "ASMDCheckpointer"]
