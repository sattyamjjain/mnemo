//! CLI entrypoint for the processing-log retention-conformance harness.
//!
//! Drives every deletion / compaction / cold-tier path against mnemo's
//! append-only `agent_events` log and asserts a retention floor held, emitting a
//! **byte-stable** Markdown + JSON artifact (counts / pass-fail only).
//!
//! Reproduce: `cargo run --release -p mnemo-retention-conformance-bench`

use std::path::PathBuf;

use clap::Parser;

use mnemo_retention_conformance_bench::{Config, render_markdown, run_report};

const REPORT_DATE: &str = "2026-07-19";

#[derive(Parser, Debug)]
#[command(
    name = "retention_conformance",
    about = "Offline, deterministic proof that mnemo's agent_events log survives every deletion path within a retention floor."
)]
struct Cli {
    /// Retention profile to check against ("dpdp", "eu-ai-act-art19", "hipaa").
    #[arg(long, default_value = "dpdp")]
    profile: String,
    /// Override the retention floor in days (defaults to the obligation's minimum).
    #[arg(long)]
    floor_days: Option<u32>,
    /// Memory-write events seeded per path.
    #[arg(long, default_value_t = 24)]
    records: usize,
    /// Traffic-bearing (model-response) events seeded per path.
    #[arg(long, default_value_t = 4)]
    traffic_events: usize,
    /// Output directory for the byte-stable report.
    #[arg(long, default_value = "bench/retention_conformance/results")]
    out_dir: PathBuf,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let cli = Cli::parse();
    let cfg = Config {
        profile: cli.profile,
        floor_days: cli.floor_days,
        records: cli.records,
        traffic_events: cli.traffic_events,
    };

    let report = run_report(&cfg).await?;

    std::fs::create_dir_all(&cli.out_dir)?;
    let md = render_markdown(&report, &cfg, REPORT_DATE);
    std::fs::write(cli.out_dir.join("retention_conformance.md"), &md)?;
    std::fs::write(
        cli.out_dir.join("retention_conformance.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "date": REPORT_DATE,
            "report": &report,
        }))? + "\n",
    )?;

    print!("{md}");
    println!(
        "wrote {}",
        cli.out_dir.join("retention_conformance.md").display()
    );
    println!(
        "wrote {}",
        cli.out_dir.join("retention_conformance.json").display()
    );

    if !report.conformant {
        return Err("retention conformance FAILED — see report".into());
    }
    Ok(())
}
