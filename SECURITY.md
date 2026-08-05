# Security Policy

Mnemo positions as on-prem, cryptographically-auditable memory for regulated AI
(EU AI Act Art.12, India DPDP, HIPAA §164.312(b)). A crate that makes those
claims should say how to report a problem privately. This is that policy.

## Supported versions

Mnemo is pre-1.0. Security fixes target the latest published 0.5.x release only.
Older versions do not receive backported fixes, so the fix for a report is to
upgrade to the newest release.

| Version | Supported |
|---|---|
| Latest 0.5.x on crates.io | Yes |
| Anything older | No, upgrade |

## Reporting a vulnerability

Report privately through GitHub private vulnerability reporting: open the
repository Security tab and choose "Report a vulnerability"
(<https://github.com/sattyamjjain/mnemo/security/advisories/new>). Do not open a
public issue or pull request for a suspected vulnerability.

Include the affected version or commit, which storage backend you are running
(see below), a reproduction, and the impact you observed.

## First response

This is a small project with one maintainer, so the honest commitment is a first
acknowledgement within 10 working days, not a same-day turnaround. If a report is
credible, the fix and a coordinated disclosure timeline are agreed with the
reporter from there.

## Supported backends, because that is security-relevant

The surface mnemo stands behind:

- Storage: embedded DuckDB (default) and PostgreSQL with pgvector. Both are
  covered by CI, including a live pgvector integration test.
- At-rest content encryption: AES-256-GCM, when an encryption key is configured.
- The hardened `mnemo mcp-server` boot path: manifest-gated capabilities, refusal
  to start when sensitive env vars leak in, and serve-time tool-catalog
  attestation.

Not part of the supported surface, and out of scope below:

- S3 cold storage (the `s3` feature) and the golem WASM vector provider
  (`crates/mnemo-golem-*`) are experimental and not hardened.
- Embedding backends (OpenAI, ONNX, Ollama) send text to whatever service the
  operator configures. That data path is the operator's to secure.
- The unqualified `mnemo` crate on crates.io is a different, unrelated project.
  Install `mnemo-core`, `mnemo-mcp` and `mnemo-postgres`.

## Out of scope

- Denial of service from unbounded local input or resource exhaustion under the
  operator's own configuration.
- Anything requiring host or filesystem access the threat model already assumes
  the operator controls. The manifest and keystore files gate every capability,
  so their permissions are the operator's responsibility.
- The benchmark harnesses under `bench/` and the example code.
- The feature-gated experimental surfaces listed above.
- Version lag on crates.io by itself. That is tracked by the drift guard and is a
  release problem, not a vulnerability.
