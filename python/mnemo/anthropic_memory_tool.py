"""Anthropic raw-API memory-tool 6-op server backed by Mnemo.

Implements the client-side handler contract for Anthropic's
`memory_20250818` tool — see
https://platform.claude.com/docs/en/docs/agents-and-tools/tool-use/memory-tool
— mapping the six commands (`view`, `create`, `str_replace`, `insert`,
`delete`, `rename`) onto Mnemo memories.

Each "file" in the memory tree is a single Mnemo `MemoryRecord`
tagged ``memorytool`` and ``path:<absolute-path>``. Directories are
implicit: they exist when at least one file lives under that prefix.
This keeps the audit log, hash chain, and ACL enforcement Mnemo
already provides — every tool call lands as a normal remember /
forget / update on the underlying engine.

Security
--------
The memory tool spec calls path-traversal "the most important security
control" for client-side handlers. This module enforces:

* Every path must start with the configured root (default ``/memories``).
* Paths are normalised (`os.path.normpath`) and the result must still
  start with the root.
* `..` segments and URL-encoded `%2e%2e` sequences are rejected before
  normalisation.
* Output is constrained to the specified return strings — the model
  cannot trick the handler into echoing arbitrary host paths because
  every response is built from the canonical memory-tree path, not
  from any host filesystem.

The tool itself is client-side per the spec, so all storage lives in
the Mnemo backend the operator hands us — not on disk by default.

Beta headers
------------
The basic ``memory_20250818`` surface needs no beta header. When the
caller is using the Managed Agents container, ``managed_agents_beta=True``
exposes the ``anthropic-beta: managed-agents-2026-04-01`` header
through :meth:`MnemoMemoryToolServer.beta_header`.
"""

from __future__ import annotations

import os
from typing import Any, Iterable, Protocol

# Spec-pinned constants — keep in sync with the memory-tool docs page.
TOOL_TYPE = "memory_20250818"
TOOL_NAME = "memory"
DEFAULT_ROOT = "/memories"
LINE_LIMIT = 999_999
TAG_MARKER = "memorytool"
PATH_TAG_PREFIX = "path:"
MANAGED_AGENTS_BETA = "managed-agents-2026-04-01"


class MemoryToolError(Exception):
    """Raised when a handler call would produce a tool_result with `is_error: True`.

    The wrapped string is the literal error text the spec demands —
    callers should pass it back to the model verbatim. We use an
    exception (rather than a status flag) so misuses surface as
    Python errors when the handler is invoked outside the `handle()`
    dispatcher.
    """


class _StoreLike(Protocol):
    """Minimal surface this module needs from a Mnemo client.

    Wrapped as a Protocol so tests can inject an in-memory fake — the
    real PyO3 ``MnemoClient`` already satisfies it. Method shapes
    match :class:`mnemo.MnemoClient` directly.
    """

    def remember(  # noqa: PLR0913 — mirrors MnemoClient surface
        self,
        content: str,
        memory_type: str | None = None,
        tags: list[str] | None = None,
        metadata: dict[str, Any] | None = None,
        thread_id: str | None = None,
    ) -> dict[str, Any]: ...

    def recall(
        self,
        query: str,
        limit: int | None = None,
        tags: list[str] | None = None,
    ) -> dict[str, Any]: ...

    def forget(
        self,
        memory_ids: list[str],
        strategy: str | None = None,
    ) -> dict[str, Any]: ...


