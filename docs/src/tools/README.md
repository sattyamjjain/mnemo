# MCP Tools Reference

Mnemo exposes 10 MCP tools via the `rmcp` framework. Each tool is available through STDIO transport when running the `mnemo` binary.

| Tool | Description |
|------|-------------|
| [mnemo.remember](./remember.md) | Store a new memory |
| [mnemo.recall](./recall.md) | Retrieve memories by semantic/text/exact search |
| [mnemo.forget](./forget.md) | Soft-delete, hard-delete, decay, consolidate, or archive memories |
| [mnemo.share](./share.md) | Grant other agents access to a memory |
| [mnemo.checkpoint](./checkpoint.md) | Create a named snapshot of agent memory state |
| [mnemo.branch](./branch.md) | Create a named branch from a checkpoint |
| [mnemo.merge](./merge.md) | Merge a branch back into main agent state |
| [mnemo.replay](./replay.md) | Replay events from a checkpoint forward |
| [mnemo.verify](./verify.md) | Verify hash chain integrity |
| [mnemo.delegate](./delegate.md) | Delegate permissions to another agent |
