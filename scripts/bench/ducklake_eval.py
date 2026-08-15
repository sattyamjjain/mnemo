"""#41 Step 2 evaluation benchmark: DuckLake vs embedded DuckDB at 1M rows.

Regenerates docs/benchmarks/2026-08-15-ducklake-evaluation.md.

    pip install duckdb && python3 scripts/bench/ducklake_eval.py

Standalone on purpose: it measures the two STORAGE ENGINES, not mnemo's
wrapper, so it needs no workspace build. A `DuckLakeBackend` would add
identical trait overhead to both sides, so the RATIO is the meaningful
figure, not the absolute latency.

Uses the row count the issue specifies and a query mix chosen to be FAIR
to DuckLake, not just to mnemo's hot path:

  A. agent-scoped count      -> mnemo's hot path (partition-pruned)
  B. COUNT(*)                -> the operation the DuckLake 1.0 post
                                advertises 8x-258x speedups for
  C. wide columnar aggregate -> where Parquet's column pruning should win
  D. recent-window scan      -> time-partitioned range read
  E. point lookup by id      -> single-row fetch

Reported as p95 over repeated runs, after a warmup.
"""

import os
import tempfile
import time

import duckdb

N = 1_000_000
con = duckdb.connect()
con.execute("INSTALL ducklake"); con.execute("LOAD ducklake")

tmp = tempfile.mkdtemp(prefix="ducklake_bench_")
cat, data = os.path.join(tmp, "c.ducklake"), os.path.join(tmp, "d")
con.execute(f"ATTACH 'ducklake:{cat}' AS lake (DATA_PATH '{data}')")

DDL = """CREATE TABLE {t} (
    id VARCHAR, agent_id VARCHAR, content VARCHAR,
    created_at TIMESTAMP, tokens INTEGER, metadata VARCHAR)"""
con.execute(DDL.format(t="lake.memories"))
con.execute("ALTER TABLE lake.memories SET PARTITIONED BY (agent_id)")

plain = duckdb.connect(os.path.join(tmp, "plain.db"))
plain.execute(DDL.format(t="memories"))

GEN = f"""SELECT 'id-'||i, 'agent-'||(i%50), repeat('x',200),
 TIMESTAMP '2026-01-01' + INTERVAL (i%500) DAY, (i%4096),
 '{{"k":'||i||'}}' FROM range({N}) t(i)"""

t0 = time.perf_counter(); con.execute(f"INSERT INTO lake.memories {GEN}")
lw = time.perf_counter() - t0
t0 = time.perf_counter(); plain.execute(f"INSERT INTO memories {GEN}")
pw = time.perf_counter() - t0

print(f"duckdb {duckdb.__version__}  n={N:,}")
print(f"bulk write: lake={lw:.2f}s  plain={pw:.2f}s  "
      f"({(pw-lw)/pw*100:+.1f}% vs plain)\n")

QUERIES = {
    "A agent-scoped count": "SELECT count(*) FROM {t} WHERE agent_id='agent-7'",
    "B COUNT(*)":           "SELECT count(*) FROM {t}",
    "C columnar aggregate": "SELECT avg(tokens), max(tokens) FROM {t}",
    "D recent-window scan": ("SELECT count(*) FROM {t} WHERE created_at >= "
                             "TIMESTAMP '2026-06-01'"),
    "E point lookup":       "SELECT content FROM {t} WHERE id='id-999999'",
}


def p95(conn, sql, reps=30):
    conn.execute(sql).fetchall()  # warmup
    xs = []
    for _ in range(reps):
        t = time.perf_counter(); conn.execute(sql).fetchall()
        xs.append((time.perf_counter() - t) * 1000)
    xs.sort()
    return xs[max(0, int(len(xs) * 0.95) - 1)]


print(f"{'query':24} {'lake p95':>11} {'plain p95':>11} {'delta':>10}  verdict")
print("-" * 72)
wins = 0
for label, sql in QUERIES.items():
    lk = p95(con, sql.format(t="lake.memories"))
    pl = p95(plain, sql.format(t="memories"))
    d = (pl - lk) / pl * 100 if pl else 0.0
    if d >= 20:
        wins += 1
    verdict = "lake wins" if d >= 20 else ("~parity" if abs(d) < 20 else "lake LOSES")
    print(f"{label:24} {lk:>9.3f}ms {pl:>9.3f}ms {d:>+9.1f}%  {verdict}")

print("-" * 72)
print(f"queries meeting the issue's >=20% p95 gate: {wins}/{len(QUERIES)}")
