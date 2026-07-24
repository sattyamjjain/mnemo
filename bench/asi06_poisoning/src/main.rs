//! `asi06_poisoning` — ASI06 auditable memory-poisoning-RESISTANCE benchmark.
//!
//! Measures the share of poisoning **cover-up / forgery** attempts that mnemo's
//! auditable layer (SHA-256 hash-chain + read-provenance HMAC) **rejects**, with
//! a Wilson 95% interval and a benign false-positive control. Deterministic and
//! fully offline — no embedder, no database, no network.
//!
//! Reproduce:
//!
//! ```text
//! cargo run --release -p mnemo-asi06-poisoning-bench --bin asi06_poisoning
//! ```

use std::path::PathBuf;

use clap::Parser;

use mnemo_asi06_poisoning_bench::{render_console, render_json, run_bench};

#[derive(Parser, Debug)]
#[command(
    name = "asi06_poisoning",
    about = "ASI06 auditable memory-poisoning-resistance benchmark (resistance + 95% CI + benign-FPR)"
)]
struct Cli {
    /// Cover-up attempts per attack family.
    #[arg(long, default_value_t = 500)]
    trials: usize,
    /// Legitimate operations for the benign false-positive control.
    #[arg(long, default_value_t = 300)]
    benign: usize,
    #[arg(long, default_value = "bench/results/asi06_poisoning.json")]
    out: PathBuf,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let outcome = run_bench(cli.trials, cli.benign);

    if let Some(parent) = cli.out.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = render_json(&outcome);
    std::fs::write(&cli.out, serde_json::to_string_pretty(&json)? + "\n")?;

    print!("{}", render_console(&outcome));
    println!("wrote {}", cli.out.display());
    Ok(())
}
