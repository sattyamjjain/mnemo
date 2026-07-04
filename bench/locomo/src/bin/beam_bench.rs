//! BEAM-style multi-hop + open-domain retrieval bench over mnemo's hybrid recall.
//!
//! # What this is
//!
//! A **reproducible, deterministic** micro-benchmark that exercises mnemo's
//! default hybrid retrieval (semantic vector + BM25 + graph-expansion + recency,
//! fused with RRF — `strategy = "auto"`) on two BEAM-style subtasks:
//!
//! * **multi-hop** — the gold answer lives in a memory that is *graph-linked*
//!   (`related_to`) to a memory the query matches, but shares **no** entity
//!   token with the query. Answering requires the hop
//!   `query → head → linked detail`, which only the graph-expansion lane of
//!   `auto` recall can bridge.
//! * **open-domain** — the gold memory sits among many same-schema distractors;
//!   the query must surface it from the whole corpus.
//!
//! Accuracy per subtask = fraction of queries whose gold memory is in the
//! top-`k`, reported with a **Wilson 95%** interval
//! ([`mnemo_locomo_bench::stats::wilson_95`], shared with the other benches).
//!
//! # This is NOT the BEAM dataset
//!
//! BEAM (Hindsight's [10M-token memory benchmark](https://hindsight.vectorize.io/blog/2026/04/02/beam-sota),
//! self-reported SOTA **64.1%**) runs a huge external corpus scored by an LLM
//! judge. This bin runs a **small synthetic fixture** with a deterministic
//! offline embedder and **no LLM**. The number here measures whether mnemo's
//! retrieval *mechanism* recovers multi-hop / open-domain gold on a controlled
//! fixture — it is **not comparable** to the upstream self-reported score and
//! must never be read as a ranking against it (see `bench/RESULTS.md`).
//!
//! # Embedder
//!
//! Default: a **deterministic offline** hashed-bag-of-tokens embedder — no
//! network, identical output every run, CI-safe. Pass `--ollama-model <name>`
//! for a real semantic embedder (higher fidelity, **not** deterministic); like
//! the LongMemEval bench it fails loud if Ollama is unreachable rather than
//! emitting a silent number.
//!
//! Reproduce: `cargo run --release -p mnemo-locomo-bench --bin beam_bench`

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use clap::Parser;

use mnemo_core::embedding::EmbeddingProvider;
use mnemo_core::error::{Error, Result as MnResult};
use mnemo_core::index::usearch::UsearchIndex;
use mnemo_core::query::MnemoEngine;
use mnemo_core::query::recall::RecallRequest;
use mnemo_core::query::remember::RememberRequest;
use mnemo_core::search::tantivy_index::TantivyFullTextIndex;
use mnemo_core::storage::duckdb::DuckDbStorage;
use mnemo_locomo_bench::stats::wilson_95;

const AGENT: &str = "beam-bench-agent";
/// Fixed default seed → fully reproducible run.
const DEFAULT_SEED: u64 = 0xBEA3_2026_2026_u64;
/// Deterministic offline embedder width.
const EMBED_DIM: usize = 128;

#[derive(Parser, Debug)]
#[command(name = "beam_bench")]
struct Cli {
    /// Queries (and gold memories) per subtask, per repeat.
    #[arg(long, default_value_t = 100)]
    queries: usize,
    /// In-process repeats (fresh fixture each) pooled into one number. The
    /// vector lane is an approximate-NN index (USearch HNSW) whose ranking has
    /// a small run-to-run noise floor; pooling repeats stabilises the reported
    /// rate the way `semantic_recall_bench` does. `queries * repeats` = n.
    #[arg(long, default_value_t = 5)]
    repeats: usize,
    /// Distractor memories per gold (same-schema noise the gold competes with).
    #[arg(long, default_value_t = 4)]
    distractors: usize,
    /// Top-k cutoff for "was the gold recalled".
    #[arg(long, default_value_t = 5)]
    k: usize,
    /// Deterministic seed.
    #[arg(long, default_value_t = DEFAULT_SEED)]
    seed: u64,
    /// Output directory for the Markdown + JSON report.
    #[arg(long, default_value = "bench/locomo/results")]
    out_dir: PathBuf,
    /// Use a real Ollama embedder (e.g. `nomic-embed-text`) instead of the
    /// deterministic offline one. Higher fidelity, but NOT deterministic and
    /// NOT CI-safe; fails loud if Ollama is unreachable.
    #[arg(long)]
    ollama_model: Option<String>,
    /// Ollama base URL (used only with `--ollama-model`).
    #[arg(long, default_value = "http://localhost:11434")]
    ollama_url: String,
}

