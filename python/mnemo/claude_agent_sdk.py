"""Claude Agent SDK integration for Mnemo.

Bridges Mnemo to the `claude-agent-sdk` Python package (Claude Opus 4.7's
Auto Memory + Auto Dream workflow). Two-layer integration:

1. **MCP-server surface** — exposes Mnemo's 10 MCP tools (remember, recall,
   forget, share, checkpoint, branch, merge, replay, delegate, verify) to
   the agent via the `mcp_servers` parameter on :class:`ClaudeAgentOptions`.

2. **Memory-file bridge** — optionally materializes relevant memories into
   Markdown files under ``memory_dir``. Claude Opus 4.7's Auto Memory reads
   and writes these files directly; a `watchdog` observer picks up those
   writes and calls :meth:`MnemoClient.remember` to persist them back into
   Mnemo, so the two views stay in sync.

Example::

    import asyncio
    from pathlib import Path
    from claude_agent_sdk import ClaudeSDKClient, ClaudeAgentOptions
    from mnemo.claude_agent_sdk import MnemoClaudeMemory

    async def main():
        async with MnemoClaudeMemory(
            db_path="agent.mnemo.db",
            agent_id="my-project",
            memory_dir=Path(".claude/memory"),
        ) as memory:
            memory.materialize(query="recent work", limit=25)
            memory.watch()  # start watchdog observer

            options = ClaudeAgentOptions(
                mcp_servers={"mnemo": memory.mcp_server_config},
                allowed_tools=[
                    "mcp__mnemo__recall",
                    "mcp__mnemo__remember",
                ],
            )
            async with ClaudeSDKClient(options=options) as client:
                await client.query("Summarize what I worked on yesterday.")

    asyncio.run(main())

Install::

    pip install mnemo[claude]
"""

from __future__ import annotations

import json
import shutil
import threading
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Optional

# Module-level marker so tests can detect whether watchdog is available.
# The real imports are only performed inside watch(); we keep typing permissive
# here to support environments where watchdog isn't installed.
_WATCHDOG_AVAILABLE: bool
try:  # pragma: no cover - import guard
    import watchdog  # noqa: F401

    _WATCHDOG_AVAILABLE = True
except ImportError:  # pragma: no cover
    _WATCHDOG_AVAILABLE = False


_FRONTMATTER_FENCE = "---"


@dataclass
class MemoryFile:
    """Parsed representation of a Markdown memory file."""

    id: Optional[str]
    importance: float
    tags: list[str]
    expires_at: Optional[str]
    extra: dict[str, Any] = field(default_factory=dict)
    body: str = ""

    def to_text(self) -> str:
        """Serialize back to Markdown with YAML-like frontmatter."""
        fm: dict[str, Any] = {}
        if self.id is not None:
            fm["id"] = self.id
        fm["importance"] = self.importance
        fm["tags"] = list(self.tags)
        if self.expires_at is not None:
            fm["expires_at"] = self.expires_at
        fm.update(self.extra)
        return _dump_frontmatter(fm) + "\n" + self.body


def _dump_frontmatter(data: dict[str, Any]) -> str:
    """Emit a minimal YAML-subset frontmatter block.

    Kept intentionally tiny so Mnemo does not take a hard dependency on PyYAML
    for this adapter. Only scalar/list values are supported.
    """
    lines = [_FRONTMATTER_FENCE]
    for key, value in data.items():
        if isinstance(value, list):
            lines.append(f"{key}: [{', '.join(json.dumps(v) for v in value)}]")
        else:
            lines.append(f"{key}: {json.dumps(value)}")
    lines.append(_FRONTMATTER_FENCE)
    return "\n".join(lines)


def parse_memory_file(text: str) -> MemoryFile:
    """Parse a Markdown memory file into a :class:`MemoryFile`.

    Accepts either the strict subset produced by :func:`_dump_frontmatter`
    or a more permissive YAML-ish format with ``key: value`` pairs.
    """
    lines = text.splitlines()
    if not lines or lines[0].strip() != _FRONTMATTER_FENCE:
        return MemoryFile(id=None, importance=0.5, tags=[], expires_at=None, body=text)

    end = None
    for idx in range(1, len(lines)):
        if lines[idx].strip() == _FRONTMATTER_FENCE:
            end = idx
            break
    if end is None:
        return MemoryFile(id=None, importance=0.5, tags=[], expires_at=None, body=text)

    fm: dict[str, Any] = {}
    for raw in lines[1:end]:
        if not raw.strip() or ":" not in raw:
            continue
        key, _, value = raw.partition(":")
        key = key.strip()
        value = value.strip()
        if value.startswith("[") and value.endswith("]"):
            inner = value[1:-1].strip()
            fm[key] = [
                json.loads(piece.strip()) if piece.strip() else ""
                for piece in inner.split(",")
                if piece.strip()
            ]
        else:
            try:
                fm[key] = json.loads(value)
            except json.JSONDecodeError:
                fm[key] = value

    body = "\n".join(lines[end + 1 :]).lstrip("\n")
    importance = fm.pop("importance", 0.5)
    try:
        importance = float(importance)
    except (TypeError, ValueError):
        importance = 0.5
    tags = fm.pop("tags", [])
    if not isinstance(tags, list):
        tags = []
    return MemoryFile(
        id=fm.pop("id", None),
        importance=importance,
        tags=list(tags),
        expires_at=fm.pop("expires_at", None),
        extra=fm,
        body=body,
    )


