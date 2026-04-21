"""OpenAI Agents SDK GA — crash / resume with S3-backed workspace.

Shows how Mnemo's ``MnemoSnapshotStore`` plus the new ``S3Workspace``
together let a GA-SDK worker crash at step 3 and resume from the last
signed snapshot on a second process.

Run::

    pip install "mnemo[openai-agents,openai-sandbox-s3]"
    export OPENAI_API_KEY=sk-...
    export AWS_ACCESS_KEY_ID=...
    export AWS_SECRET_ACCESS_KEY=...
    export MNEMO_BUCKET=my-agent-snapshots

    # Step 1: runs steps 1-2, writes a snapshot, then simulates a crash
    python examples/openai_agents_resume_s3_example.py 1

    # Step 2: picks up the snapshot, runs steps 3-5, finishes cleanly
    python examples/openai_agents_resume_s3_example.py 2
"""

from __future__ import annotations

import asyncio
import os
import sys
from pathlib import Path

import boto3

from mnemo.openai_sandbox import S3Workspace, WorkspaceSigner
from mnemo.openai_sessions_ga import MnemoSnapshotStore


def _signer() -> WorkspaceSigner:
    """Load a persistent Ed25519 key from disk; generate on first run."""
    path = Path(".mnemo-workspace-key")
    if path.exists():
        return WorkspaceSigner.from_secret_bytes(path.read_bytes())
    signer = WorkspaceSigner.generate_ephemeral()
    # Export raw private bytes for local persistence. In production, load
    # from an HSM/KMS instead.
    from cryptography.hazmat.primitives import serialization

    raw = signer._sk.private_bytes(  # noqa: SLF001
        encoding=serialization.Encoding.Raw,
        format=serialization.PrivateFormat.Raw,
        encryption_algorithm=serialization.NoEncryption(),
    )
    path.write_bytes(raw)
    return signer


async def run_step(step: int) -> None:
    db_path = os.environ.get("MNEMO_DB_PATH", "resume_s3.mnemo.db")
    session_id = os.environ.get("MNEMO_SESSION_ID", "resume-s3-demo")
    bucket = os.environ["MNEMO_BUCKET"]

    store = MnemoSnapshotStore(
        session_id=session_id,
        db_path=db_path,
        agent_id="resume-s3-demo",
        workspace_backend="local",  # Mnemo's own snapshot dir stays local
        workspace_root=Path("/tmp/mnemo-snapshot-meta"),
        openai_api_key=os.environ.get("OPENAI_API_KEY"),
    )
    s3 = S3Workspace(bucket=bucket, client=boto3.client("s3"))
    signer = _signer()

    workspace_dir = Path("/tmp/agent-scratch")
    workspace_dir.mkdir(parents=True, exist_ok=True)

    if step == 1:
        # Simulate agent writing tools output.
        (workspace_dir / "notes.md").write_text(
            "Step 1 and 2 output: opened a ticket.\n", encoding="utf-8"
        )
        spec = s3.save_workspace(
            workspace_root=workspace_dir,
            signer=signer,
            workspace_id="step-2-snapshot",
            created_at="2026-04-21T00:00:00Z",
            key_prefix=f"agents/{session_id}/step-2",
        )
        # The GA SDK treats the SnapshotRef as opaque; for this demo we
        # encode the RemoteSnapshotSpec into Mnemo's own SnapshotRef store.
        await store.save_snapshot(
            {"step": 2, "spec": spec.__dict__},
            {"cwd": str(workspace_dir)},
        )
        print("snapshot written. simulating crash with exit 42.")
        sys.exit(42)

    if step == 2:
        ref, run_state, _sandbox = await store.resume(from_ref="latest")
        spec = run_state["spec"]
        from mnemo.openai_sandbox.spec import RemoteSnapshotSpec

        spec_obj = RemoteSnapshotSpec(**spec)
        # Restore workspace
        restored = Path("/tmp/agent-scratch-restored")
        restored.mkdir(parents=True, exist_ok=True)
        s3.load_workspace(
            spec=spec_obj,
            workspace_root=restored,
            verifying_key_raw=signer.verifying_key_raw(),
        )
        notes = (restored / "notes.md").read_text(encoding="utf-8")
        print(f"resumed from {ref.as_uri()}; restored notes:\n{notes}")


def main() -> None:
    step = int(sys.argv[1]) if len(sys.argv) > 1 else 1
    asyncio.run(run_step(step))


if __name__ == "__main__":  # pragma: no cover
    main()