/// splitmix64 — tiny deterministic PRNG (no `rand` dep, stable across platforms
/// so the published number is reproducible).
struct SplitMix64(u64);
impl SplitMix64 {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    /// A short lowercase token, deterministic in the draw.
    fn token(&mut self) -> String {
        format!("{:08x}", self.next_u64() & 0xFFFF_FFFF)
    }
}

// ---------------------------------------------------------------------------
// Embedders
// ---------------------------------------------------------------------------

fn fnv1a(t: &str) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325_u64;
    for b in t.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Deterministic, offline, network-free embedder: L2-normalised hashed
/// bag-of-tokens. Overlapping vocabulary → higher cosine, identical every run.
/// It is lexical (no synonymy) — that is the point: reproducible, not clever.
struct HashEmbedding {
    dim: usize,
}

impl HashEmbedding {
    fn embed_sync(&self, text: &str) -> Vec<f32> {
        let mut v = vec![0.0f32; self.dim];
        for tok in text
            .split(|c: char| !c.is_ascii_alphanumeric())
            .filter(|s| !s.is_empty())
        {
            let idx = (fnv1a(&tok.to_ascii_lowercase()) as usize) % self.dim;
            v[idx] += 1.0;
        }
        let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for x in &mut v {
                *x /= norm;
            }
        }
        v
    }
}

#[async_trait]
impl EmbeddingProvider for HashEmbedding {
    async fn embed(&self, text: &str) -> MnResult<Vec<f32>> {
        Ok(self.embed_sync(text))
    }
    async fn embed_batch(&self, texts: &[&str]) -> MnResult<Vec<Vec<f32>>> {
        Ok(texts.iter().map(|t| self.embed_sync(t)).collect())
    }
    fn dimensions(&self) -> usize {
        self.dim
    }
}

/// Real semantic embedder via a local Ollama server (gated behind
/// `--ollama-model`). Fails loud, never silent — matching the LongMemEval bench.
struct OllamaEmbedding {
    client: reqwest::Client,
    url: String,
    model: String,
    dim: usize,
}

impl OllamaEmbedding {
    async fn connect(url: String, model: String) -> MnResult<Self> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .map_err(|e| Error::Embedding(e.to_string()))?;
        let mut me = Self {
            client,
            url,
            model,
            dim: 0,
        };
        // Probe dimensionality once so the index is sized correctly.
        let probe = me.embed("dimension probe").await?;
        me.dim = probe.len();
        Ok(me)
    }
    async fn embed_one(&self, text: &str) -> MnResult<Vec<f32>> {
        let resp = self
            .client
            .post(format!("{}/api/embeddings", self.url))
            .json(&serde_json::json!({ "model": self.model, "prompt": text }))
            .send()
            .await
            .map_err(|e| {
                Error::Embedding(format!(
                    "{e} — is Ollama running and the model pulled? Try: `ollama pull {}`",
                    self.model
                ))
            })?;
        if !resp.status().is_success() {
            return Err(Error::Embedding(format!(
                "ollama returned HTTP {}",
                resp.status()
            )));
        }
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| Error::Embedding(format!("ollama response decode: {e}")))?;
        let embedding = body
            .get("embedding")
            .and_then(|e| e.as_array())
            .ok_or_else(|| Error::Embedding("ollama response missing `embedding`".to_string()))?
            .iter()
            .map(|v| v.as_f64().unwrap_or(0.0) as f32)
            .collect();
        Ok(embedding)
    }
}

#[async_trait]
impl EmbeddingProvider for OllamaEmbedding {
    async fn embed(&self, text: &str) -> MnResult<Vec<f32>> {
        self.embed_one(text).await
    }
    async fn embed_batch(&self, texts: &[&str]) -> MnResult<Vec<Vec<f32>>> {
        let mut out = Vec::with_capacity(texts.len());
        for t in texts {
            out.push(self.embed_one(t).await?);
        }
        Ok(out)
    }
    fn dimensions(&self) -> usize {
        self.dim
    }
}

// ---------------------------------------------------------------------------
// Engine + subtasks
// ---------------------------------------------------------------------------