class MnemoClaudeMemory:
    """Claude Agent SDK integration for Mnemo.

    Args:
        db_path: Path to the DuckDB database file.
        agent_id: Default agent identifier.
        memory_dir: Directory where Markdown memory files are materialized.
            If ``None``, only the MCP surface is active (no file bridge).
        openai_api_key: Optional OpenAI API key for embeddings.
        embedding_model: Embedding model name.
        dimensions: Embedding dimensions.
        command: Path to the ``mnemo`` binary (auto-detected if not provided).
        project_tag: Optional tag prefix applied to materialized files, so
            multiple projects can share one DB without stomping on each other.
    """

    def __init__(
        self,
        db_path: str = "mnemo.db",
        agent_id: str = "default",
        memory_dir: Optional[Path] = None,
        openai_api_key: Optional[str] = None,
        embedding_model: str = "text-embedding-3-small",
        dimensions: int = 1536,
        command: Optional[str] = None,
        project_tag: Optional[str] = None,
    ) -> None:
        self.db_path = db_path
        self.agent_id = agent_id
        self.memory_dir = Path(memory_dir) if memory_dir is not None else None
        self.openai_api_key = openai_api_key
        self.embedding_model = embedding_model
        self.dimensions = dimensions
        self.command = command or shutil.which("mnemo") or "mnemo"
        self.project_tag = project_tag

        self._client = None
        self._observer = None
        self._suppress: set[Path] = set()
        self._suppress_lock = threading.Lock()

        if self.memory_dir is not None:
            self.memory_dir.mkdir(parents=True, exist_ok=True)

    # ------------------------------------------------------------------ MCP
    @property
    def mcp_server_config(self) -> dict[str, Any]:
        """Return the dict config passed into ``ClaudeAgentOptions.mcp_servers``.

        Claude Agent SDK accepts external stdio MCP servers in the form::

            {"type": "stdio", "command": ..., "args": [...], "env": {...}}
        """
        args = [
            "--db-path", self.db_path,
            "--agent-id", self.agent_id,
            "--embedding-model", self.embedding_model,
            "--dimensions", str(self.dimensions),
        ]
        if self.openai_api_key:
            args.extend(["--openai-api-key", self.openai_api_key])
        config: dict[str, Any] = {
            "type": "stdio",
            "command": self.command,
            "args": args,
        }
        return config

    # -------------------------------------------------------------- client
    def _ensure_client(self):
        if self._client is not None:
            return self._client
        try:
            from mnemo._mnemo import MnemoClient  # type: ignore[attr-defined]
        except ImportError as exc:  # pragma: no cover
            from mnemo.availability import MnemoClientUnavailable

            raise MnemoClientUnavailable(
                "MnemoClaudeMemory needs the native mnemo._mnemo extension"
            ) from exc
        self._client = MnemoClient(
            db_path=self.db_path,
            agent_id=self.agent_id,
            openai_api_key=self.openai_api_key,
            embedding_model=self.embedding_model,
            dimensions=self.dimensions,
        )
        return self._client

    # ----------------------------------------------------------- file ops
    def _file_path(self, memory_id: str) -> Path:
        assert self.memory_dir is not None, "memory_dir not configured"
        return self.memory_dir / f"{memory_id}.md"

    def _suppress_once(self, path: Path) -> None:
        with self._suppress_lock:
            self._suppress.add(path.resolve())

    def _should_suppress(self, path: Path) -> bool:
        with self._suppress_lock:
            resolved = path.resolve()
            if resolved in self._suppress:
                self._suppress.discard(resolved)
                return True
            return False

    def materialize(
        self,
        query: str = "",
        limit: int = 50,
        strategy: str = "auto",
    ) -> list[Path]:
        """Write the top-N recalled memories into ``memory_dir`` as ``.md`` files.

        Returns the list of paths written. Each file contains YAML-subset
        frontmatter with ``id``, ``importance``, ``tags``, ``expires_at`` and
        the memory content as the body.
        """
        if self.memory_dir is None:
            raise ValueError("memory_dir is not configured on this MnemoClaudeMemory instance")

        client = self._ensure_client()
        recall_query = query or (self.project_tag or self.agent_id)
        # For a listing-style recall we still pass a query (embedded server-side).
        result = client.recall(
            query=recall_query,
            limit=limit,
            strategy=strategy,
        )
        memories = result.get("memories", []) if isinstance(result, dict) else []

        written: list[Path] = []
        for memory in memories:
            mem_id = memory.get("id")
            if not mem_id:
                continue
            mf = MemoryFile(
                id=str(mem_id),
                importance=float(memory.get("importance", 0.5)),
                tags=list(memory.get("tags", [])),
                expires_at=memory.get("expires_at"),
                body=str(memory.get("content", "")),
            )
            path = self._file_path(str(mem_id))
            self._suppress_once(path)
            path.write_text(mf.to_text(), encoding="utf-8")
            written.append(path)
        return written

    def sync_file(self, path: Path) -> Optional[str]:
        """Persist an edited memory file back into Mnemo.

        Returns the memory id that was written (new or updated), or ``None``
        if the file was empty/unparsable.
        """
        if not path.exists():
            return None
        text = path.read_text(encoding="utf-8")
        if not text.strip():
            return None
        mf = parse_memory_file(text)
        if not mf.body.strip():
            return None

        client = self._ensure_client()
        tags = list(mf.tags)
        if self.project_tag and self.project_tag not in tags:
            tags.append(self.project_tag)

        # If the file carries an id and that memory still exists, re-remember
        # the content so the hash chain records the edit. Mnemo's engine does
        # not yet expose an in-place update via MnemoClient, so a second
        # remember is the honest bridge today.
        new = client.remember(
            content=mf.body,
            importance=mf.importance,
            tags=tags,
            memory_type="episodic",
        )
        return new["id"] if isinstance(new, dict) else None

    def delete_file(self, path: Path) -> Optional[str]:
        """Soft-delete the memory referenced by ``path``.

        The memory id is recovered from the filename (``<uuid>.md``). Returns
        the id that was forgotten or ``None`` if the filename is not a uuid.
        """
        stem = path.stem
        try:  # validate uuid shape without depending on `uuid` at import time
            import uuid

            uuid.UUID(stem)
        except (ValueError, TypeError):
            return None
        client = self._ensure_client()
        try:
            client.forget([stem])
        except Exception:  # pragma: no cover - forget may not exist on older builds
            return None
        return stem

    # ------------------------------------------------------------- watch
    def watch(self) -> None:
        """Start a background observer that mirrors edits/deletes into Mnemo.

        Requires the optional ``watchdog`` dependency. Raises ``RuntimeError``
        if watchdog is not installed.
        """
        if self.memory_dir is None:
            raise ValueError("memory_dir must be configured before calling watch()")
        if not _WATCHDOG_AVAILABLE:
            raise RuntimeError(
                "watchdog is required for MnemoClaudeMemory.watch(). "
                "Install with: pip install mnemo[claude]"
            )
        if self._observer is not None:
            return

        # Deferred import — only loaded when watch() is actually invoked.
        from watchdog.observers import Observer  # type: ignore[import-not-found]

        handler = _build_memory_dir_handler(self)
        observer = Observer()
        observer.schedule(handler, str(self.memory_dir), recursive=False)
        observer.daemon = True
        observer.start()
        self._observer = observer

    def unwatch(self) -> None:
        """Stop the background observer started by :meth:`watch`."""
        if self._observer is None:
            return
        self._observer.stop()
        self._observer.join(timeout=5)
        self._observer = None

    # ----------------------------------------------------- context manager
    def __enter__(self) -> "MnemoClaudeMemory":
        return self

    def __exit__(self, exc_type, exc_val, exc_tb) -> None:
        self.unwatch()

    async def __aenter__(self) -> "MnemoClaudeMemory":
        return self

    async def __aexit__(self, exc_type, exc_val, exc_tb) -> None:
        self.unwatch()


