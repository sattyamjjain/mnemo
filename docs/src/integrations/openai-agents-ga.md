# OpenAI Agents SDK — GA resume contract

The 2026-04-16 GA release of `openai-agents` generalised the preview
`Session` protocol into three cooperating interfaces:

* `SessionStore` — per-turn conversation items.
* `SnapshotStore` — durable `RunState` + `SandboxSessionState` blobs.
* `ResumeProvider` — locator layer for picking a `SnapshotRef` to
  resume from.

Mnemo ships adapters for both halves.

## `MnemoSessionStore` (chat history)

`python/mnemo/openai_sessions.py`. Stores each conversation turn as a
session-tagged episodic memory; survives process restarts.

```python
from mnemo.openai_sessions import MnemoSessionStore

session = MnemoSessionStore(
    db_path="agent.mnemo.db",
    agent_id="user-42",
    session_id="support-2026-04-20",
)
# Pass `session=session` to the Agents SDK `Runner`.
```

## `MnemoSnapshotStore` (run state)

`python/mnemo/openai_sessions_ga.py`. Persists `RunState` +
`SandboxSessionState` and lets the GA SDK resume crashed runs.

```python
from mnemo.openai_sessions_ga import MnemoSnapshotStore

store = MnemoSnapshotStore(
    session_id="support-2026-04-20",
    db_path="agent.mnemo.db",
    agent_id="user-42",
    workspace_backend="local",
    workspace_root="/var/mnemo/snapshots",
)

ref = await store.save_snapshot(run_state, sandbox_state)
# ... process crashes ...
ref, run, sandbox = await store.resume(from_ref="latest")
```

`SnapshotRef.as_uri()` returns a stable `snapshot://<session>/<ts>` URI
suitable for the forthcoming MCP resource exposure layer.

## Payload storage policy

* **Inline** — payloads at or below `inline_threshold_bytes` (default
  64 KiB) live in the Mnemo memory body as base64, with a SHA-256
  digest.
* **Offload** — larger payloads go to a pluggable `WorkspaceStorage`.
  Mnemo only keeps the locator + SHA-256; the `load_snapshot` path
  verifies the digest on every read.

## Workspace backends

* `local` — **shipped.** Writes under `workspace_root`.
* `s3` / `r2` / `gcs` / `azure` — **stubs in v0.3.1.** The
  `WorkspaceStorage` class raises `NotImplementedError` with a
  `NotImplementedError("…install mnemo[openai-sandbox-<backend>] …")`
  message. The v0.3.1 roadmap ships a real `aioboto3`-backed S3
  backend; R2/GCS/Azure follow once the `SnapshotSpec` shape stabilises
  in the GA SDK.

Install with `pip install mnemo[openai-agents]`.

## Example (crash / resume)

See `python/examples/openai_agents_snapshot_example.py` for a 3-step
agent that writes two snapshots, crashes before the final reply, and
resumes from the latest snapshot on the second process start. The
equivalent `openai_agents_resume_s3_example.py` ships with the v0.3.1
S3 backend.
