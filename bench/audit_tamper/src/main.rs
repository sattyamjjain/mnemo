//! CLI entrypoint for the audit-log tamper-evidence bench.
//!
//! Runs four post-hoc attacks against a real, hash-chained `agent_events` log
//! and scores each with mnemo's shipped `verify_event_chain`, emitting a
//! **byte-stable** Markdown + JSON report (counts / rates / Wilson-95 only — no
//! timestamps or run-varying hashes in the body).
//!
//! Reproduce: `cargo run --release -p mnemo-audit-tamper-bench`

use std::path::PathBuf;

use clap::Parser;

use mnemo_audit_tamper_bench::{BenchConfig, render_json, render_markdown, run_bench};

/// Authored date recorded in the report. A fixed constant (not a wall clock) so
/// re-running the bench produces an identical file — `diff` two runs and they match.
const REPORT_DATE: &str = "2026-07-16";

#[derive(Parser, Debug)]
#[command(
    name = "audit_tamper_bench",
    about = "Adversarial tamper-evidence bench: delete / reorder / forge / truncate a real \
             agent_events hash chain, scored by the shipped verify_event_chain primitive."
)]
struct Cli {
    /// Independent tamper trials per attack (each attacks a different position).
    #[arg(long, default_value_t = 200)]
    trials: usize,
    /// Length of the legitimate `agent_events` chain that gets attacked.
    #[arg(long, default_value_t = 64)]
    chain_len: usize,
    /// Output directory for the byte-stable report.
    #[arg(long, default_value = "bench/audit_tamper/results")]
    out_dir: PathBuf,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let cfg = BenchConfig {
        trials: cli.trials,
        chain_len: cli.chain_len,
    };

    let outcome = run_bench(&cfg).await;

    std::fs::create_dir_all(&cli.out_dir)?;
    let md = render_markdown(&outcome, REPORT_DATE);
    let json = render_json(&outcome, REPORT_DATE);
    std::fs::write(cli.out_dir.join("audit_tamper.md"), &md)?;
    std::fs::write(
        cli.out_dir.join("audit_tamper.json"),
        serde_json::to_string_pretty(&json)? + "\n",
    )?;

    // Echo the report to stdout so a CI/`cargo run` invocation is self-documenting.
    print!("{md}");
    println!("wrote {}", cli.out_dir.join("audit_tamper.md").display());
    println!("wrote {}", cli.out_dir.join("audit_tamper.json").display());
    Ok(())
}