def _build_memory_dir_handler(owner: MnemoClaudeMemory):
    """Construct a watchdog handler bound to ``owner``.

    Built lazily inside :meth:`MnemoClaudeMemory.watch` so the module has no
    hard import dependency on ``watchdog``.
    """
    from watchdog.events import FileSystemEvent, FileSystemEventHandler  # type: ignore[import-not-found]

    class _MemoryDirHandler(FileSystemEventHandler):
        """Watchdog handler that funnels events into :class:`MnemoClaudeMemory`."""

        def _accept(self, event: FileSystemEvent) -> Optional[Path]:  # pragma: no cover
            if event.is_directory:
                return None
            path = Path(str(event.src_path))
            if path.suffix.lower() != ".md":
                return None
            return path

        def on_created(self, event: FileSystemEvent) -> None:  # pragma: no cover
            path = self._accept(event)
            if path is None:
                return
            if owner._should_suppress(path):
                return
            try:
                owner.sync_file(path)
            except Exception:
                pass

        def on_modified(self, event: FileSystemEvent) -> None:  # pragma: no cover
            path = self._accept(event)
            if path is None:
                return
            if owner._should_suppress(path):
                return
            try:
                owner.sync_file(path)
            except Exception:
                pass

        def on_deleted(self, event: FileSystemEvent) -> None:  # pragma: no cover
            path = self._accept(event)
            if path is None:
                return
            try:
                owner.delete_file(path)
            except Exception:
                pass

    return _MemoryDirHandler()


__all__ = [
    "MnemoClaudeMemory",
    "MemoryFile",
    "parse_memory_file",
]
