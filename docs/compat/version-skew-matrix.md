# Mnemo Version Skew Matrix

> Updated 2026-05-03 for the v0.4.2 cut.

The matrix below pins which downstream and upstream versions are tested
together for each `mnemo` workspace release. Bumping any cell requires a
new workspace cut (the Cargo `workspace.package.version` is the
source-of-truth — see [U1 in CHANGELOG](../../CHANGELOG.md)).

## Current

| `mnemo` (Cargo workspace) | `rmcp` | `tantivy` | `usearch` | `pgvector` | `sqlx` | Python SDK (`mnemo-db`) | TypeScript SDK (`@mndfreek/mnemo-sdk`) | Go SDK (`mnemo.Version`) |
|---|---|---|---|---|---|---|---|---|
| **0.4.2** (2026-05-03) | 1.3 | 0.26 | 2.21 | 0.8.2 | 0.8 | 0.4.2 | 0.4.2 | 0.4.2 |

## History

| `mnemo` | `rmcp` | `tantivy` | `usearch` | `pgvector` | `sqlx` | Python | TypeScript | Go |
|---|---|---|---|---|---|---|---|---|
| 0.4.1 (2026-04-28) | 1.3 | 0.26 | 2.21 | 0.8.2 | 0.8 | 0.4.1 | 0.4.1 | 0.1.0 ¹ |
| 0.4.0 (2026-04-27) | 1.3 | 0.26 | 2.21 | 0.8.2 | 0.8 | 0.4.0 | 0.4.0 | 0.1.0 ¹ |
| 0.3.2 (2026-04-23) | 0.14 → 1.3 ² | 0.26 | 2.21 | 0.8.0 | 0.8 | 0.3.2 | 0.3.2 | 0.1.0 ¹ |

¹ Pre-v0.4.2 the Go SDK reported `clientInfo.version = "0.1.0"` on MCP
  initialize. v0.4.2 introduces a `Version` const that tracks the
  workspace, so older Go binaries pinning `0.1.0` are still compatible
  with `mnemo` ≥ 0.4.2 servers (the field is informational; the MCP
  protocol version is unchanged at `2024-11-05`).

² `mnemo` 0.3.2 shipped the `rmcp` 0.14 → 1.3 upgrade as a single
  release; consumers building against 0.3.x should pin `rmcp = "1.3"`
  to match.

## How to verify

CI enforces the matrix via two regression tests:

- `crates/mnemo-core/tests/version_metadata.rs` — fails if
  `env!("CARGO_PKG_VERSION")` drifts from the matrix's "Current" row.
- `python/tests/test_version_alignment.py` — fails if
  `mnemo.__version__` does not match the Cargo workspace version
  parsed at test time.

The TypeScript SDK's `package.json` `version` and the Go SDK's
`mnemo.Version` are checked at release time by the GitHub Actions
publish workflows (`.github/workflows/npm-publish.yml`,
`.github/workflows/cargo-publish.yml`).