class MnemoMemoryToolServer:
    """Handle Anthropic memory-tool calls against a Mnemo backend."""

    def __init__(
        self,
        client: _StoreLike,
        *,
        root: str = DEFAULT_ROOT,
        managed_agents_beta: bool = False,
        thread_id: str | None = None,
    ) -> None:
        if not root.startswith("/"):
            raise ValueError("root must be an absolute path starting with '/'")
        self._client = client
        self._root = _normalise(root)
        self._managed_agents_beta = managed_agents_beta
        self._thread_id = thread_id

    # ------------------------------------------------------------------
    # API-level surface — what callers wire into the Anthropic SDK
    # ------------------------------------------------------------------

    def tool_schema(self) -> dict[str, str]:
        """The dict to pass under ``tools=[...]`` in a Messages create.

        Anthropic's spec is: ``{"type": "memory_20250818", "name": "memory"}``.
        """
        return {"type": TOOL_TYPE, "name": TOOL_NAME}

    def beta_header(self) -> str | None:
        """Optional ``anthropic-beta`` header value.

        Returns ``None`` for the basic tool. When constructed with
        ``managed_agents_beta=True``, returns
        ``managed-agents-2026-04-01``. Callers attach via
        ``extra_headers={"anthropic-beta": server.beta_header()}``.
        """
        return MANAGED_AGENTS_BETA if self._managed_agents_beta else None

    def handle(self, tool_use: dict[str, Any]) -> dict[str, Any]:
        """Dispatch one tool_use block; return one tool_result block.

        Tolerates two input shapes: the bare ``input`` dict, or the
        full ``{"type": "tool_use", "id": ..., "name": ..., "input": {...}}``
        block. Returns the matching ``tool_result`` shape with the same
        ``tool_use_id`` echoed back.
        """
        if "input" in tool_use and "command" not in tool_use:
            command_input = dict(tool_use["input"])
            tool_use_id = tool_use.get("id")
        else:
            command_input = dict(tool_use)
            tool_use_id = None

        try:
            content = self._dispatch(command_input)
            is_error = False
        except MemoryToolError as exc:
            content = str(exc)
            is_error = True

        result: dict[str, Any] = {"type": "tool_result", "content": content}
        if tool_use_id is not None:
            result["tool_use_id"] = tool_use_id
        if is_error:
            result["is_error"] = True
        return result

    # ------------------------------------------------------------------
    # Command dispatcher
    # ------------------------------------------------------------------

    def _dispatch(self, ci: dict[str, Any]) -> str:
        command = ci.get("command")
        if command == "view":
            return self._view(ci.get("path", ""), ci.get("view_range"))
        if command == "create":
            return self._create(ci.get("path", ""), ci.get("file_text", ""))
        if command == "str_replace":
            return self._str_replace(
                ci.get("path", ""),
                ci.get("old_str", ""),
                ci.get("new_str", ""),
            )
        if command == "insert":
            return self._insert(
                ci.get("path", ""),
                int(ci.get("insert_line", 0)),
                ci.get("insert_text", ""),
            )
        if command == "delete":
            return self._delete(ci.get("path", ""))
        if command == "rename":
            return self._rename(ci.get("old_path", ""), ci.get("new_path", ""))
        raise MemoryToolError(f"Unknown command: {command!r}")

    # ------------------------------------------------------------------
    # Command handlers — exact return strings per spec
    # ------------------------------------------------------------------

    def _view(self, path: str, view_range: list[int] | None) -> str:
        canonical = self._validate_path(path)
        files = self._list_files()
        # Directory case: canonical equals an existing prefix or root.
        if canonical not in files:
            children = self._directory_children(files, canonical)
            if not children and canonical != self._root:
                raise MemoryToolError(
                    f"The path {path} does not exist. Please provide a valid path."
                )
            return _format_directory_listing(canonical, files, children)

        content = files[canonical]["content"]
        lines = content.split("\n")
        if len(lines) > LINE_LIMIT:
            raise MemoryToolError(
                f"File {path} exceeds maximum line limit of {LINE_LIMIT} lines."
            )
        if view_range:
            if (
                len(view_range) != 2
                or not all(isinstance(v, int) for v in view_range)
                or view_range[0] < 1
                or view_range[1] < view_range[0]
            ):
                raise MemoryToolError(
                    f"Error: Invalid `view_range`: {view_range!r}. Expected [start, end] with 1<=start<=end."
                )
            start, end = view_range
            visible = list(enumerate(lines[start - 1 : end], start=start))
        else:
            visible = list(enumerate(lines, start=1))
        return _format_file_view(canonical, visible)

    def _create(self, path: str, file_text: str) -> str:
        canonical = self._validate_path(path)
        if canonical == self._root:
            raise MemoryToolError(f"Error: {path} is the memory root, not a file")
        files = self._list_files()
        if canonical in files:
            raise MemoryToolError(f"Error: File {path} already exists")
        self._client.remember(
            content=file_text,
            memory_type="semantic",
            tags=[TAG_MARKER, f"{PATH_TAG_PREFIX}{canonical}"],
            metadata={"memory_tool": True, "path": canonical},
            thread_id=self._thread_id,
        )
        return f"File created successfully at: {path}"

    def _str_replace(self, path: str, old_str: str, new_str: str) -> str:
        canonical = self._validate_path(path)
        files = self._list_files()
        if canonical not in files:
            raise MemoryToolError(
                f"Error: The path {path} does not exist. Please provide a valid path."
            )
        record = files[canonical]
        content = record["content"]
        occurrences = _find_occurrence_lines(content, old_str)
        if not occurrences:
            return (
                f"No replacement was performed, old_str `{old_str}` did not appear "
                f"verbatim in {path}."
            )
        if len(occurrences) > 1:
            line_list = ", ".join(str(n) for n in occurrences)
            return (
                f"No replacement was performed. Multiple occurrences of old_str "
                f"`{old_str}` in lines: {line_list}. Please ensure it is unique"
            )
        new_content = content.replace(old_str, new_str, 1)
        self._rewrite(canonical, new_content, replacing=record["id"])
        snippet = _snippet_around(new_content, occurrences[0])
        return "The memory file has been edited.\n" + snippet

    def _insert(self, path: str, insert_line: int, insert_text: str) -> str:
        canonical = self._validate_path(path)
        files = self._list_files()
        if canonical not in files:
            raise MemoryToolError(f"Error: The path {path} does not exist")
        record = files[canonical]
        content = record["content"]
        lines = content.split("\n")
        if insert_line < 0 or insert_line > len(lines):
            raise MemoryToolError(
                f"Error: Invalid `insert_line` parameter: {insert_line}. "
                f"It should be within the range of lines of the file: [0, {len(lines)}]"
            )
        # Spec: insert AFTER `insert_line`. insert_line=0 means before line 1.
        new_lines = lines[:insert_line] + insert_text.split("\n") + lines[insert_line:]
        new_content = "\n".join(new_lines)
        self._rewrite(canonical, new_content, replacing=record["id"])
        return f"The file {path} has been edited."

    def _delete(self, path: str) -> str:
        canonical = self._validate_path(path)
        files = self._list_files()
        children = self._directory_children(files, canonical)
        if canonical in files:
            self._client.forget(memory_ids=[files[canonical]["id"]], strategy="hard_delete")
            return f"Successfully deleted {path}"
        if children:
            ids = []
            for child_name in children:
                child_path = _join(canonical, child_name)
                if child_path in files:
                    ids.append(files[child_path]["id"])
                else:
                    # Deeper subtree — sweep every descendant
                    for fp, rec in files.items():
                        if fp.startswith(child_path + "/"):
                            ids.append(rec["id"])
            if ids:
                self._client.forget(memory_ids=ids, strategy="hard_delete")
            return f"Successfully deleted {path}"
        raise MemoryToolError(f"Error: The path {path} does not exist")

    def _rename(self, old_path: str, new_path: str) -> str:
        old = self._validate_path(old_path)
        new = self._validate_path(new_path)
        files = self._list_files()
        if old not in files and not self._directory_children(files, old):
            raise MemoryToolError(f"Error: The path {old_path} does not exist")
        if new in files or self._directory_children(files, new):
            raise MemoryToolError(f"Error: The destination {new_path} already exists")
        if old in files:
            record = files[old]
            self._rewrite(new, record["content"], replacing=record["id"])
            return f"Successfully renamed {old_path} to {new_path}"
        # Directory rename — re-tag every descendant.
        prefix = old + "/"
        for fp, rec in list(files.items()):
            if fp.startswith(prefix):
                rel = fp[len(prefix) :]
                target = _join(new, rel)
                self._rewrite(target, rec["content"], replacing=rec["id"])
        return f"Successfully renamed {old_path} to {new_path}"

    # ------------------------------------------------------------------
    # Storage helpers
    # ------------------------------------------------------------------

    def _list_files(self) -> dict[str, dict[str, Any]]:
        """Return every memory-tool record keyed by canonical path.

        Recall is tag-filtered to ``memorytool`` so we don't sweep the
        whole agent's memory. Limit is high but bounded — operators
        should not pile millions of tool-files into one tree.
        """
        result = self._client.recall(
            query="",
            limit=10_000,
            tags=[TAG_MARKER],
        )
        out: dict[str, dict[str, Any]] = {}
        for mem in result.get("memories", []) if isinstance(result, dict) else []:
            tags = mem.get("tags", []) or []
            for t in tags:
                if t.startswith(PATH_TAG_PREFIX):
                    out[t[len(PATH_TAG_PREFIX) :]] = {
                        "id": mem.get("id"),
                        "content": mem.get("content", ""),
                    }
                    break
        return out

    def _directory_children(
        self,
        files: dict[str, dict[str, Any]],
        directory: str,
    ) -> list[str]:
        prefix = directory.rstrip("/") + "/"
        names: set[str] = set()
        for fp in files:
            if fp.startswith(prefix):
                rest = fp[len(prefix) :]
                names.add(rest.split("/", 1)[0])
        return sorted(names)

    def _rewrite(self, path: str, new_content: str, replacing: str | None) -> None:
        if replacing:
            self._client.forget(memory_ids=[replacing], strategy="hard_delete")
        self._client.remember(
            content=new_content,
            memory_type="semantic",
            tags=[TAG_MARKER, f"{PATH_TAG_PREFIX}{path}"],
            metadata={"memory_tool": True, "path": path},
            thread_id=self._thread_id,
        )

    # ------------------------------------------------------------------
    # Path validation
    # ------------------------------------------------------------------

    def _validate_path(self, path: str) -> str:
        if not isinstance(path, str) or not path:
            raise MemoryToolError(
                f"The path {path!r} does not exist. Please provide a valid path."
            )
        if "%2e%2e" in path.lower() or "%2f" in path.lower():
            raise MemoryToolError(
                f"The path {path} contains URL-encoded traversal sequences."
            )
        if ".." in path.split("/"):
            raise MemoryToolError(
                f"The path {path} contains parent-directory traversal segments."
            )
        canonical = _normalise(path)
        if canonical != self._root and not canonical.startswith(self._root + "/"):
            raise MemoryToolError(
                f"The path {path} resolves outside the memory root {self._root}."
            )
        return canonical


