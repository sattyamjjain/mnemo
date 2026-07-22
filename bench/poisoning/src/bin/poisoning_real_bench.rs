//! `poisoning_real_bench` — memory-poisoning **defense** benchmark on a **real
//! semantic embedder** (never `NoopEmbedding`).
//!
//! Exercises mnemo's shipped poisoning detector (`check_for_anomaly` →
//! `quarantine_memory` on the `remember` write path + `recall`'s quarantined
//! skip, incl. the embedding-space z-score lane) through a real embedder, and
//! reports per-attack **ASR + Wilson 95%** and the **benign false-positive
//! rate** over `--repeats` seeds. Refuses to score under a no-op embedder.
//!
//! # Reproduce (ONNX default, no credentials)
//!
//! ```text
//! MNEMO_ONNX_MODEL_PATH=/path/to/all-MiniLM-L6-v2/model.onnx \
//!   cargo run --release --features onnx -p mnemo-poisoning-bench --bin poisoning_real_bench
//! ```
//!
//! Alternatives: `--embedder openai` (`OPENAI_API_KEY`) or `--embedder ollama`
//! (local Ollama, `ollama pull nomic-embed-text`).

use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;

use mnemo_core::embedding::EmbeddingProvider;
use mnemo_core::error::{Error, Result as MnemoResult};
use mnemo_locomo_bench::real_embedder::guard_real_embedder;
use mnemo_poisoning_bench::real_embedder_bench::{
    RealBenchConfig, render_console, render_json, run_real_bench,
};

type BoxErr = Box<dyn std::error::Error + Send + Sync>;

// ---------------------------------------------------------------------------
// Ollama HTTP embedder (local, no key)
// ---------------------------------------------------------------------------
struct OllamaEmbedding {
    client: reqwest::Client,
    url: String,
    model: String,
    dimensions: usize,
}

impl OllamaEmbedding {
    async fn connect(url: String, model: String) -> MnemoResult<Self> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| Error::Embedding(format!("http client: {e}")))?;
        let probe = Self {
            client,
            url,
            model,
            dimensions: 0,
        };
        let v = probe.embed_raw("dimensionality probe").await.map_err(|e| {
            Error::Embedding(format!(
                "{e} — is Ollama running and the model pulled? Try: `ollama pull {}`",
                probe.model
            ))
        })?;
        let dimensions = v.len();
        if dimensions == 0 {
            return Err(Error::Embedding(
                "embedder returned a 0-length vector".into(),
            ));
        }
        Ok(Self {
            dimensions,
            ..probe
        })
    }

    async fn embed_raw(&self, text: &str) -> MnemoResult<Vec<f32>> {
        let resp = self
            .client
            .post(&self.url)
            .json(&serde_json::json!({ "model": self.model, "prompt": text }))
            .send()
            .await
            .map_err(|e| Error::Embedding(format!("ollama request failed: {e}")))?;
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
        let arr = body
            .get("embedding")
            .and_then(|e| e.as_array())
            .ok_or_else(|| Error::Embedding("response missing 'embedding' array".into()))?;
        Ok(arr
            .iter()
            .map(|x| x.as_f64().unwrap_or(0.0) as f32)
            .collect())
    }
}

#[async_trait::async_trait]
impl EmbeddingProvider for OllamaEmbedding {
    async fn embed(&self, text: &str) -> MnemoResult<Vec<f32>> {
        self.embed_raw(text).await
    }
    async fn embed_batch(&self, texts: &[&str]) -> MnemoResult<Vec<Vec<f32>>> {
        let mut out = Vec::with_capacity(texts.len());
        for t in texts {
            out.push(self.embed_raw(t).await?);
        }
        Ok(out)
    }
    fn dimensions(&self) -> usize {
        self.dimensions
    }
}

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------
#[derive(Parser, Debug)]
#[command(
    name = "poisoning_real_bench",
    about = "memory-poisoning defense benchmark on a real embedder (ASR + 95% CI + benign-FPR)"
)]
struct Cli {
    /// Embedder backend: `onnx` (default, no key), `openai`, or `ollama`.
    #[arg(long, default_value = "onnx")]
    embedder: String,
    #[arg(long, env = "MNEMO_ONNX_MODEL_PATH")]
    onnx_model: Option<PathBuf>,
    #[arg(long, default_value_t = 384)]
    onnx_dim: usize,
    #[arg(long, default_value = "text-embedding-3-small")]
    openai_model: String,
    #[arg(long, default_value_t = 1536)]
    openai_dim: usize,
    #[arg(long, default_value = "http://localhost:11434/api/embeddings")]
    ollama_url: String,
    #[arg(long, default_value = "nomic-embed-text")]
    ollama_model: String,
    #[arg(long, default_value_t = 30)]
    trials: usize,
    #[arg(long, default_value_t = 5)]
    k: usize,
    #[arg(long, default_value_t = 3)]
    repeats: usize,
    #[arg(long, default_value_t = 200)]
    benign: usize,
    #[arg(long, default_value_t = 100)]
    benign_control: usize,
    #[arg(long, default_value_t = 3.0)]
    z_threshold: f32,
    #[arg(long, default_value = "bench/results/poisoning_real.json")]
    out: PathBuf,
}

