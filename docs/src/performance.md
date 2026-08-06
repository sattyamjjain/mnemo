# Performance

## Benchmarks

**Every published headline number — retrieval quality, poisoning resistance,
audit tamper-evidence — with its exact reproduction command and raw-results
file lives in one place: the [benchmark index](../benchmarks/index.md).** Start
there; each row also says what the number does *not* show.

Mnemo includes Criterion benchmarks in `benches/engine_bench.rs`:

```bash
cargo bench -p mnemo-core
```

## Retrieval Strategies

| Strategy | Speed | Quality | Best For |
|----------|-------|---------|----------|
| `exact` | Fastest | Filter-only | Known queries, tag-based |
| `bm25` | Fast | Good for keywords | Keyword search |
| `vector` | Medium | Best semantic | Semantic similarity |
| `graph` | Medium | Good for related | Connected memories |
| `hybrid` | Slowest | Best overall | General use (default) |

## Storage Backend Comparison

| Metric | DuckDB | PostgreSQL |
|--------|--------|------------|
| Latency (single op) | ~1ms | ~5ms |
| Throughput | High (local) | High (concurrent) |
| Memory usage | Low | Medium |
| Setup | Zero-config | Requires server |

## Optimization Tips

1. **Use noop embeddings** during development (faster, no API calls)
2. **Set appropriate limits** in recall to avoid over-fetching
3. **Use tags and filters** to narrow search space before semantic search
4. **Use exact strategy** when you know the filtering criteria
5. **Run decay passes** periodically to clean up low-importance memories
