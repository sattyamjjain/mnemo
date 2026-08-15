# Workspace backends (parity matrix)

The OpenAI Agents SDK GA snapshot store persists an agent's workspace
tree to object storage. Mnemo ships four backends. All four write the
**identical object layout** and share the **identical signing contract**,
so a snapshot is portable between providers by copying objects — nothing
in the manifest is provider-specific.

```text
<bucket-or-container>/<key_prefix>/manifest.json
<bucket-or-container>/<key_prefix>/manifest.sig
<bucket-or-container>/<key_prefix>/files/<rel_path>
```

## The four backends

| Backend | Class | Extra | Client | Implementation |
|---|---|---|---|---|
| AWS S3 | `S3Workspace` | `mnemo-db[openai-sandbox-s3]` | `boto3` | base class |
| Cloudflare R2 | `CloudflareR2Workspace` | `mnemo-db[openai-sandbox-r2]` | `boto3` | **subclasses** `S3Workspace` |
| Google Cloud Storage | `GCSWorkspace` | `mnemo-db[openai-sandbox-gcs]` | `google-cloud-storage` | standalone |
| Azure Blob | `AzureBlobWorkspace` | `mnemo-db[openai-sandbox-azure]` | `azure-storage-blob` | standalone |

### Why R2 subclasses and the other two do not

R2 speaks the S3 wire protocol, so it inherits the entire storage
contract and encodes only an endpoint and a region. That makes it a
one-paragraph maintenance burden.

GCS and Azure Blob are **not** S3-wire-compatible:

- **GCS** exposes an interoperability XML API that is a *partial* S3
  emulation. It does not cover the paginated `list_objects_v2` or batch
  `delete_objects` calls `S3Workspace` relies on, so subclassing would
  inherit methods that fail at runtime against a real bucket.
- **Azure Blob** shares no wire surface at all — different auth (shared
  key / SAS / Entra ID), different REST API, different pagination.

Both therefore use their provider's native client, as a single
standalone class each. There is deliberately **no abstract base layer**:
the genuinely shared logic (manifest construction, Ed25519 signing,
per-blob digest verification) already lives in
`mnemo.openai_sandbox.manifest`, and what remains per backend is a
~5-method object-store adapter. An ABC over five methods would add a
layer without removing duplication.

## One spec shape, four backends

`RemoteSnapshotSpec` has a single `bucket` field. Azure calls its
top-level namespace a **container**; that name goes in `bucket` anyway.

```python
RemoteSnapshotSpec(backend="azure", bucket="<container-name>", ...)
```

This is intentional. A provider-specific field would push a conditional
into every consumer of a spec; one field keeps `MnemoSnapshotStore`'s
dispatch to a single `spec.backend` lookup.

## Construction

```python
# AWS S3 — standard credential chain
from mnemo.openai_sandbox import S3Workspace
ws = S3Workspace(bucket="agent-snapshots")

# Cloudflare R2 — account ID + access keys
from mnemo.openai_sandbox import CloudflareR2Workspace
ws = CloudflareR2Workspace(
    bucket="agent-snapshots", account_id="abc123",
    access_key_id="...", secret_access_key="...",
)

# GCS — Application Default Credentials
from mnemo.openai_sandbox import GCSWorkspace
ws = GCSWorkspace(bucket="agent-snapshots")

# Azure Blob — connection string (also works against Azurite)
from mnemo.openai_sandbox import AzureBlobWorkspace
ws = AzureBlobWorkspace.from_connection_string(
    "DefaultEndpointsProtocol=...", container="agent-snapshots",
)
```

Every backend then exposes the same three methods: `save_workspace`,
`load_workspace`, `delete_workspace`.

## Provider behaviour worth knowing

| Behaviour | S3 / R2 | GCS | Azure Blob |
|---|---|---|---|
| Overwrite existing object | silent | silent | **rejects unless `overwrite=True`** |
| Prefix listing | explicit paginator | auto-paginated | auto-paginated |
| Batch delete | `delete_objects` (1000/call) | per blob | per blob |

The Azure row is the one that bites. `upload_blob` defaults to raising
`ResourceExistsError` on an existing name, so `AzureBlobWorkspace`
passes `overwrite=True` — without it, re-saving a workspace to the same
`key_prefix` would fail on the *second* attempt, making `save_workspace`
non-idempotent across retries. A regression test pins this.

## Integrity model (identical across backends)

1. `save_workspace` walks the tree, records a SHA-256 per file, builds
   `manifest.json`, and signs it with Ed25519.
2. The returned spec carries `manifest_sha256`.
3. `load_workspace` re-checks that digest against what the provider
   served **before** verifying the signature, then verifies every
   per-file digest while materialising the tree.

Step 2/3 is what catches tampering in the bucket even if an attacker
also re-signs the manifest with a rotated key. A tampered manifest fails
closed, and each backend has a test that mutates the stored manifest and
asserts the load raises.

## Testing

| Backend | Unit substrate | Live gate |
|---|---|---|
| S3, R2 | `moto` (in-memory S3) | `R2_ACCOUNT_ID` + `R2_ACCESS_KEY_ID` + `R2_SECRET_ACCESS_KEY` + `R2_BUCKET` |
| GCS | in-process fake client | `GCS_BUCKET` + Application Default Credentials |
| Azure | in-process fake container client | `AZURE_STORAGE_CONNECTION_STRING` + `AZURE_CONTAINER` |

GCS has no moto equivalent and its official emulator (`fake-gcs-server`)
is a Docker image; Azurite likewise needs Docker or npm. So those two use
small in-process fakes that implement only the handful of client methods
the backend actually calls — a new call reaching for an unfaked method
fails loudly rather than passing against a permissive mock. The live
gates are skipped by default and never run in CI.

The Azure live test works unmodified against **Azurite**, whose
connection string is well-known — the cheapest way to exercise the real
SDK path locally.