# ----------------------------------------------------------------------
# Pure helpers
# ----------------------------------------------------------------------


def _normalise(path: str) -> str:
    """POSIX-normalise an absolute path; strip trailing slash except root."""
    if not path.startswith("/"):
        path = "/" + path
    norm = os.path.normpath(path)
    return norm or "/"


def _join(parent: str, child: str) -> str:
    if parent.endswith("/"):
        parent = parent.rstrip("/") or "/"
    if parent == "/":
        return "/" + child
    return parent + "/" + child


def _find_occurrence_lines(content: str, needle: str) -> list[int]:
    if not needle:
        return []
    out: list[int] = []
    for idx, line in enumerate(content.split("\n"), start=1):
        if needle in line:
            out.append(idx)
    return out


def _format_directory_listing(
    directory: str,
    files: dict[str, dict[str, Any]],
    children: Iterable[str],
) -> str:
    """Render the directory listing per spec.

    Spec format:

        Here're the files and directories up to 2 levels deep in {path},
        excluding hidden items and node_modules:
        {size}\\t{path}
        {size}\\t{path}/{filename}
    """
    header = (
        f"Here're the files and directories up to 2 levels deep in "
        f"{directory}, excluding hidden items and node_modules:"
    )
    lines = [header]
    # Compute aggregate size of `directory` (sum of files at any depth).
    total = 0
    for fp, rec in files.items():
        if fp == directory or fp.startswith(directory.rstrip("/") + "/"):
            total += len(rec["content"])
    lines.append(f"{_human_size_bytes(total)}\t{directory}")
    for name in children:
        if name.startswith(".") or name == "node_modules":
            continue
        child_path = _join(directory, name)
        if child_path in files:
            size = len(files[child_path]["content"])
        else:
            size = sum(
                len(rec["content"])
                for fp, rec in files.items()
                if fp.startswith(child_path + "/")
            )
        lines.append(f"{_human_size_bytes(size)}\t{child_path}")
    return "\n".join(lines)


def _human_size_bytes(n: int) -> str:
    if n < 1024:
        # Format follows the spec example which uses sub-K sizes verbatim.
        return f"{n}"
    if n < 1024 * 1024:
        return f"{n / 1024:.1f}K"
    return f"{n / (1024 * 1024):.1f}M"


def _format_file_view(path: str, visible: list[tuple[int, str]]) -> str:
    header = f"Here's the content of {path} with line numbers:"
    body = "\n".join(f"{n:>6}\t{line}" for n, line in visible)
    return header + "\n" + body if body else header


def _snippet_around(content: str, line_no: int, radius: int = 3) -> str:
    lines = content.split("\n")
    start = max(0, line_no - 1 - radius)
    end = min(len(lines), line_no - 1 + radius + 1)
    out = []
    for idx in range(start, end):
        out.append(f"{idx + 1:>6}\t{lines[idx]}")
    return "\n".join(out)
