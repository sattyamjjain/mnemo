# Anthropic memory-tool (`memory_20250818`)

Mnemo ships a client-side handler that satisfies Anthropic's
[memory-tool spec](https://platform.claude.com/docs/en/docs/agents-and-tools/tool-use/memory-tool)
for the `memory_20250818` surface. It maps the six tool commands
(`view`, `create`, `str_replace`, `insert`, `delete`, `rename`) onto
Mnemo's storage so the model gets a persistent "memory directory"
that's audited, hash-chained, and ACL-aware by default.

## Install

```bash
pip install 'mnemo-db[anthropic-memory-tool]'
```

The extra pulls `anthropic>=0.40` for SDK-driven integration. The
server itself does not import Anthropic at runtime, so handlers can
also be wired directly into raw API requests.

## Quick start

```python
from anthropic import Anthropic
from mnemo import MnemoClient, MnemoMemoryToolServer

# 1. Wire a Mnemo backend.
client = MnemoClient(db_path="memory.mnemo.db", agent_id="agent-1")

# 2. Construct the handler.
server = MnemoMemoryToolServer(client=client)

# 3. Register the tool with Anthropic.
anthropic = Anthropic()
extra_headers = {}
if server.beta_header():
    extra_headers["anthropic-beta"] = server.beta_header()

response = anthropic.messages.create(
    model="claude-opus-4-7",
    max_tokens=2048,
    tools=[server.tool_schema()],
    messages=[{"role": "user", "content": "Help me with my project."}],
    extra_headers=extra_headers or None,
)

# 4. When the model emits a `tool_use`, dispatch through the server.
for block in response.content:
    if block.type == "tool_use" and block.name == "memory":
        result = server.handle({
            "type": "tool_use",
            "id": block.id,
            "name": block.name,
            "input": block.input,
        })
        # Feed `result` back into the next `messages.create` call as a
        # `{"role": "user", "content": [{... tool_result ...}]}` block.
```

## Storage shape

Every "file" is one Mnemo `MemoryRecord` with two tags:

* `memorytool` — flags it as belonging to this surface.
* `path:/memories/...` — the canonical absolute path.

Directories are implicit. They exist when at least one file lives
under that prefix. `view` of a directory enumerates first-level
children; `delete` of a directory recursively forgets every record
under that prefix; `rename` re-writes every descendant under the
new prefix.

This means:

* Every file write lands in Mnemo's hash chain.
* `forget` propagates correctly — there is no separate file system
  to keep in sync.
* ACL enforcement is whatever the underlying `MnemoClient` is
  configured for. The handler doesn't bypass scope checks.

## Beta header

The basic surface needs no `anthropic-beta` header. When using the
[Managed Agents container](https://claude.com/blog/claude-managed-agents),
construct with `managed_agents_beta=True`:

```python
server = MnemoMemoryToolServer(client=client, managed_agents_beta=True)
extra_headers = {"anthropic-beta": server.beta_header()}
# server.beta_header() == "managed-agents-2026-04-01"
```

## Path-traversal safeguards

The spec calls path validation "the most important security control"
for client-side handlers. `MnemoMemoryToolServer` enforces:

* Every `path` (and `old_path` / `new_path`) must start with the
  configured root (default `/memories`).
* Paths are normalised with `os.path.normpath`; the result must
  still be under the root.
* Inputs containing `..` segments or URL-encoded `%2e%2e` / `%2f`
  sequences are rejected before normalisation.

Override the root with `MnemoMemoryToolServer(client=..., root="/other")`
if you need a different namespace.

## Return-string contract

All return values are spec-pinned. Tests in
`python/tests/test_anthropic_memory_tool.py` assert the exact strings
listed in the [memory-tool spec](https://platform.claude.com/docs/en/docs/agents-and-tools/tool-use/memory-tool):

* `view` of a directory: `Here're the files and directories up to 2 levels deep in {path} ...`
* `view` of a file: `Here's the content of {path} with line numbers:\n{6-char-right-padded line no}\\t{content}`
* `create` success: `File created successfully at: {path}`
* `create` duplicate: `Error: File {path} already exists`
* `str_replace` success: `The memory file has been edited.\n{snippet with line numbers}`
* `str_replace` no match: ``No replacement was performed, old_str `{old}` did not appear verbatim in {path}.``
* `str_replace` multi: ``No replacement was performed. Multiple occurrences of old_str `{old}` in lines: {a, b, ...}. Please ensure it is unique``
* `insert` success: `The file {path} has been edited.`
* `delete` success: `Successfully deleted {path}`
* `rename` success: `Successfully renamed {old} to {new}`

Errors are returned with `is_error: true` on the `tool_result`
block so the model can react.

## Sources

* [Anthropic — Memory tool docs](https://platform.claude.com/docs/en/docs/agents-and-tools/tool-use/memory-tool)
* [Anthropic — Claude Opus 4.7 release post](https://www.anthropic.com/news/claude-opus-4-7)
* [Anthropic — Effective context engineering for AI agents](https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents)
