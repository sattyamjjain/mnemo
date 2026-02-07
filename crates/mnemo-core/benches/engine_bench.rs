use criterion::{criterion_group, criterion_main, Criterion};
use std::sync::Arc;

use mnemo_core::embedding::NoopEmbedding;
use mnemo_core::index::usearch::UsearchIndex;
use mnemo_core::query::MnemoEngine;
use mnemo_core::query::recall::RecallRequest;
use mnemo_core::query::remember::RememberRequest;
use mnemo_core::search::tantivy_index::TantivyFullTextIndex;
use mnemo_core::storage::duckdb::DuckDbStorage;

fn make_engine() -> MnemoEngine {
    let storage = Arc::new(DuckDbStorage::open_in_memory().unwrap());
    let index = Arc::new(UsearchIndex::new(3).unwrap());
    let embedding = Arc::new(NoopEmbedding::new(3));
    MnemoEngine::new(
        storage,
        index,
        embedding,
        "bench-agent".to_string(),
        None,
    )
}

fn remember_throughput(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let engine = make_engine();

    c.bench_function("remember_throughput", |b| {
        b.iter(|| {
            rt.block_on(async {
                let request = RememberRequest {
                    content: "Benchmark memory content for testing throughput".to_string(),
                    agent_id: None,
                    memory_type: None,
                    scope: None,
                    importance: Some(0.5),
                    tags: Some(vec!["bench".to_string()]),
                    metadata: None,
                    source_type: None,
                    source_id: None,
                    org_id: None,
                    thread_id: None,
                    ttl_seconds: None,
                    related_to: None,
                    decay_rate: None,
                    created_by: None,
                };
                engine.remember(request).await.unwrap();
            });
        });
    });
}

fn recall_latency(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let engine = make_engine();

    // Seed with memories
    rt.block_on(async {
        for i in 0..100 {
            let request = RememberRequest {
                content: format!("Memory number {} about various topics for recall testing", i),
                agent_id: None,
                memory_type: None,
                scope: None,
                importance: Some(0.5),
                tags: Some(vec!["recall-bench".to_string()]),
                metadata: None,
                source_type: None,
                source_id: None,
                org_id: None,
                thread_id: None,
                ttl_seconds: None,
                related_to: None,
                decay_rate: None,
                created_by: None,
            };
            engine.remember(request).await.unwrap();
        }
    });

    c.bench_function("recall_latency", |b| {
        b.iter(|| {
            rt.block_on(async {
                let request = RecallRequest {
                    query: "topics for testing".to_string(),
                    agent_id: None,
                    limit: Some(10),
                    memory_type: None,
                    memory_types: None,
                    scope: None,
                    min_importance: None,
                    tags: None,
                    org_id: None,
                    strategy: Some("semantic".to_string()),
                    temporal_range: None,
                    recency_half_life_hours: None,
                    hybrid_weights: None,
                    rrf_k: None,
                    as_of: None,
                };
                engine.recall(request).await.unwrap();
            });
        });
    });
}

fn verify_chain_performance(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let engine = make_engine();

    // Seed with chained memories
    rt.block_on(async {
        for i in 0..100 {
            let request = RememberRequest {
                content: format!("Chained memory {} for verification benchmark", i),
                agent_id: None,
                memory_type: None,
                scope: None,
                importance: Some(0.5),
                tags: None,
                metadata: None,
                source_type: None,
                source_id: None,
                org_id: None,
                thread_id: Some("bench-thread".to_string()),
                ttl_seconds: None,
                related_to: None,
                decay_rate: None,
                created_by: None,
            };
            engine.remember(request).await.unwrap();
        }
    });

    c.bench_function("verify_chain_100", |b| {
        b.iter(|| {
            rt.block_on(async {
                engine
                    .verify_integrity(None, Some("bench-thread"))
                    .await
                    .unwrap();
            });
        });
    });
}

fn hybrid_recall_latency(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let storage = Arc::new(DuckDbStorage::open_in_memory().unwrap());
    let index = Arc::new(UsearchIndex::new(3).unwrap());
    let embedding = Arc::new(NoopEmbedding::new(3));
    let ft = Arc::new(TantivyFullTextIndex::open_in_memory().unwrap());
    let engine = MnemoEngine::new(
        storage,
        index,
        embedding,
        "bench-agent".to_string(),
        None,
    )
    .with_full_text(ft);

    rt.block_on(async {
        for i in 0..500 {
            let request = RememberRequest {
                content: format!(
                    "Hybrid benchmark memory {} about AI agents and vector databases",
                    i
                ),
                agent_id: None,
                memory_type: None,
                scope: None,
                importance: Some(0.5),
                tags: Some(vec!["hybrid-bench".to_string()]),
                metadata: None,
                source_type: None,
                source_id: None,
                org_id: None,
                thread_id: None,
                ttl_seconds: None,
                related_to: None,
                decay_rate: None,
                created_by: None,
            };
            engine.remember(request).await.unwrap();
        }
    });

    c.bench_function("hybrid_recall_latency", |b| {
        b.iter(|| {
            rt.block_on(async {
                let request = RecallRequest {
                    query: "AI agents and vector databases".to_string(),
                    agent_id: None,
                    limit: Some(10),
                    memory_type: None,
                    memory_types: None,
                    scope: None,
                    min_importance: None,
                    tags: None,
                    org_id: None,
                    strategy: Some("hybrid".to_string()),
                    temporal_range: None,
                    recency_half_life_hours: None,
                    hybrid_weights: None,
                    rrf_k: None,
                    as_of: None,
                };
                engine.recall(request).await.unwrap();
            });
        });
    });
}

