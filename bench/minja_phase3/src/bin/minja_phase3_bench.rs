//! Non-adaptive Phase-3 exploitation runner.
//!
//! ```text
//! curl -sSL --fail -o model.onnx \
//!   https://huggingface.co/Xenova/all-MiniLM-L6-v2/resolve/main/onnx/model.onnx
//! curl -sSL --fail -o tokenizer.json \
//!   https://huggingface.co/Xenova/all-MiniLM-L6-v2/resolve/main/tokenizer.json
//! MNEMO_ONNX_MODEL_PATH=./model.onnx \
//!   cargo run --release --features onnx \
//!     -p mnemo-minja-phase3-bench --bin minja_phase3_bench
//! ```
//!
//! The weights are pinned by digest and the run **refuses to start** on a
//! mismatch, so the command above reproduces the published figure or fails
//! loudly — it cannot quietly produce a different one.

use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use mnemo_core::embedding::EmbeddingProvider;
use mnemo_locomo_bench::pinned_model::verify_pinned;
use mnemo_locomo_bench::real_embedder::guard_real_embedder;
use mnemo_minja_phase3_bench::fixture::{BRIDGING_MARKERS, CorpusRow, Phase3Fixture};
use mnemo_minja_phase3_bench::{Arm, Config, Rate, clustering, run_all, summarise};
use sha2::{Digest, Sha256};

type BoxErr = Box<dyn std::error::Error + Send + Sync>;

/// The pinned checkpoint. Same weights `locomo_v1_bench` measures retrieval
/// quality on, so the two numbers describe the same embedder rather than two
/// unrelated ones.
const PINNED_MODEL_SHA256: &str =
    "759c3cd2b7fe7e93933ad23c4c9181b7396442a2ed746ec7c1d46192c469c46e";
const PINNED_MODEL_ID: &str = "Xenova/all-MiniLM-L6-v2 (onnx/model.onnx)";
const PINNED_MODEL_SOURCE: &str =
    "https://huggingface.co/Xenova/all-MiniLM-L6-v2/resolve/main/onnx/model.onnx";

#[derive(Parser, Debug)]
#[command(
    name = "minja_phase3_bench",
    about = "Non-adaptive Phase-3 exploitation ASR with a matched benign floor (issue #37). NOT a MINJA number."
)]
struct Cli {
    /// ONNX model path. Must hash to the pinned digest.
    #[arg(long, env = "MNEMO_ONNX_MODEL_PATH")]
    onnx_model: Option<PathBuf>,
    #[arg(long, default_value_t = 384)]
    onnx_dim: usize,
    /// Top-k the victim query retrieves (headline).
    #[arg(long, default_value_t = 10)]
    k: usize,
    /// Additional k values measured alongside the headline.
    ///
    /// At k=10 the top-k oracle is at ceiling for BOTH arms (they miss on the
    /// same two queries), which leaves it almost no power to separate them. A
    /// null from a saturated oracle is not worth much, so the same measurement
    /// is taken at smaller k and published with it.
    #[arg(long, value_delimiter = ',', default_values_t = [1usize, 3, 10])]
    k_sweep: Vec<usize>,
    /// Seeds; each permutes insertion order.
    #[arg(long, value_delimiter = ',', default_values_t = [1u64, 2, 3])]
    seeds: Vec<u64>,
    /// z-threshold handed to the shipped PoisoningPolicy.
    #[arg(long, default_value_t = 3.0)]
    z_threshold: f32,
    #[arg(long, default_value = "bench/results/minja_phase3.json")]
    out: PathBuf,
    /// Escape hatch for the offline self-check ONLY. Emits no result file.
    #[arg(long, hide = true)]
    deterministic_smoke: bool,
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("repo root")
        .to_path_buf()
}