fn build_engine(embedding: Arc<dyn EmbeddingProvider>, dim: usize) -> MnemoEngine {
    let storage = Arc::new(DuckDbStorage::open_in_memory().unwrap());
    let index = Arc::new(UsearchIndex::new(dim).unwrap());
    let ft = Arc::new(TantivyFullTextIndex::open_in_memory().unwrap());
    MnemoEngine::new(storage, index, embedding, AGENT.to_string(), None).with_full_text(ft)
}

async fn remember(engine: &MnemoEngine, content: String) -> uuid::Uuid {
    engine
        .remember(RememberRequest::new(content))
        .await
        .unwrap()
        .id
}

async fn remember_linked(engine: &MnemoEngine, content: String, related: uuid::Uuid) -> uuid::Uuid {
    let mut req = RememberRequest::new(content);
    req.related_to = Some(vec![related.to_string()]);
    engine.remember(req).await.unwrap().id
}

/// Does `auto` (RRF hybrid) recall surface `gold_id` in the top-k for `query`?
async fn auto_recall_hits(
    engine: &MnemoEngine,
    query: &str,
    gold_id: uuid::Uuid,
    k: usize,
) -> bool {
    let mut req = RecallRequest::new(query.to_string());
    req.strategy = Some("auto".to_string());
    req.limit = Some(k);
    let resp = engine.recall(req).await.unwrap();
    resp.memories.iter().any(|m| m.id == gold_id)
}

struct SubtaskResult {
    label: &'static str,
    n: usize,
    hits: usize,
}

impl SubtaskResult {
    fn accuracy(&self) -> f64 {
        self.hits as f64 / self.n.max(1) as f64
    }
    /// (point, ci_low, ci_high)
    fn accuracy_ci(&self) -> (f64, f64, f64) {
        let (lo, hi) = wilson_95(self.hits, self.n);
        (self.accuracy(), lo, hi)
    }
}

/// Multi-hop: the gold detail is graph-linked to a head the query matches, but
/// shares no entity token with the query — only graph expansion can bridge it.
/// Returns `(hits, queries)` for one fixture drawn from `seed`.
async fn run_multi_hop(
    make_embedding: &dyn Fn() -> Arc<dyn EmbeddingProvider>,
    dim: usize,
    cli: &Cli,
    seed: u64,
) -> (usize, usize) {
    let engine = build_engine(make_embedding(), dim);
    let mut rng = SplitMix64(seed ^ 0x11_11_11);

    // Seed all clusters first so the corpus (and its distractors) is fully
    // present before any query runs.
    let mut golds: Vec<(String, uuid::Uuid)> = Vec::with_capacity(cli.queries);
    for _ in 0..cli.queries {
        let person = rng.token();
        let team = rng.token();
        let service = rng.token();
        // Head: matches the query on `person`.
        let head = remember(
            &engine,
            format!("Person {person} leads the {team} engineering team."),
        )
        .await;
        // Gold detail: holds the answer `service`, linked to the head, and
        // shares NO token with the query (the query never mentions `team`).
        let gold = remember_linked(
            &engine,
            format!("The {team} team owns and operates the {service} service."),
            head,
        )
        .await;
        // Same-schema distractors (unlinked, unrelated people/teams/services).
        for _ in 0..cli.distractors {
            let content = format!(
                "The {} team owns and operates the {} service.",
                rng.token(),
                rng.token()
            );
            remember(&engine, content).await;
        }
        let query = format!("Which service does the team led by {person} operate?");
        golds.push((query, gold));
    }

    let mut hits = 0usize;
    for (query, gold) in &golds {
        if auto_recall_hits(&engine, query, *gold, cli.k).await {
            hits += 1;
        }
    }
    (hits, golds.len())
}

