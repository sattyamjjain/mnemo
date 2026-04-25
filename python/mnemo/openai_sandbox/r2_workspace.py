"""Cloudflare R2 workspace backend (v0.3.4 Task A3).

Cloudflare R2 is S3-API-compatible, so almost all of the storage
contract reuses :class:`S3Workspace`. The two differences this class
encodes are:

1. **Endpoint.** R2 lives under
   ``https://{account_id}.r2.cloudflarestorage.com`` instead of an AWS
   regional endpoint.
2. **Region.** R2 uses the magic literal ``"auto"`` everywhere.
   AWS-specific region selection is meaningless against R2.

Everything else — Sigv4 auth, virtual-host addressing, the manifest
shape, the Ed25519 signature contract — is inherited unchanged. This
keeps R2 a one-paragraph maintenance burden: when the parent class
gets a new feature, R2 inherits it for free.

See :doc:`/storage/workspace-backends` for the parity matrix.

Install
-------

::

    pip install mnemo[openai-sandbox-r2]

Construct
---------

::

    from mnemo.openai_sandbox.r2_workspace import CloudflareR2Workspace

    ws = CloudflareR2Workspace(
        bucket="agent-snapshots",
        account_id="abc123",            # Cloudflare account ID
        access_key_id="...",            # R2 access key
        secret_access_key="...",        # R2 secret access key
    )

The credentials are forwarded to ``boto3.client("s3", ...)``. R2
itself does not understand the AWS instance / role / SSO credential
providers — pass real keys (or set ``AWS_ACCESS_KEY_ID`` /
``AWS_SECRET_ACCESS_KEY`` env vars before construction).
"""

from __future__ import annotations

from typing import Any

from mnemo.openai_sandbox.s3_workspace import S3Workspace
from mnemo.openai_sandbox.spec import WorkspaceBackend


class CloudflareR2Workspace(S3Workspace):
    """R2-flavoured :class:`S3Workspace` — see module docstring."""

    backend_name: WorkspaceBackend = "r2"

    def __init__(
        self,
        bucket: str,
        *,
        account_id: str,
        access_key_id: str | None = None,
        secret_access_key: str | None = None,
        client: Any | None = None,
        key_prefix_root: str = "",
    ) -> None:
        if not account_id:
            raise ValueError("CloudflareR2Workspace: account_id is required")
        self._account_id = account_id
        self._access_key_id = access_key_id
        self._secret_access_key = secret_access_key
        super().__init__(
            bucket=bucket,
            client=client,
            key_prefix_root=key_prefix_root,
            endpoint_url=f"https://{account_id}.r2.cloudflarestorage.com",
            region="auto",
            addressing_style="virtual",
            signature_version="s3v4",
        )

    def _build_default_client(self) -> Any:
        """Bake R2 credentials into the auto-built boto3 client.

        ``S3Workspace._build_default_client`` honours endpoint /
        region / addressing — we additionally pass the access keys
        when the caller supplied them. This is the only thing the
        AWS path doesn't do (it relies on the standard credential
        chain), so it's the only piece worth overriding.
        """
        import boto3  # type: ignore[import-not-found]
        from botocore.config import Config  # type: ignore[import-not-found]

        return boto3.client(
            "s3",
            endpoint_url=self.endpoint_url,
            region_name=self.region,
            aws_access_key_id=self._access_key_id,
            aws_secret_access_key=self._secret_access_key,
            config=Config(
                signature_version=self.signature_version,
                s3={"addressing_style": self.addressing_style or "virtual"},
            ),
        )
