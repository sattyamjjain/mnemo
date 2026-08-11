//! Compositional ("Salami") memory-poisoning fixture runner — issue #37.
//!
//! Prints a JSON report with a write-path SAVE rate and a RETRIEVAL-INFLUENCE
//! (assembly) rate — each with a Wilson 95% interval — for a set of
//! individually-benign-but-collectively-harmful memories (the "Salami" shape of
//! arXiv:2608.01637), alongside a benign control that shares the surface topic
//! but must NOT complete the harm.
//!
//! ```bash
//! cargo run -p mnemo-salami-poisoning-bench --release -- --trials 500
//! ```

use mnemo_salami_poisoning_bench::run_report;

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    // Tiny hand-rolled arg parse to avoid a clap dep for three knobs.
    let mut trials: u64 = 500;
    let mut background_n: usize = 16;
    let mut k: usize = 8;
    let args: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--trials" => {
                i += 1;
                trials = args.get(i).and_then(|v| v.parse().ok()).unwrap_or(trials);
            }
            "--background" => {
                i += 1;
                background_n = args
                    .get(i)
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(background_n);
            }
            "--top-k" => {
                i += 1;
                k = args.get(i).and_then(|v| v.parse().ok()).unwrap_or(k);
            }
            _ => {}
        }
        i += 1;
    }

    let report = run_report(trials, background_n, k).await;
    println!("{}", serde_json::to_string_pretty(&report).unwrap());
}
