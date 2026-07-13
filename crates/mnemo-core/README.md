# mnemo-core

[![crates.io](https://img.shields.io/crates/v/mnemo-core.svg)](https://crates.io/crates/mnemo-core)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](https://opensource.org/licenses/Apache-2.0)

Core engine of **[Mnemo](https://github.com/sattyamjjain/mnemo)** — an
on-prem, MCP-native, cryptographically-auditable memory database for AI agents.
This crate is the storage, data model, query engine, vector/full-text indexing,
encryption, and **tamper-evident hash-chain** substrate that the higher-level
crates build on.

It runs **in-process** (embedded DuckDB + USearch HNSW + Tantivy BM25) with no
hosted tier to trust. Every memory write and delete is a SHA-256 hash-chained
event, and an external verifier can detect any post-hoc mutation offline —
without consulting the store. That is the record-keeping substrate regulated AI
deployments need (EU AI Act Art.12, India DPDP, HIPAA §164.312(b)).

## Install

```bash
cargo add mnemo-core
```

## The audit-log verify API

The hash-chain verifier is a pure function over exported records — the store is
never consulted, so an auditor can check the log on their own machine:

```rust
use mnemo_core::hash::{verify_chain, ChainVerificationResult};
use mnemo_core::model::memory::MemoryRecord;

fn audit(records: &[MemoryRecord]) {
    let result: ChainVerificationResult = verify_chain(records);
    if result.valid {
        println!("chain OK — {}/{} records verify", result.verified_records, result.total_records);
    } else {
        // Names the first broken link — the record whose content or
        // chain hash no longer matches (i.e. was mutated after the fact).
        eprintln!("TAMPER at {:?}: {:?}", result.first_broken_at, result.error_message);
    }
}
```

`verify_event_chain` does the same for the append-only `agent_events` log. This
is exactly what the offline [`audit-conformance`](https://github.com/sattyamjjain/mnemo/tree/main/bench/audit_conformance)
bench drives to prove 100% single-byte-mutation detection.

## Optional features

- `onnx` — local ONNX sentence embeddings (no network).
- `s3` — S3 cold-storage tier.

## Positioning

Why on-prem + hash-chain-audited memory, and how it compares to Mem0 / Letta /
native provider memory on the compliance-audit axis:
**[docs/POSITIONING.md](https://github.com/sattyamjjain/mnemo/blob/main/docs/POSITIONING.md)**.

## License

Apache-2.0.
