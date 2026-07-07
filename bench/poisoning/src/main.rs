//! `poisoning_bench` — memory-poisoning defense-delta benchmark (bin).
//!
//! Reproduce: `cargo run --release -p mnemo-poisoning-bench`
//! Writes a byte-stable report to `bench/poisoning/results/poisoning_<date>.{md,json}`.

use std::path::PathBuf;

use clap::Parser;

use mnemo_poisoning_bench::{BenchConfig, DEFAULT_SEED, render_json, render_markdown, run_bench};

#[derive(Parser, Debug)]
#[command(name = "poisoning_bench")]
struct Cli {
    /// Trials per attack.
    #[arg(long, default_value_t = 200)]
    trials: usize,
    /// Top-k cutoff for "was the poison recalled".
    #[arg(long, default_value_t = 5)]
    k: usize,
    /// Deterministic seed (pinned in the report).
    #[arg(long, default_value_t = DEFAULT_SEED)]
    seed: u64,
    /// Benign corpus size for the AgentPoison low-rate scenario (< 0.1% poison).
    #[arg(long, default_value_t = 1001)]
    agentpoison_benign: usize,
    /// Report date `YYYY-MM-DD`; defaults to today (UTC). Pinned for byte-stability.
    #[arg(long)]
    date: Option<String>,
    /// Output directory.
    #[arg(long, default_value = "bench/poisoning/results")]
    out_dir: PathBuf,
}

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let cfg = BenchConfig {
        trials: cli.trials,
        k: cli.k,
        seed: cli.seed,
        agentpoison_benign: cli.agentpoison_benign,
        ..BenchConfig::default()
    };
    assert!(
        100.0 / (cfg.agentpoison_benign as f64 + 1.0) < 0.1,
        "AgentPoison must be a genuinely low-rate attack (< 0.1%); raise --agentpoison-benign"
    );

    let outcome = run_bench(&cfg).await;
    let md = render_markdown(&outcome, &cfg);
    let json = render_json(&outcome, &cfg);

    let date = cli
        .date
        .clone()
        .unwrap_or_else(|| chrono::Utc::now().format("%Y-%m-%d").to_string());
    std::fs::create_dir_all(&cli.out_dir)?;
    std::fs::write(cli.out_dir.join(format!("poisoning_{date}.md")), &md)?;
    std::fs::write(
        cli.out_dir.join(format!("poisoning_{date}.json")),
        serde_json::to_string_pretty(&json)?,
    )?;

    println!(
        "\n=== poisoning_bench (defense delta) — {} trials/attack, top-{}, seed {:#x} ===",
        cfg.trials, cfg.k, cfg.seed
    );
    for a in &outcome.attacks {
        println!(
            "  {:<38} ASR_off {:>6.1}%  ASR_on {:>6.1}%  delta {:+.1} pts   [{}]",
            a.name,
            a.asr_off.rate() * 100.0,
            a.asr_on.rate() * 100.0,
            a.delta() * 100.0,
            a.defense_lane,
        );
    }
    println!(
        "  benign control: {}/{} false-quarantine ({:.1}%)",
        outcome.benign_control_fp,
        outcome.benign_control_n,
        outcome.benign_control_fp as f64 / outcome.benign_control_n.max(1) as f64 * 100.0,
    );
    println!(
        "wrote {}",
        cli.out_dir.join(format!("poisoning_{date}.md")).display()
    );
    Ok(())
}