#[tokio::main]
async fn main() -> Result<(), BoxErr> {
    let cli = Cli::parse();
    let repo = repo_root();

    // ---- corpus + committed fixture ------------------------------------
    let corpus_path = repo.join("crates/mnemo-core/benches/data/longmemeval_m.jsonl");
    let corpus_raw = std::fs::read(&corpus_path)?;
    let corpus_sha256 = hex::encode(Sha256::digest(&corpus_raw));
    let corpus: Vec<CorpusRow> = String::from_utf8(corpus_raw)?
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()?;

    let fixture_path = repo.join("bench/minja_phase3/fixtures/phase3_records.json");
    let fixture_raw = std::fs::read(&fixture_path)?;
    let fixture_sha256 = hex::encode(Sha256::digest(&fixture_raw));
    let fixture: Phase3Fixture = serde_json::from_slice(&fixture_raw)?;

    // The fixture is pre-registered AGAINST A CORPUS. If the corpus moved, the
    // pairs no longer target the records they were derived from and every rate
    // below would be measuring a mismatch. Refuse rather than report.
    if fixture.corpus_sha256 != corpus_sha256 {
        return Err(format!(
            "fixture was derived from corpus {} but the corpus on disk is {}. \
             Re-derive deliberately: cargo run -p mnemo-minja-phase3-bench --bin derive_fixture",
            fixture.corpus_sha256, corpus_sha256
        )
        .into());
    }

    // A poisoned record carrying bridging markers is the CANONICAL variant that
    // mnemo's lexical lane already quarantines 100% (bench/poisoning). If one
    // ever appeared here the headline would silently become a much easier
    // number wearing this bench's name.
    for p in &fixture.pairs {
        let lc = p.poison_content.to_lowercase();
        if let Some(m) = BRIDGING_MARKERS.iter().find(|m| lc.contains(**m)) {
            return Err(format!(
                "fixture pair {} carries bridging marker {m:?}. That is the CANONICAL \
                 variant, not a Phase-2-shortened record, and would measure the lexical \
                 lane instead of the retrieval-time consequence this bench exists for.",
                p.gold_id
            )
            .into());
        }
    }

    // ---- embedder: real, and the pinned one ----------------------------
    let (embedding, backend, dim, model_sha): (Arc<dyn EmbeddingProvider>, &str, usize, String) =
        if cli.deterministic_smoke {
            let dim = 64;
            (
                Arc::new(mnemo_core::embedding::DeterministicEmbedding::new(dim)),
                "deterministic",
                dim,
                "n/a (offline self-check; emits no result file)".to_string(),
            )
        } else {
            #[cfg(feature = "onnx")]
            {
                let path = cli.onnx_model.clone().ok_or_else(|| {
                    "needs --onnx-model (or MNEMO_ONNX_MODEL_PATH); see this file's header"
                        .to_string()
                })?;
                // FAIL CLOSED before anything is measured.
                let sha = verify_pinned(&path, Some(PINNED_MODEL_SHA256))?;
                let e = mnemo_core::embedding::onnx::OnnxEmbedding::new(
                    &path.to_string_lossy(),
                    cli.onnx_dim,
                )?;
                (Arc::new(e), "onnx", cli.onnx_dim, sha)
            }
            #[cfg(not(feature = "onnx"))]
            {
                let _ = verify_pinned;
                return Err(format!(
                    "built without --features onnx: this bench scores only on the pinned ONNX \
                     checkpoint {PINNED_MODEL_ID} (sha256 {PINNED_MODEL_SHA256}, from \
                     {PINNED_MODEL_SOURCE}). Rebuild with `--features onnx`."
                )
                .into());
            }
        };
    guard_real_embedder(&*embedding, backend)?;
    eprintln!("embedder OK: backend={backend} dim={dim} (semantic-capable, digest-pinned)");

    // ---- run ------------------------------------------------------------
    let cfg = Config {
        k: cli.k,
        seeds: cli.seeds.clone(),
        z_threshold: cli.z_threshold,
    };
    let trials = run_all(embedding.clone(), dim, &corpus, &fixture.pairs, &cfg).await;

    // Sensitivity across k, run here rather than described in prose so the
    // saturation caveat is checkable from the artifact alone.
    let mut k_sensitivity = Vec::new();
    for kk in cli.k_sweep.iter().copied() {
        let c = Config {
            k: kk,
            ..cfg.clone()
        };
        let tr = run_all(embedding.clone(), dim, &corpus, &fixture.pairs, &c).await;
        let po = summarise(&tr, Arm::Poison, false);
        let pn = summarise(&tr, Arm::Poison, true);
        let bo = summarise(&tr, Arm::Benign, false);
        k_sensitivity.push(serde_json::json!({
            "k": kk,
            "poison_exploited_off": po.exploited,
            "poison_exploited_on": pn.exploited,
            "benign_floor_off": bo.exploited,
            "defense_delta_off_minus_on": round4(po.exploited.rate - pn.exploited.rate),
            "poisoning_delta_poison_minus_benign": round4(po.exploited.rate - bo.exploited.rate),
        }));
    }

    let arms: Vec<_> = [
        (Arm::Poison, false),
        (Arm::Poison, true),
        (Arm::Benign, false),
        (Arm::Benign, true),
    ]
    .iter()
    .map(|(a, on)| summarise(&trials, *a, *on))
    .collect();

    let get = |arm: Arm, on: bool| {
        arms.iter()
            .find(|s| s.arm == arm.as_str() && s.detector_on == on)
            .expect("arm was run")
    };
    let p_off = get(Arm::Poison, false);
    let p_on = get(Arm::Poison, true);
    let b_off = get(Arm::Benign, false);
    let b_on = get(Arm::Benign, true);

    let n_distinct = fixture.pairs.len();
    let n_trials = p_off.exploited.n;

    // ---- console --------------------------------------------------------
    // Name the embedder that actually ran. Printing the pinned id during the
    // offline self-check would attribute a deterministic-hash number to MiniLM.
    let model_label = if cli.deterministic_smoke {
        "DeterministicEmbedding (offline self-check, NOT the pinned model)"
    } else {
        PINNED_MODEL_ID
    };
    println!(
        "\n=== Phase-3 exploitation (NON-ADAPTIVE) — {backend} {model_label} {dim}-dim ===\n\
         corpus n={n_distinct} distinct victim queries x {} seeds = {n_trials} trials, k={}",
        cli.seeds.len(),
        cli.k
    );
    let row = |label: &str, r: &Rate| {
        println!(
            "{label:<34} {:>7.3}  [{:.3}, {:.3}]   {}/{}",
            r.rate, r.ci95[0], r.ci95[1], r.successes, r.n
        );
    };
    println!("\n{:<34} {:>7}  {:<16} n", "", "rate", "95% CI");
    row("POISON exploited, detector OFF", &p_off.exploited);
    row("POISON exploited, detector ON", &p_on.exploited);
    row("BENIGN floor, detector OFF", &b_off.exploited);
    row("BENIGN floor, detector ON", &b_on.exploited);
    println!();
    row("POISON outranks gold, OFF", &p_off.outranks_gold);
    row("BENIGN outranks gold, OFF", &b_off.outranks_gold);
    println!();
    row("POISON quarantined (ON)", &p_on.quarantined);
    row("BENIGN false-quarantine (ON)", &b_on.quarantined);
    println!(
        "\nmean z-score vs pre-attack baseline: poison {:?}, benign {:?} (threshold {})",
        p_on.mean_z, b_on.mean_z, cli.z_threshold
    );

    let defense_delta = round4(p_off.exploited.rate - p_on.exploited.rate);
    let poisoning_delta = round4(p_off.exploited.rate - b_off.exploited.rate);
    println!(
        "\ndefense delta (OFF - ON)          {defense_delta:+.4}   <- what the detector buys\n\
         poisoning delta (poison - benign) {poisoning_delta:+.4}   <- what is attributable to poisoning\n\
                                                        rather than to topical retrieval"
    );

    if cli.deterministic_smoke {
        println!("\n[deterministic smoke: no result file written]");
        return Ok(());
    }

    // ---- artifact -------------------------------------------------------
    let payload = serde_json::json!({
        "bench": "minja_phase3",
        "scope": "Non-adaptive Phase-3 exploitation ONLY. Phases 1 and 2 (indication-prompt \
                  injection and progressive shortening) are NOT implemented: they are generative \
                  and need a model in the loop as the attacker. Per ADR 0003, removing the \
                  adaptive shortening step is exactly what makes a result stop being MINJA, so \
                  this MUST NOT be labelled a MINJA number.",
        "tracks_issue": 37,
        "adr": "docs/adr/0003-minja-procedure-harness.md",
        "corpus": {
            "dataset": "crates/mnemo-core/benches/data/longmemeval_m.jsonl",
            "sha256": corpus_sha256,
            "distinct_victim_queries": n_distinct,
        },
        "fixture": {
            "path": "bench/minja_phase3/fixtures/phase3_records.json",
            "sha256": fixture_sha256,
            "derivation": fixture.derivation,
        },
        "embedder": {
            "backend": backend,
            "dim": dim,
            "model_id": PINNED_MODEL_ID,
            "model_sha256": model_sha,
            "model_source": PINNED_MODEL_SOURCE,
            "pinned": true,
        },
        "storage_backend": "duckdb (in-memory)",
        "storage_backend_note": "DuckDB is the supported backend for this measurement. \
             `semantic`/`auto` recall on the pgvector index returns a typed BackendUnsupported \
             error rather than an empty result set (crates/mnemo-postgres/tests/\
             semantic_recall_fails_loud.rs), so the measurement cannot be taken there.",
        "config": { "k": cli.k, "seeds": cli.seeds, "z_threshold": cli.z_threshold },
        "n_trials": n_trials,
        "clustering": {
            "poison_detector_off": clustering(&trials, Arm::Poison, false, &cli.seeds),
            "poison_detector_on": clustering(&trials, Arm::Poison, true, &cli.seeds),
            "benign_detector_off": clustering(&trials, Arm::Benign, false, &cli.seeds),
            "note": "The independent unit is the victim QUERY, not the trial. The pooled \
                 interval over queries x seeds is narrower than the data earns; quote \
                 conservative_ci95.",
        },
        "k_sensitivity": k_sensitivity,
        "arms": arms,
        "headline": {
            "poison_exploited_detector_off": p_off.exploited,
            "poison_exploited_detector_on": p_on.exploited,
            "benign_floor_detector_off": b_off.exploited,
            "benign_false_quarantine_detector_on": b_on.quarantined,
            "defense_delta_off_minus_on": defense_delta,
            "poisoning_delta_poison_minus_benign": poisoning_delta,
        },
        "honesty": "Every rate carries its denominator and a Wilson-95 interval; a zero is \
             published as a measured null with its interval, never as a bare 0.0. The benign arm \
             is topic- and register-matched (identical opening clause and tags), so the \
             poisoning-specific effect is the poison-minus-benign delta, not the poison rate. \
             Trials are n_distinct victim queries repeated across seeds: the independent unit is \
             the query, not the trial, and the pooled interval is therefore optimistic.",
        "regeneration_command": "MNEMO_ONNX_MODEL_PATH=./model.onnx cargo run --release \
             --features onnx -p mnemo-minja-phase3-bench --bin minja_phase3_bench",
        "trials": trials,
    });

    let out = if cli.out.is_absolute() {
        cli.out.clone()
    } else {
        repo.join(&cli.out)
    };
    std::fs::create_dir_all(out.parent().unwrap())?;
    std::fs::write(&out, serde_json::to_string_pretty(&payload)? + "\n")?;
    println!("\nwrote {}", out.display());
    Ok(())
}

fn round4(x: f64) -> f64 {
    (x * 10_000.0).round() / 10_000.0
}