/// Open-domain: the gold sits among same-schema distractors across the whole
/// corpus; the query shares the gold's subject entity plus schema words.
/// Returns `(hits, queries)` for one fixture drawn from `seed`.
async fn run_open_domain(
    make_embedding: &dyn Fn() -> Arc<dyn EmbeddingProvider>,
    dim: usize,
    cli: &Cli,
    seed: u64,
) -> (usize, usize) {
    let engine = build_engine(make_embedding(), dim);
    let mut rng = SplitMix64(seed ^ 0x22_22_22);

    let mut golds: Vec<(String, uuid::Uuid)> = Vec::with_capacity(cli.queries);
    for _ in 0..cli.queries {
        let subject = rng.token();
        let answer = rng.token();
        let gold = remember(
            &engine,
            format!("Support ticket about {subject}: the incident was resolved by {answer}."),
        )
        .await;
        for _ in 0..cli.distractors {
            let content = format!(
                "Support ticket about {}: the incident was resolved by {}.",
                rng.token(),
                rng.token()
            );
            remember(&engine, content).await;
        }
        let query = format!("Who resolved the support ticket about {subject}?");
        golds.push((query, gold));
    }

    let mut hits = 0usize;
    for (query, gold) in &golds {
        if auto_recall_hits(&engine, query, *gold, cli.k).await {
            hits += 1;
        }
    }
    (hits, golds.len())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    // Resolve the embedder. Default = deterministic offline; `--ollama-model`
    // opts into a real embedder (probed once up front so it fails loud here,
    // not mid-run).
    let (embed_backend, dim): (String, usize) = if let Some(ref model) = cli.ollama_model {
        let probe = OllamaEmbedding::connect(cli.ollama_url.clone(), model.clone()).await?;
        (format!("ollama:{model}"), probe.dim)
    } else {
        (
            "hash-bag-of-tokens (deterministic, offline)".to_string(),
            EMBED_DIM,
        )
    };

    // A factory so each subtask gets a fresh embedder instance.
    let url = cli.ollama_url.clone();
    let model = cli.ollama_model.clone();
    let make_embedding = move || -> Arc<dyn EmbeddingProvider> {
        if let Some(ref m) = model {
            // block_on a fresh connect — the multi-thread runtime allows it;
            // this path is opt-in and non-deterministic by design.
            let url = url.clone();
            let m = m.clone();
            let handle = tokio::runtime::Handle::current();
            Arc::new(
                tokio::task::block_in_place(|| handle.block_on(OllamaEmbedding::connect(url, m)))
                    .expect("ollama connect"),
            )
        } else {
            Arc::new(HashEmbedding { dim: EMBED_DIM })
        }
    };

    // Pool `repeats` fresh fixtures per subtask into one number (n = queries *
    // repeats), so the HNSW ranking noise floor averages out into the CI.
    let mut multi_hop = SubtaskResult {
        label: "multi_hop",
        n: 0,
        hits: 0,
    };
    let mut open_domain = SubtaskResult {
        label: "open_domain",
        n: 0,
        hits: 0,
    };
    for rep in 0..cli.repeats.max(1) {
        let rep_seed = cli.seed ^ (rep as u64).wrapping_mul(0x9E37_79B1);
        let (mh, mn) = run_multi_hop(&make_embedding, dim, &cli, rep_seed).await;
        multi_hop.hits += mh;
        multi_hop.n += mn;
        let (oh, on) = run_open_domain(&make_embedding, dim, &cli, rep_seed).await;
        open_domain.hits += oh;
        open_domain.n += on;
    }

    let date = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let (mh_p, mh_lo, mh_hi) = multi_hop.accuracy_ci();
    let (od_p, od_lo, od_hi) = open_domain.accuracy_ci();

    // ---- stdout ----
    println!(
        "\n=== BEAM-style retrieval bench (hybrid auto/RRF) — {} queries × {} repeats/subtask, \
         top-{}, seed {:#x}, embedder={} ===",
        cli.queries, cli.repeats, cli.k, cli.seed, embed_backend
    );
    println!(
        "{:<14} {:>8} {:>8} {:>28}",
        "subtask", "queries", "hits", "accuracy (95% Wilson)"
    );
    for r in [&multi_hop, &open_domain] {
        let (p, lo, hi) = r.accuracy_ci();
        println!(
            "{:<14} {:>8} {:>8} {:>14.1}%  [{:.1}%, {:.1}%]",
            r.label,
            r.n,
            r.hits,
            p * 100.0,
            lo * 100.0,
            hi * 100.0
        );
    }
    println!(
        "\nReproduced on a synthetic BEAM-STYLE fixture (deterministic, no LLM). NOT the \
         upstream BEAM dataset; self-reported upstream (Hindsight, 64.1% @ 10M tokens) is an \
         upper bound and is not directly comparable — see bench/RESULTS.md."
    );

    // ---- JSON ----
    let result_json: Vec<serde_json::Value> = [&multi_hop, &open_domain]
        .iter()
        .map(|r| {
            let (p, lo, hi) = r.accuracy_ci();
            serde_json::json!({
                "subtask": r.label,
                "queries": r.n,
                "hits": r.hits,
                "accuracy": p,
                "accuracy_ci95_low": lo,
                "accuracy_ci95_high": hi,
            })
        })
        .collect();
    let json = serde_json::json!({
        "bench": "beam_bench",
        "style": "BEAM-style multi-hop + open-domain retrieval over hybrid auto/RRF recall",
        "note": "synthetic deterministic fixture; NOT the upstream 10M-token BEAM dataset; no LLM judge",
        "upstream_self_reported": { "system": "Hindsight", "beam_accuracy": 0.641, "scale": "10M tokens", "source": "https://hindsight.vectorize.io/blog/2026/04/02/beam-sota" },
        "date": date,
        "queries_per_repeat": cli.queries,
        "repeats": cli.repeats,
        "distractors_per_gold": cli.distractors,
        "top_k": cli.k,
        "seed": cli.seed,
        "embedder": embed_backend,
        "hnsw_noise_note": "vector lane is USearch HNSW (approximate NN); repeats pooled to average the run-to-run ranking noise floor into the Wilson CI",
        "results": result_json,
    });

    // ---- Markdown ----
    let md = format!(
        "# BEAM-style retrieval bench — multi-hop + open-domain (hybrid auto/RRF)\n\n\
         > {date} — reproducible retrieval bench over mnemo's default hybrid recall \
         (`strategy=\"auto\"`: semantic + BM25 + graph-expansion + recency, RRF-fused). \
         **Synthetic deterministic fixture, no LLM** — this is NOT the upstream BEAM \
         dataset. See the honesty note in [`bench/RESULTS.md`](../../RESULTS.md).\n\n\
         - **{q} queries × {reps} repeats/subtask** (n={mn}), {d} distractors/gold, top-{k}, \
         seed `{seed:#x}`, embedder `{eb}`.\n\
         - *Accuracy* = fraction of queries whose gold memory is in the top-{k} (Wilson 95%). \
         Repeats are pooled to average the USearch HNSW approximate-NN ranking noise floor into \
         the CI (same treatment `semantic_recall_bench` documents).\n\n\
         | subtask | queries | hits | **accuracy** (95% Wilson) |\n\
         |---|---:|---:|---:|\n\
         | `multi_hop` (graph-linked answer, no shared query token) | {mn} | {mh} | **{mp:.1}%** [{mlo:.1}%, {mhi:.1}%] |\n\
         | `open_domain` (gold among same-schema distractors) | {on} | {oh} | **{op:.1}%** [{olo:.1}%, {ohi:.1}%] |\n\n\
         **Reproduced vs self-reported.** These numbers are reproduced on *this* synthetic \
         fixture. Upstream **BEAM** (Hindsight, self-reported **64.1%** at 10M tokens, \
         [source](https://hindsight.vectorize.io/blog/2026/04/02/beam-sota)) runs a vastly \
         larger real corpus scored by an LLM judge. Self-reported memory scores are an \
         **upper bound** (vendor-run, not independently reproduced across labs), and our \
         fixture number is **not comparable** to it — do not read the two as a ranking. \
         Reproduce: `cargo run --release -p mnemo-locomo-bench --bin beam_bench`.\n",
        q = cli.queries,
        reps = cli.repeats,
        d = cli.distractors,
        k = cli.k,
        seed = cli.seed,
        eb = embed_backend,
        mn = multi_hop.n,
        mh = multi_hop.hits,
        mp = mh_p * 100.0,
        mlo = mh_lo * 100.0,
        mhi = mh_hi * 100.0,
        on = open_domain.n,
        oh = open_domain.hits,
        op = od_p * 100.0,
        olo = od_lo * 100.0,
        ohi = od_hi * 100.0,
        date = date,
    );

    std::fs::create_dir_all(&cli.out_dir).ok();
    let md_path = cli.out_dir.join(format!("beam_{date}.md"));
    let json_path = cli.out_dir.join(format!("beam_{date}.json"));
    std::fs::write(&md_path, md)?;
    std::fs::write(&json_path, serde_json::to_string_pretty(&json)?)?;
    println!("\nwrote {}", md_path.display());
    println!("wrote {}", json_path.display());
    Ok(())
}
