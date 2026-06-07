//! `phase_cost` — phase-aware cost attribution + arXiv:2606.06448
//! recommendations scorecard (bench-only).
//!
//! Splits every benchmark scenario's cost into the paper's three phases
//! — **construction** (remember-path: embedding calls, prefill tokens,
//! write latency), **retrieval** (recall-path: ANN + BM25 + graph + RRF
//! latency, query tokens), and **generation** (downstream, estimated) —
//! and emits a per-phase Markdown table (tokens, wall-ms, $-estimate at
//! configurable per-1K rates).
//!
//! With `--scorecard-2606-06448` it instead (or additionally) renders
//! mnemo's PASS / PARTIAL / FAIL position against the paper's 10 §5
//! system recommendations as a 10-row Markdown table.
//!
//! This is wired through the existing `mnemo-locomo-bench` bench entry
//! point only — it imports nothing from the four access protocols
//! (MCP / REST / gRPC / pgwire) and changes no retrieval default.
//!
//! # Examples
//!
//! ```text
//! # Per-phase cost table to stdout:
//! cargo run --release --bin phase_cost -p mnemo-locomo-bench
//!
//! # Just the §5 recommendations scorecard:
//! cargo run --release --bin phase_cost -p mnemo-locomo-bench -- \
//!   --scorecard-2606-06448
//!
//! # Custom rates + write artifacts to a directory:
//! cargo run --release --bin phase_cost -p mnemo-locomo-bench -- \
//!   --embed-per-1k 0.00002 --input-per-1k 0.0003 --output-per-1k 0.0012 \
//!   --out-dir bench/locomo/results
//! ```

use std::path::PathBuf;

use clap::Parser;

use mnemo_locomo_bench::phase_cost::{
    PhaseOpts, Rates, render_phase_table, render_scorecard, run_phase_attribution,
};

#[derive(Parser, Debug)]
#[command(
    name = "phase_cost",
    about = "Phase-aware cost attribution + arXiv:2606.06448 recommendations scorecard"
)]
struct Cli {
    /// Render the 10-row arXiv:2606.06448 §5 recommendations scorecard.
    /// When set without `--with-phase-table`, only the scorecard is
    /// emitted (the phase attribution does not run).
    #[arg(long = "scorecard-2606-06448", default_value_t = false)]
    scorecard_2606_06448: bool,

    /// Force the per-phase cost table to also render even when
    /// `--scorecard-2606-06448` is set.
    #[arg(long, default_value_t = false)]
    with_phase_table: bool,

    /// Records written per scenario (construction units).
    #[arg(long, default_value_t = 64)]
    records: usize,

    /// Recall queries issued per scenario.
    #[arg(long, default_value_t = 16)]
    queries: usize,

    /// Top-k recall limit (drives the assembled-context token count).
    #[arg(long, default_value_t = 5)]
    recall_limit: usize,

    /// Output-token budget charged to the estimated generation phase
    /// per query.
    #[arg(long, default_value_t = 256)]
    output_tokens: u64,

    /// $ per 1K tokens for embedding calls.
    #[arg(long, default_value_t = 0.000_02)]
    embed_per_1k: f64,

    /// $ per 1K input/prefill tokens.
    #[arg(long, default_value_t = 0.000_15)]
    input_per_1k: f64,

    /// $ per 1K generated output tokens.
    #[arg(long, default_value_t = 0.000_60)]
    output_per_1k: f64,

    /// Optional directory to also write the Markdown report into.
    #[arg(long)]
    out_dir: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let rates = Rates {
        embed_per_1k: cli.embed_per_1k,
        input_per_1k: cli.input_per_1k,
        output_per_1k: cli.output_per_1k,
    };

    let mut report = String::new();

    // The phase table runs unless the caller asked for the scorecard
    // alone (i.e. `--scorecard-2606-06448` without `--with-phase-table`).
    let run_table = !cli.scorecard_2606_06448 || cli.with_phase_table;
    if run_table {
        let opts = PhaseOpts {
            records_per_scenario: cli.records,
            queries_per_scenario: cli.queries,
            recall_limit: cli.recall_limit,
            output_tokens_per_query: cli.output_tokens,
            rates,
        };
        let reports = run_phase_attribution(&opts).await;
        report.push_str(&render_phase_table(&reports, &rates));
    }

    if cli.scorecard_2606_06448 {
        if !report.is_empty() {
            report.push('\n');
        }
        report.push_str(&render_scorecard());
    }

    print!("{report}");

    if let Some(dir) = cli.out_dir {
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("phase_cost_2606-06448.md");
        std::fs::write(&path, &report)?;
        eprintln!("wrote {}", path.display());
    }

    Ok(())
}