fn graph_traversal_latency(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let engine = make_engine();

    let mut ids: Vec<uuid::Uuid> = Vec::new();
    rt.block_on(async {
        for i in 0..50 {
            let related = if ids.is_empty() {
                None
            } else {
                Some(vec![ids.last().unwrap().to_string()])
            };
            let request = RememberRequest {
                content: format!("Graph memory {} with relationships for traversal", i),
                agent_id: None,
                memory_type: None,
                scope: None,
                importance: Some(0.5),
                tags: Some(vec!["graph-bench".to_string()]),
                metadata: None,
                source_type: None,
                source_id: None,
                org_id: None,
                thread_id: None,
                ttl_seconds: None,
                related_to: related,
                decay_rate: None,
                created_by: None,
            };
            let resp = engine.remember(request).await.unwrap();
            ids.push(resp.id);
        }
    });

    c.bench_function("graph_traversal_latency", |b| {
        b.iter(|| {
            rt.block_on(async {
                let request = RecallRequest {
                    query: "graph relationships for traversal".to_string(),
                    agent_id: None,
                    limit: Some(10),
                    memory_type: None,
                    memory_types: None,
                    scope: None,
                    min_importance: None,
                    tags: None,
                    org_id: None,
                    strategy: Some("graph".to_string()),
                    temporal_range: None,
                    recency_half_life_hours: None,
                    hybrid_weights: None,
                    rrf_k: None,
                    as_of: None,
                };
                engine.recall(request).await.unwrap();
            });
        });
    });
}

fn checkpoint_restore_latency(c: &mut Criterion) {
    use mnemo_core::query::checkpoint::CheckpointRequest;
    use mnemo_core::query::replay::ReplayRequest;

    let rt = tokio::runtime::Runtime::new().unwrap();
    let engine = make_engine();

    rt.block_on(async {
        for i in 0..20 {
            let request = RememberRequest {
                content: format!("Checkpoint memory {} for restore benchmark", i),
                agent_id: None,
                memory_type: None,
                scope: None,
                importance: Some(0.5),
                tags: None,
                metadata: None,
                source_type: None,
                source_id: None,
                org_id: None,
                thread_id: Some("bench-cp-thread".to_string()),
                ttl_seconds: None,
                related_to: None,
                decay_rate: None,
                created_by: None,
            };
            engine.remember(request).await.unwrap();
        }
        engine
            .checkpoint(CheckpointRequest {
                thread_id: "bench-cp-thread".to_string(),
                agent_id: None,
                branch_name: Some("main".to_string()),
                state_snapshot: serde_json::json!({"bench": true}),
                label: Some("bench".to_string()),
                metadata: None,
            })
            .await
            .unwrap();
    });

    c.bench_function("checkpoint_restore_latency", |b| {
        b.iter(|| {
            rt.block_on(async {
                let request = ReplayRequest {
                    thread_id: "bench-cp-thread".to_string(),
                    agent_id: None,
                    checkpoint_id: None,
                    branch_name: Some("main".to_string()),
                };
                engine.replay(request).await.unwrap();
            });
        });
    });
}

fn concurrent_agents_throughput(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let engine = make_engine();

    c.bench_function("concurrent_agents_throughput", |b| {
        b.iter(|| {
            rt.block_on(async {
                for agent_idx in 0..10 {
                    let agent_id = format!("agent-{agent_idx}");
                    for mem_idx in 0..10 {
                        let request = RememberRequest {
                            content: format!(
                                "Concurrent memory {mem_idx} from agent {agent_idx}"
                            ),
                            agent_id: Some(agent_id.clone()),
                            memory_type: None,
                            scope: None,
                            importance: Some(0.5),
                            tags: None,
                            metadata: None,
                            source_type: None,
                            source_id: None,
                            org_id: None,
                            thread_id: None,
                            ttl_seconds: None,
                            related_to: None,
                            decay_rate: None,
                            created_by: None,
                        };
                        engine.remember(request).await.unwrap();
                    }
                }
            });
        });
    });
}

fn forget_throughput(c: &mut Criterion) {
    use mnemo_core::query::forget::ForgetRequest;

    let rt = tokio::runtime::Runtime::new().unwrap();
    let engine = make_engine();

    // Seed memories and collect IDs
    let ids: Vec<uuid::Uuid> = rt.block_on(async {
        let mut ids = Vec::new();
        for i in 0..100 {
            let request = RememberRequest {
                content: format!("Forget benchmark memory {i}"),
                agent_id: None,
                memory_type: None,
                scope: None,
                importance: Some(0.5),
                tags: None,
                metadata: None,
                source_type: None,
                source_id: None,
                org_id: None,
                thread_id: None,
                ttl_seconds: None,
                related_to: None,
                decay_rate: None,
                created_by: None,
            };
            let resp = engine.remember(request).await.unwrap();
            ids.push(resp.id);
        }
        ids
    });

    let mut batch_start = 0usize;
    c.bench_function("forget_throughput", |b| {
        b.iter(|| {
            rt.block_on(async {
                if batch_start + 10 <= ids.len() {
                    let batch: Vec<uuid::Uuid> = ids[batch_start..batch_start + 10].to_vec();
                    let request = ForgetRequest {
                        memory_ids: batch,
                        agent_id: None,
                        strategy: None,
                        criteria: None,
                    };
                    let _ = engine.forget(request).await;
                    batch_start += 10;
                }
            });
        });
    });
}

criterion_group!(
    benches,
    remember_throughput,
    recall_latency,
    verify_chain_performance,
    hybrid_recall_latency,
    graph_traversal_latency,
    checkpoint_restore_latency,
    concurrent_agents_throughput,
    forget_throughput
);
criterion_main!(benches);
