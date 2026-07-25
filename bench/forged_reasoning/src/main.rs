//! `forged_reasoning` — forged-reasoning memory-injection resistance benchmark
//! on a real embedder (ASR OFF vs ON + Wilson-95 + benign false-quarantine).
//!
//! Reproduce (ONNX default, no credentials):
//!
//! ```text
//! MNEMO_ONNX_MODEL_PATH=/path/to/all-MiniLM-L6-v2/model.onnx \
//!   cargo run --release --features onnx -p mnemo-forged-reasoning-bench --bin forged_reasoning
//! ```
//!
//! Alternatives: `--embedder openai` (`OPENAI_API_KEY`) or `--embedder ollama`.

use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;

use mnemo_core::embedding::EmbeddingProvider;
use mnemo_core::error::{Error, Result as MnemoResult};
use mnemo_forged_reasoning_bench::{BenchConfig, render_console, render_json, run_bench};
use mnemo_locomo_bench::real_embedder::guard_real_embedder;

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
    name = "forged_reasoning",
    about = "forged-reasoning memory-injection resistance on a real embedder (ASR OFF/ON + 95% CI + benign FQR)"
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
    #[arg(long, default_value_t = 40)]
    trials: usize,
    #[arg(long, default_value_t = 120)]
    clean: usize,
    #[arg(long, default_value_t = 3)]
    repeats: usize,
    #[arg(long, default_value_t = 5)]
    k: usize,
    #[arg(long, default_value_t = 60)]
    benign_control: usize,
    #[arg(long, default_value = "bench/results/forged_reasoning.json")]
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
    let (embedding, meta) = resolve_embedder(&cli).await?;
    guard_real_embedder(&*embedding, &meta.backend)?;
    eprintln!(
        "embedder OK: backend={} model={} dim={} (semantic-capable)",
        meta.backend, meta.model, meta.dim
    );

    let cfg = BenchConfig {
        trials: cli.trials,
        clean: cli.clean,
        repeats: cli.repeats,
        k: cli.k,
        benign_control: cli.benign_control,
    };

    let outcome = run_bench(embedding, meta.dim, &meta.backend, &meta.model, &cfg).await?;

    if let Some(parent) = cli.out.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(
        &cli.out,
        serde_json::to_string_pretty(&render_json(&outcome))? + "\n",
    )?;
    print!("{}", render_console(&outcome));
    println!("wrote {}", cli.out.display());
    Ok(())
}