struct EmbedderMeta {
    backend: String,
    model: String,
    dim: usize,
}

async fn resolve_embedder(cli: &Cli) -> Result<(Arc<dyn EmbeddingProvider>, EmbedderMeta), BoxErr> {
    match cli.embedder.as_str() {
        "onnx" => {
            #[cfg(feature = "onnx")]
            {
                let path = cli.onnx_model.clone().ok_or_else(|| {
                    "onnx embedder needs --onnx-model (or MNEMO_ONNX_MODEL_PATH)".to_string()
                })?;
                let model_str = path.to_string_lossy().to_string();
                let e = mnemo_core::embedding::onnx::OnnxEmbedding::new(&model_str, cli.onnx_dim)?;
                let model_name = path
                    .parent()
                    .and_then(|p| p.file_name())
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "onnx".to_string());
                Ok((
                    Arc::new(e),
                    EmbedderMeta {
                        backend: "onnx".into(),
                        model: model_name,
                        dim: cli.onnx_dim,
                    },
                ))
            }
            #[cfg(not(feature = "onnx"))]
            {
                Err(
                    "this binary was built WITHOUT the `onnx` feature; rebuild with \
                     `--features onnx` (or use `--embedder ollama`)"
                        .into(),
                )
            }
        }
        "openai" => {
            let key = std::env::var("OPENAI_API_KEY")
                .map_err(|_| "openai embedder needs OPENAI_API_KEY".to_string())?;
            let e = mnemo_core::embedding::openai::OpenAiEmbedding::new(
                key,
                cli.openai_model.clone(),
                cli.openai_dim,
            );
            Ok((
                Arc::new(e),
                EmbedderMeta {
                    backend: "openai".into(),
                    model: cli.openai_model.clone(),
                    dim: cli.openai_dim,
                },
            ))
        }
        "ollama" => {
            let e =
                OllamaEmbedding::connect(cli.ollama_url.clone(), cli.ollama_model.clone()).await?;
            let dim = e.dimensions();
            Ok((
                Arc::new(e),
                EmbedderMeta {
                    backend: "ollama".into(),
                    model: cli.ollama_model.clone(),
                    dim,
                },
            ))
        }
        other => Err(format!("unknown --embedder '{other}' (expected onnx|openai|ollama)").into()),
    }
}

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() -> Result<(), BoxErr> {
    let cli = Cli::parse();
    assert!(cli.repeats >= 1, "--repeats must be >= 1");
    assert!(
        cli.benign >= 30,
        "--benign must be >= 30 (MIN_BASELINE_SAMPLES)"
    );

    let (embedding, meta) = resolve_embedder(&cli).await?;
    // Fail loud on a no-op embedder BEFORE seeding anything.
    guard_real_embedder(&*embedding, &meta.backend)?;
    eprintln!(
        "embedder OK: backend={} model={} dim={} (semantic-capable)",
        meta.backend, meta.model, meta.dim
    );

    let cfg = RealBenchConfig {
        trials: cli.trials,
        k: cli.k,
        repeats: cli.repeats,
        benign: cli.benign,
        benign_control_n: cli.benign_control,
        z_threshold: cli.z_threshold,
    };

    let outcome = run_real_bench(embedding, meta.dim, &meta.backend, &meta.model, &cfg).await?;

    if let Some(parent) = cli.out.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = render_json(&outcome);
    std::fs::write(&cli.out, serde_json::to_string_pretty(&json)? + "\n")?;

    print!("{}", render_console(&outcome));
    println!("wrote {}", cli.out.display());
    Ok(())
}
