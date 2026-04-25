# Cloudflare R2 workspace backend

Mnemo's [`MnemoSnapshotStore`](./openai-agents-ga.md) — the OpenAI
Agents SDK GA snapshot store — supports persisting workspace trees to
any S3-compatible object store. v0.3.4 ships
[`CloudflareR2Workspace`](https://github.com/sattyamjjain/mnemo/blob/main/python/mnemo/openai_sandbox/r2_workspace.py)
as a thin subclass of [`S3Workspace`](https://github.com/sattyamjjain/mnemo/blob/main/python/mnemo/openai_sandbox/s3_workspace.py)
so R2-backed snapshots inherit every feature of the AWS path
unchanged: signed manifests, Ed25519 verification, per-blob digest
checks, batched delete.

## Install

```bash
pip install 'mnemo[openai-sandbox-r2]'
```

The extra pulls `boto3>=1.34` and `cryptography>=42`. R2's S3 API is
wire-compatible with AWS SDK v4 signing, so no R2-specific client
library is needed.

## Quick start

```python
from mnemo.openai_sandbox.r2_workspace import CloudflareR2Workspace
from mnemo.openai_sandbox.manifest import WorkspaceSigner

signer = WorkspaceSigner.generate_ephemeral()  # or load yours from KMS

ws = CloudflareR2Workspace(
    bucket="agent-snapshots",
    account_id="abc123def456",   # R2 account ID
    access_key_id="...",         # R2 access key
    secret_access_key="...",     # R2 secret access key
)

spec = ws.save_workspace(
    workspace_root="/tmp/agent-state",
    signer=signer,
    workspace_id="run-2026-04-25-1",
    created_at="2026-04-25T00:00:00Z",
    key_prefix="agents/agent-1",
)

# `spec` carries backend="r2" — MnemoSnapshotStore dispatches via
# this field to keep the load path symmetric with save.
print(spec)
# RemoteSnapshotSpec(backend='r2', bucket='agent-snapshots',
#                    key_prefix='agents/agent-1', manifest_sha256='...')
```

## Differences from S3 in one line

| Knob | AWS S3 | Cloudflare R2 |
|:--|:--|:--|
| `endpoint_url` | regional default | `https://{account_id}.r2.cloudflarestorage.com` |
| `region` | `us-east-1` etc. | `"auto"` (literal) |
| Addressing | path or virtual | `"virtual"` |
| Signature | sigv4 | sigv4 |
| Credential providers | full AWS chain | access keys only |

`CloudflareR2Workspace` sets every R2-specific knob in its
constructor; nothing else in the snapshot path needs to know it's R2.

## Storage layout

Same as `S3Workspace`. One R2 object per file in the workspace plus
two top-level objects per snapshot:

```
<bucket>/<key_prefix>/manifest.json     # signed JSON manifest
<bucket>/<key_prefix>/manifest.sig      # detached Ed25519 signature
<bucket>/<key_prefix>/files/<rel_path>  # one per source file
```

Symlinks are recorded in the manifest (not as separate objects) so
the load path can recreate them after every regular file is fetched
+ verified. See [the OpenAI Agents GA integration page](./openai-agents-ga.md#manifest-shape)
for the manifest schema.

## Live-credential test

The Mnemo test suite runs a moto-S3 round-trip against
`CloudflareR2Workspace` on every CI build. To run a real R2 round-trip
locally, export:

```bash
export R2_ACCOUNT_ID=<account>
export R2_ACCESS_KEY_ID=<key>
export R2_SECRET_ACCESS_KEY=<secret>
export R2_BUCKET=<bucket>

pytest python/tests/test_r2_workspace.py::test_live_r2_round_trip -v
```

The test creates a small workspace tree, dumps it to R2 under the
`mnemo-tests/live-r2/` prefix, fetches it back, asserts file contents
match, and cleans up. Skipped silently when any of the four env vars
are unset.

## Cost note

R2's free tier is 10 GB storage + 1M Class-A operations / month + 10M
Class-B operations / month. A typical mnemo workspace snapshot is ~10
files at ~1 MB each, so a few thousand snapshots fit inside the free
tier — see [Cloudflare R2 pricing](https://developers.cloudflare.com/r2/pricing/).

R2 also has zero egress fees, which makes it a good fit for snapshot
restore traffic patterns (lots of reads on a bad day, very few on a
good one).

## Sources

* [Cloudflare R2 — pricing & API](https://developers.cloudflare.com/r2/pricing/)
* [OpenAI — next evolution of the Agents SDK](https://openai.com/index/the-next-evolution-of-the-agents-sdk/)
* [`r2_workspace.py` source](https://github.com/sattyamjjain/mnemo/blob/main/python/mnemo/openai_sandbox/r2_workspace.py)
