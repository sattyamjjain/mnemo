# retention_conformance — processing-log retention proof

Offline, deterministic proof that mnemo's **append-only `agent_events` log**
survives every deletion / compaction / cold-tier path within a configurable
**retention floor**, scored by the shipped
[`mnemo_compliance::RetentionProfile`](../../crates/mnemo-compliance/src/retention.rs).

It is the sibling of [`bench/audit_conformance`](../audit_conformance) (which
proves the log is *tamper-evident*); this one proves the log is *retained*.

## What it drives

For each shipped deletion path, the harness seeds a chain (memory-write events +
traffic-bearing model-response events), snapshots the log, runs the path, and
verifies no in-floor event was dropped or rewritten and that traffic/processing
metadata was retained:

- `forget` — SoftDelete / HardDelete / Redact / Archive (with a real cold tier)
- `run_ttl_sweep` — hard-expiry of a past-due memory
- `run_decay_pass` — decay/archival housekeeping
- `run_consolidation` — cluster consolidation

Plus a **fail-loud backend gate**: `RetentionProfile::assert_backend_can_retain`
returns `ComplianceError::RetentionFloorUnsupported` (naming the backend) if the
active backend cannot guarantee an append-only log.

## Run

```bash
cargo run --release -p mnemo-retention-conformance-bench                       # DPDP, 365-day floor
cargo run --release -p mnemo-retention-conformance-bench -- --profile eu-ai-act-art19   # 180-day floor
cargo run --release -p mnemo-retention-conformance-bench -- --profile hipaa    # 6-year floor
```

Writes a byte-stable [`results/retention_conformance.md`](results/) (+ `.json`).
The contract is pinned by `tests/conformance.rs`.

## What this does and does NOT claim

- **Does:** the log — including traffic/processing metadata — is retained across
  the floor; every deletion edits *memory content* and *appends* an audit event.
- **Does NOT:** enforce a calendar clock, delete after the floor, or constitute
  legal compliance. It is a **conformance check for** the named obligation's
  retention mechanism — not a certification.
