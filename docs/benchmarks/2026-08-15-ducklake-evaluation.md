# DuckLake v1.0 opt-in backend — evaluation (issue #41, Step 2)

**Date:** 2026-08-15 · **Verdict: do not build Step 2 as specified.**

Issue [#41](https://github.com/sattyamjjain/mnemo/issues/41) set its own
acceptance gate: *"Do NOT flip default until Step 2 shows >= 20% p95 win."*
Measured at the issue's own specified scale (1M rows), DuckLake meets that
gate on **0 of 5** query shapes. It does not merely miss the bar — it is
**2.2×–10.7× slower** than the embedded DuckDB backend mnemo already ships.

Step 1 (the DuckDB version bump) is **done** and was worth doing: the
workspace is on `duckdb = "=1.10505.0"`, well past the 1.5.2 the issue
asked for.

## Result

DuckDB 1.5.5, n = 1,000,000, Apple M4, local filesystem. p95 over 30 runs
after warmup. `delta > 0` means DuckLake is faster.

| Query | DuckLake p95 | DuckDB p95 | Delta | Verdict |
|---|---|---|---|---|
| A — agent-scoped count (mnemo's hot path) | 2.995 ms | 0.461 ms | **−550%** | loses |
| B — `COUNT(*)` | 1.360 ms | 0.129 ms | **−958%** | loses |
| C — columnar aggregate (`avg`/`max`) | 2.135 ms | 0.355 ms | **−502%** | loses |
| D — recent-window scan | 4.514 ms | 0.385 ms | **−1073%** | loses |
| E — point lookup by id | 2.476 ms | 0.763 ms | **−225%** | loses |

**Queries meeting the ≥20% gate: 0/5.**

The single place DuckLake wins is **bulk write: +23.8%** (0.39 s vs 0.51 s
for 1M rows).

The query mix was chosen to be *fair to DuckLake*, not tuned to mnemo's
hot path: C is a wide columnar aggregate where Parquet's column pruning
should win, and B is the exact operation the DuckLake 1.0 announcement
advertises **8×–258×** speedups for. Both still lose.

## Why the announced speedups do not transfer

This is the crux, and it is not a defect in DuckLake.

DuckLake's published 8×–258× `COUNT(*)` numbers are measured **against
other lakehouse formats** (Iceberg, Delta) reading Parquet on **object
storage**, where the competition is a manifest-listing round trip. mnemo's
workload is the opposite corner of the design space: **embedded,
single-node, single-writer, local single-file**. Against a local `.db`,
DuckLake's catalog indirection and Parquet round-trips are pure added cost
with nothing to amortise them against.

Put plainly: DuckLake is faster than the thing it was designed to replace.
mnemo is not running that thing.

## What was verified as working

Feasibility is not the blocker — every DDL feature the issue names is
expressible today:

| Feature named in #41 | Status |
|---|---|
| `ATTACH 'ducklake:…'` catalog + `DATA_PATH` | works |
| Sorted / partitioned tables (`SET PARTITIONED BY (agent_id)`) | works |
| Partition by a time bucket | works via an expression (`year(created_at)`); a literal 7-day bucket needs an expression, not a bare keyword |
| Data inlining (`set_option('data_inlining_row_limit', …)`) | works |
| `VARIANT` for `metadata` | works; JSON round-trips intact |

So Step 2 is **buildable**. It is just not **worth building** for this
architecture right now.

## The cost side of the ledger

`StorageBackend` has **50 methods**. The two existing implementations are
1,853 lines (DuckDB) and 1,657 lines (PostgreSQL). A `DuckLakeBackend`
is therefore a ~1,500-line commitment plus a benchmark harness — to ship a
backend that is currently slower on every read shape mnemo performs.

## Recommendation

1. **Close #41 Step 2 as evaluated-and-declined.** Keep Step 1 (done).
2. **Re-open when the architecture changes**, specifically if mnemo gains
   any of: multi-writer concurrency, object-store-backed storage,
   cross-engine access to the same tables, or a time-travel / snapshot
   isolation requirement. Those are DuckLake's actual wins, and none of
   them is a property mnemo has today.
3. **If bulk-ingest throughput ever becomes the bottleneck**, revisit
   narrowly — write is the one axis where DuckLake led (+23.8%).

## Reproduce

The probe and benchmark are standalone Python (no workspace build
required — they measure the storage engines, not mnemo's wrapper):

```bash
pip install duckdb
python3 scripts/bench/ducklake_eval.py
```

## What this does **not** show

- It measures **raw SQL** against both engines, not calls through
  mnemo's `StorageBackend` trait. Building the trait impl would add
  identical overhead to both sides, so the ratio is the meaningful figure,
  not the absolute latency.
- It is **single-node, local-filesystem, single-writer**. It says nothing
  about DuckLake on S3/GCS with concurrent writers, which is the
  configuration DuckLake is built for and where it would very likely win.
- It does not test time travel, snapshot isolation, or cross-engine reads
  — capabilities mnemo does not currently expose and therefore cannot
  benefit from.
