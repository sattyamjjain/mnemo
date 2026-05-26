//! Criterion-driven single-vector embed-latency target for every
//! discovered backend. Quality (nDCG@10 / recall@10) is computed
//! separately in `mnemo_embeddings_bench::run_all` and surfaced via
//! the CLI (`mnemo bench embeddings --slo-ms <N>`). Criterion here
//! owns only the wall-clock latency dimension; combining it with
//! the LLM-judge / GPT-judge arm (gated dataset) is deferred to
//! [#44](https://github.com/sattyamjjain/mnemo/issues/44).

use criterion::{Criterion, criterion_group, criterion_main};
use mnemo_core::embedding::EmbeddingProvider;
use mnemo_embeddings_bench::discover_backends;
use std::time::Duration;

const PROBE: &str = "how do relational engines support many readers without blocking each other";
const DIMENSIONS: usize = 384;

fn bench_embed_single(c: &mut Criterion) {
    let backends = discover_backends(DIMENSIONS);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    let mut group = c.benchmark_group("embed_single");
    // Network-bound backends (OpenAI) are slow; keep the per-bench
    // measurement window short and the sample count modest so the
    // total wall-clock budget stays sane in CI.
    group.measurement_time(Duration::from_secs(5));
    group.sample_size(10);

    for b in &backends {
        let Some(provider) = b.provider.as_ref() else {
            continue;
        };
        let label = b.kind.label();
        let p = provider.clone();
        group.bench_function(label, |bencher| {
            bencher.iter(|| {
                runtime.block_on(async {
                    let _ = (p.as_ref() as &dyn EmbeddingProvider).embed(PROBE).await;
                });
            });
        });
    }

    group.finish();
}

criterion_group!(benches, bench_embed_single);
criterion_main!(benches);
