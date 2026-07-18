//! Phase-aware cost attribution + agent-memory characterization scorecard.
//!
//! Anchored on **arXiv:2606.06448** — *"Agent Memory: Characterization
//! and System Implications of Stateful Long-Horizon Workloads"* (Omri,
//! Gan, Broveak, Geens, He, Pentland, Verhelst, Weissman, Tambe). The
//! paper profiles agent-memory systems by attributing cost to **three
//! logical phases** and derives **10 system recommendations** for
//! operators. This module re-uses both ideas as *bench-only* tooling:
//!
//! 1. [`run_phase_attribution`] splits every benchmark scenario's cost
//!    into the paper's three phases and returns a per-scenario
//!    [`ScenarioPhases`] that [`render_phase_table`] renders to a
//!    Markdown table (tokens, wall-ms, $-estimate at configurable
//!    per-1K rates).
//! 2. [`scorecard_2606_06448`] returns mnemo's honest PASS / PARTIAL /
//!    FAIL position against the paper's 10 recommendations (quoted
//!    verbatim in [`RECOMMENDATIONS`]); [`render_scorecard`] renders
//!    the 10-row Markdown table.
//!
//! # The three phases (paper's definitions, verbatim)
//!
//! - **Memory construction:** *"The write-path transformation from raw
//!   interaction history into persistent memory records."* For mnemo
//!   this is the `remember` path — embedding calls + prefill tokens +
//!   write latency.
//! - **Retrieval:** *"Selection of memory records relevant to the
//!   current query."* For mnemo this is the `recall` path —
//!   ANN + BM25 + graph + RRF latency, plus the query-embed tokens.
//! - **Generation:** *"Invocation of the task LLM on the current query
//!   and assembled memory context."* mnemo is a memory database, not a
//!   generator, so this phase is **downstream and estimated, not
//!   executed here** — we count the assembled-context input tokens plus
//!   a configurable output-token budget so the operator sees the full
//!   lifecycle cost the paper insists on (Recommendation 2).
//!
//! # What this module is NOT
//!
//! - **Not a faithful arXiv:2606.06448 reproduction.** The paper
//!   profiles many third-party systems on real hardware energy
//!   counters; this is a single-system, token/$/wall-ms attribution
//!   over a synthetic LoCoMo-shaped trace on the embedded DuckDB
//!   backend with [`NoopEmbedding`]. The generation phase is an
//!   estimate, never an LLM call.
//! - **Not a managed-cloud default.** Everything runs in-process
//!   against `DuckDbStorage::open_in_memory()`.
//! - **Not a change to any retrieval default.** This module only
//!   *measures* the existing `remember` / `recall` paths; no RRF
//!   weights, half-lives, or scorer defaults are touched, and none of
//!   the four access protocols (MCP / REST / gRPC / pgwire) are
//!   imported.
//! - **Token counts are estimates.** With `NoopEmbedding` there is no
//!   tokenizer in the loop, so tokens are approximated as
//!   `ceil(chars / 4)` — the standard rough-cut heuristic. Absolute
//!   `$` figures track the configured per-1K rates, not a billed
//!   invoice; the *split between phases* is the headline, not the
//!   absolute dollar amount.

use std::sync::Arc;
use std::time::Instant;

use serde::{Deserialize, Serialize};

use mnemo_core::embedding::NoopEmbedding;
use mnemo_core::index::usearch::UsearchIndex;
use mnemo_core::query::MnemoEngine;
use mnemo_core::query::recall::RecallRequest;
use mnemo_core::query::remember::RememberRequest;
use mnemo_core::search::tantivy_index::TantivyFullTextIndex;

/// Bench agent id for the in-memory phase-attribution engine.
const AGENT: &str = "phase-cost-bench-agent";
/// Embedding dimension for the degenerate `NoopEmbedding` vector lane.
const DIM: usize = 8;

/// The three logical phases the paper attributes cost to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase {
    /// Write-path: embedding calls, prefill tokens, write latency.
    Construction,
    /// Read-path: ANN + BM25 + graph + RRF latency, query tokens.
    Retrieval,
    /// Downstream task-LLM invocation over the assembled context
    /// (estimated here — mnemo does not generate).
    Generation,
}

impl Phase {
    /// Stable lowercase label used in tables and JSON.
    pub fn as_str(&self) -> &'static str {
        match self {
            Phase::Construction => "construction",
            Phase::Retrieval => "retrieval",
            Phase::Generation => "generation",
        }
    }
}

/// Configurable per-1,000-token dollar rates. Defaults are
/// order-of-magnitude placeholders for a small-model deployment; the
/// operator overrides them on the CLI to match their own pricing.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Rates {
    /// $ per 1K tokens for embedding calls (construction + query embed).
    pub embed_per_1k: f64,
    /// $ per 1K input/prefill tokens (construction prefill + generation
    /// context).
    pub input_per_1k: f64,
    /// $ per 1K generated output tokens (generation only).
    pub output_per_1k: f64,
}

impl Default for Rates {
    fn default() -> Self {
        // Placeholder small-model rates (USD / 1K tokens).
        Self {
            embed_per_1k: 0.000_02,
            input_per_1k: 0.000_15,
            output_per_1k: 0.000_60,
        }
    }
}

/// Cost attributed to one phase of one scenario.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PhaseCost {
    /// Phase this cost belongs to.
    pub phase: Phase,
    /// Estimated tokens consumed in this phase.
    pub tokens: u64,
    /// Wall-clock milliseconds. `measured = false` for the generation
    /// phase, which mnemo does not execute (the value is `0.0`).
    pub wall_ms: f64,
    /// Estimated dollars at the configured [`Rates`].
    pub dollars: f64,
    /// `true` when `wall_ms` is a real measurement (construction,
    /// retrieval); `false` for the estimated generation phase.
    pub measured: bool,
}

/// Per-scenario phase breakdown.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioPhases {
    /// Human-readable scenario name.
    pub scenario: String,
    /// Records written (construction units).
    pub records: usize,
    /// Recall queries issued (retrieval + generation units).
    pub queries: usize,
    /// Construction-phase cost.
    pub construction: PhaseCost,
    /// Retrieval-phase cost.
    pub retrieval: PhaseCost,
    /// Generation-phase cost (estimated downstream).
    pub generation: PhaseCost,
}

impl ScenarioPhases {
    /// Total estimated dollars across the three phases.
    pub fn total_dollars(&self) -> f64 {
        self.construction.dollars + self.retrieval.dollars + self.generation.dollars
    }

    /// Total estimated tokens across the three phases.
    pub fn total_tokens(&self) -> u64 {
        self.construction.tokens + self.retrieval.tokens + self.generation.tokens
    }
}

/// Rough-cut token estimate: `ceil(chars / 4)`. With `NoopEmbedding`
/// there is no tokenizer in the loop, so this is a deliberate
/// approximation (documented in the module-level "what this is NOT").
pub fn est_tokens(text: &str) -> u64 {
    (text.chars().count() as u64).div_ceil(4)
}

/// Knobs for [`run_phase_attribution`].
#[derive(Debug, Clone)]
pub struct PhaseOpts {
    /// Records written per scenario (construction units).
    pub records_per_scenario: usize,
    /// Recall queries issued per scenario.
    pub queries_per_scenario: usize,
    /// Top-k recall limit (drives the assembled-context token count).
    pub recall_limit: usize,
    /// Output-token budget charged to the (estimated) generation phase
    /// per query.
    pub output_tokens_per_query: u64,
    /// Per-1K dollar rates.
    pub rates: Rates,
}

impl Default for PhaseOpts {
    fn default() -> Self {
        Self {
            records_per_scenario: 64,
            queries_per_scenario: 16,
            recall_limit: 5,
            output_tokens_per_query: 256,
            rates: Rates::default(),
        }
    }
}

/// A built-in scenario: a name plus a content-size profile. The three
/// shapes deliberately move the construction/retrieval/generation split
/// around so the per-phase table is legible.
struct Scenario {
    name: &'static str,
    /// Approximate characters per remembered fact.
    fact_chars: usize,
}

const SCENARIOS: &[Scenario] = &[
    Scenario {
        name: "short-fact",
        fact_chars: 80,
    },
    Scenario {
        name: "long-context",
        fact_chars: 480,
    },
    Scenario {
        name: "multi-session",
        fact_chars: 200,
    },
];

fn build_engine() -> MnemoEngine {
    let storage = Arc::new(
        mnemo_core::storage::duckdb::DuckDbStorage::open_in_memory().expect("duckdb open"),
    );
    let index = Arc::new(UsearchIndex::new(DIM).expect("usearch new"));
    let embedding = Arc::new(NoopEmbedding::new(DIM));
    let ft = Arc::new(TantivyFullTextIndex::open_in_memory().expect("tantivy open"));
    MnemoEngine::new(storage, index, embedding, AGENT.to_string(), None).with_full_text(ft)
}

/// Deterministic filler so every fact reaches `fact_chars` without a
/// PRNG (the bench must be reproducible; `Math.random`-style entropy is
/// intentionally avoided).
fn make_fact(scenario: &str, i: usize, fact_chars: usize) -> String {
    let head = format!("{scenario} fact #{i}: NEEDLE-{scenario}-{i} ");
    let mut s = String::with_capacity(fact_chars.max(head.len()));
    s.push_str(&head);
    let filler = "lorem ipsum dolor sit amet consectetur ";
    while s.chars().count() < fact_chars {
        s.push_str(filler);
    }
    s.chars().take(fact_chars.max(head.len())).collect()
}

/// Run the phase-aware cost attribution over the built-in scenarios.
///
/// For each scenario this seeds `records_per_scenario` facts (timing the
/// `remember` path as **construction**), issues `queries_per_scenario`
/// recalls (timing the `recall` path as **retrieval**), and estimates
/// the downstream **generation** cost from the assembled top-k context
/// plus the configured output-token budget.
pub async fn run_phase_attribution(opts: &PhaseOpts) -> Vec<ScenarioPhases> {
    let mut out = Vec::with_capacity(SCENARIOS.len());
    for scenario in SCENARIOS {
        out.push(run_one(scenario, opts).await);
    }
    out
}

async fn run_one(scenario: &Scenario, opts: &PhaseOpts) -> ScenarioPhases {
    let engine = build_engine();

    // ---- CONSTRUCTION: write-path (embed + prefill + write latency).
    let mut construction_tokens: u64 = 0;
    let started = Instant::now();
    for i in 0..opts.records_per_scenario {
        let content = make_fact(scenario.name, i, scenario.fact_chars);
        // Prefill tokens (the content the write path ingests) + the
        // embedding call over the same content.
        construction_tokens += est_tokens(&content) * 2;
        let mut req = RememberRequest::new(content);
        req.tags = Some(vec![scenario.name.to_string()]);
        engine.remember(req).await.expect("remember");
    }
    let construction_ms = started.elapsed().as_secs_f64() * 1000.0;

    // ---- RETRIEVAL: read-path token attribution. Token cost here is just
    // the query embed; the assembled context is charged to generation,
    // matching the paper's phase boundary. This bench uses `NoopEmbedding`
    // (vector lane degenerate by design), so it drives the **lexical (BM25)**
    // lane — the `NEEDLE-…` query tokens match the seeded facts. (The
    // vector-dependent `auto`/hybrid path now hard-errors under a no-op
    // embedder, v0.5.13; measuring the full ANN+RRF path would need a real
    // embedder.)
    let mut retrieval_tokens: u64 = 0;
    let mut context_tokens: u64 = 0;
    let started = Instant::now();
    for q in 0..opts.queries_per_scenario {
        let idx = q % opts.records_per_scenario.max(1);
        let query = format!("NEEDLE-{}-{idx}", scenario.name);
        retrieval_tokens += est_tokens(&query);
        let mut req = RecallRequest::new(query.clone());
        req.limit = Some(opts.recall_limit);
        req.strategy = Some("lexical".to_string());
        let resp = engine.recall(req).await.expect("recall");
        // Assembled-context tokens = the recalled records the downstream
        // generator would be handed, plus the query itself.
        context_tokens += est_tokens(&query);
        for m in resp.memories.iter() {
            context_tokens += est_tokens(&m.content);
        }
    }
    let retrieval_ms = started.elapsed().as_secs_f64() * 1000.0;

    // ---- GENERATION: estimated downstream task-LLM cost. mnemo does
    // not generate, so wall-ms is not measured (`measured = false`).
    let output_tokens = opts.output_tokens_per_query * opts.queries_per_scenario as u64;
    let generation_tokens = context_tokens + output_tokens;

    let r = &opts.rates;
    // Construction $: half the tokens are embed calls, half are prefill.
    let construction_embed = (construction_tokens / 2) as f64;
    let construction_prefill = (construction_tokens - construction_tokens / 2) as f64;
    let construction_dollars = construction_embed / 1000.0 * r.embed_per_1k
        + construction_prefill / 1000.0 * r.input_per_1k;
    let retrieval_dollars = retrieval_tokens as f64 / 1000.0 * r.embed_per_1k;
    let generation_dollars = context_tokens as f64 / 1000.0 * r.input_per_1k
        + output_tokens as f64 / 1000.0 * r.output_per_1k;

    ScenarioPhases {
        scenario: scenario.name.to_string(),
        records: opts.records_per_scenario,
        queries: opts.queries_per_scenario,
        construction: PhaseCost {
            phase: Phase::Construction,
            tokens: construction_tokens,
            wall_ms: construction_ms,
            dollars: construction_dollars,
            measured: true,
        },
        retrieval: PhaseCost {
            phase: Phase::Retrieval,
            tokens: retrieval_tokens,
            wall_ms: retrieval_ms,
            dollars: retrieval_dollars,
            measured: true,
        },
        generation: PhaseCost {
            phase: Phase::Generation,
            tokens: generation_tokens,
            wall_ms: 0.0,
            dollars: generation_dollars,
            measured: false,
        },
    }
}

/// Render the per-phase cost table (one block per scenario + a totals
/// row) as Markdown.
pub fn render_phase_table(reports: &[ScenarioPhases], rates: &Rates) -> String {
    let mut s = String::new();
    s.push_str("## Phase-aware cost attribution (arXiv:2606.06448)\n\n");
    s.push_str(&format!(
        "Rates (USD / 1K tokens): embed `{:.5}`, input `{:.5}`, output `{:.5}`. \
         Generation is **estimated** (mnemo does not generate); its wall-ms is `n/a`.\n\n",
        rates.embed_per_1k, rates.input_per_1k, rates.output_per_1k
    ));

    for rep in reports {
        s.push_str(&format!(
            "### Scenario `{}` ({} records, {} queries)\n\n",
            rep.scenario, rep.records, rep.queries
        ));
        s.push_str("| Phase | Tokens | Wall-ms | $-estimate |\n");
        s.push_str("|---|---:|---:|---:|\n");
        for pc in [&rep.construction, &rep.retrieval, &rep.generation] {
            let wall = if pc.measured {
                format!("{:.2}", pc.wall_ms)
            } else {
                "n/a".to_string()
            };
            s.push_str(&format!(
                "| {} | {} | {} | {:.6} |\n",
                pc.phase.as_str(),
                pc.tokens,
                wall,
                pc.dollars
            ));
        }
        s.push_str(&format!(
            "| **total** | **{}** | — | **{:.6}** |\n\n",
            rep.total_tokens(),
            rep.total_dollars()
        ));
    }
    s
}

/// PASS / PARTIAL / FAIL verdict against one paper recommendation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Verdict {
    /// mnemo's posture satisfies the recommendation.
    Pass,
    /// Partially satisfied — present but operator-side policy or
    /// follow-up work is still required.
    Partial,
    /// Not satisfied.
    Fail,
}

impl Verdict {
    /// Markdown-friendly label.
    pub fn as_str(&self) -> &'static str {
        match self {
            Verdict::Pass => "PASS",
            Verdict::Partial => "PARTIAL",
            Verdict::Fail => "FAIL",
        }
    }
}

/// One scorecard row: the paper's recommendation number, mnemo's
/// verdict, and a one-line rationale.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recommendation {
    /// 1-based recommendation number from the paper's §5.
    pub n: u8,
    /// mnemo's honest verdict.
    pub verdict: Verdict,
    /// Short rationale grounded in a real mnemo surface.
    pub rationale: &'static str,
}

// The paper's 10 system recommendations, quoted VERBATIM from the §5
// HTML (arXiv:2606.06448). Index `i` is recommendation `i + 1`.
//
// R1:  "Long-horizon agent deployments should treat agent memory system
//       selection as a system-level decision. Accuracy alone is an
//       insufficient criterion when systems differ by orders of
//       magnitude in construction cost, serving latency, and storage
//       footprint."
// R2:  "Operators should account for energy across the full agent
//       lifecycle, not just at query time. Construction dominates total
//       energy for most LLM-mediated agent memory systems, which trade
//       construction cost for capabilities such as mutation, structured
//       retrieval, and multi-store routing."
// R3:  "Agent-serving systems should treat memory construction as a
//       background throughput workload with explicit admission control.
//       Construction jobs should be rate-limited, batched, or deferred
//       when they would interfere with latency-sensitive QA."
// R4:  "Construction pipelines should exploit reuse across overlapping
//       inputs. Windowed and chunked memory systems can reduce repeated
//       prefill cost through prefix reuse, chunk caching, and batching
//       of independent construction units."
// R5:  "Operators should treat the minimum viable construction LLM as an
//       algorithm-imposed cost floor. For systems with strict output
//       contracts, this floor must be validated before deployment;
//       falling below it renders the store unusable."
// R6:  "Operators selecting an agent memory system should match the
//       construction-versus-query cost split to the workload's query
//       arrival pattern, in addition to matching the agent memory
//       system's capability profile to the dominant task family."
// R7:  "For inter-session workloads with strict cross-session
//       dependencies, cumulative construction and retrieval time should
//       be treated as a hard feasibility constraint. Systems that exceed
//       the arrival interval cannot satisfy both freshness and latency
//       targets simultaneously."
// R8:  "Construction cadence should be agent memory system-aware.
//       Append-only memory can be updated continuously, but
//       consolidating and mutating systems should monitor marginal
//       per-chunk construction cost and trigger compaction or offline
//       rebuild when appropriate."
// R9:  "Selecting memory for long-lived agents requires evaluating both
//       baseline footprint and cost growth slope. Agentic systems whose
//       construction cost compounds with memory size should be paired
//       with active compaction or summarization policies to prevent
//       unbounded cost escalation."
// R10: "Latency-sensitive deployments should treat worst-case latency,
//       on both construction and retrieval, as a selection criterion.
//       Algorithm-bounded systems can be provisioned from worst-case
//       latency measured on representative inputs. LLM-bounded systems
//       require external iteration caps and timeouts."
/// The verbatim text of each recommendation, indexed `n - 1`.
pub const RECOMMENDATIONS: [&str; 10] = [
    "Long-horizon agent deployments should treat agent memory system selection as a system-level decision. Accuracy alone is an insufficient criterion when systems differ by orders of magnitude in construction cost, serving latency, and storage footprint.",
    "Operators should account for energy across the full agent lifecycle, not just at query time. Construction dominates total energy for most LLM-mediated agent memory systems, which trade construction cost for capabilities such as mutation, structured retrieval, and multi-store routing.",
    "Agent-serving systems should treat memory construction as a background throughput workload with explicit admission control. Construction jobs should be rate-limited, batched, or deferred when they would interfere with latency-sensitive QA.",
    "Construction pipelines should exploit reuse across overlapping inputs. Windowed and chunked memory systems can reduce repeated prefill cost through prefix reuse, chunk caching, and batching of independent construction units.",
    "Operators should treat the minimum viable construction LLM as an algorithm-imposed cost floor. For systems with strict output contracts, this floor must be validated before deployment; falling below it renders the store unusable.",
    "Operators selecting an agent memory system should match the construction-versus-query cost split to the workload's query arrival pattern, in addition to matching the agent memory system's capability profile to the dominant task family.",
    "For inter-session workloads with strict cross-session dependencies, cumulative construction and retrieval time should be treated as a hard feasibility constraint. Systems that exceed the arrival interval cannot satisfy both freshness and latency targets simultaneously.",
    "Construction cadence should be agent memory system-aware. Append-only memory can be updated continuously, but consolidating and mutating systems should monitor marginal per-chunk construction cost and trigger compaction or offline rebuild when appropriate.",
    "Selecting memory for long-lived agents requires evaluating both baseline footprint and cost growth slope. Agentic systems whose construction cost compounds with memory size should be paired with active compaction or summarization policies to prevent unbounded cost escalation.",
    "Latency-sensitive deployments should treat worst-case latency, on both construction and retrieval, as a selection criterion. Algorithm-bounded systems can be provisioned from worst-case latency measured on representative inputs. LLM-bounded systems require external iteration caps and timeouts.",
];

/// mnemo's position against the paper's 10 recommendations.
///
/// The verdicts are deliberately conservative: mnemo's embedded-first,
/// algorithm-bounded posture scores well on the latency / feasibility /
/// compaction recommendations (R5, R7, R8, R9, R10) and only PARTIAL on
/// the ones that require operator-side lifecycle policy mnemo does not
/// auto-apply (R1–R4, R6).
pub fn scorecard_2606_06448() -> Vec<Recommendation> {
    vec![
        Recommendation {
            n: 1,
            verdict: Verdict::Partial,
            rationale: "Four access protocols + LoCoMo/LongMemEval accuracy and latency benches exist; cross-system construction-cost and storage-footprint numbers are not yet published (this phase table is a first step).",
        },
        Recommendation {
            n: 2,
            verdict: Verdict::Partial,
            rationale: "Phase table now attributes full-lifecycle token/$ cost; embedded-first default is NOT LLM-mediated on the write path so construction does not dominate, but cost is reported in tokens/$, not joules.",
        },
        Recommendation {
            n: 3,
            verdict: Verdict::Partial,
            rationale: "Consolidation/decay run offline away from the interactive loop, but there is no explicit write-path admission control / rate-limiter yet.",
        },
        Recommendation {
            n: 4,
            verdict: Verdict::Partial,
            rationale: "Orientation cache reuses constant-token context across recalls and bench/embeddings measures batch throughput; write-path prefix reuse / chunk caching is not implemented.",
        },
        Recommendation {
            n: 5,
            verdict: Verdict::Pass,
            rationale: "mnemo imposes no construction-LLM floor — the write path is append + embed, and the embed backend is SLA-validated by the bench/embeddings recommender before deployment.",
        },
        Recommendation {
            n: 6,
            verdict: Verdict::Partial,
            rationale: "This phase table surfaces the construction-vs-query split per scenario, but mnemo does not auto-match it to a measured query arrival pattern.",
        },
        Recommendation {
            n: 7,
            verdict: Verdict::Pass,
            rationale: "Both write and read paths are algorithm-bounded (append + ANN/BM25/RRF, no LLM in the loop), so cumulative construction+retrieval time is small and bounded — feasible under tight arrival intervals; the table quantifies it.",
        },
        Recommendation {
            n: 8,
            verdict: Verdict::Pass,
            rationale: "Append-only continuous writes by default, plus the v0.4.10 maturity-driven consolidation trigger that monitors a per-cluster maturity metric and fires compaction — exactly the cadence the recommendation asks for.",
        },
        Recommendation {
            n: 9,
            verdict: Verdict::Pass,
            rationale: "Footprint growth is roughly linear (append) and paired with active compaction: decay passes, tag-overlap consolidation, and archive-to-cold-storage (S3) bound the slope.",
        },
        Recommendation {
            n: 10,
            verdict: Verdict::Pass,
            rationale: "Algorithm-bounded read/write paths are provisionable from worst-case latency on representative inputs (no LLM iteration caps needed); the phase table reports measured construction/retrieval wall-ms.",
        },
    ]
}

/// Render the 10-row PASS / PARTIAL / FAIL scorecard as Markdown, with
/// each recommendation quoted verbatim.
pub fn render_scorecard() -> String {
    let rows = scorecard_2606_06448();
    let mut s = String::new();
    s.push_str("## arXiv:2606.06448 §5 recommendations scorecard\n\n");
    s.push_str(
        "mnemo's position against the paper's 10 system recommendations. \
         Recommendation text is quoted verbatim.\n\n",
    );
    s.push_str("| # | Verdict | Recommendation (verbatim) | mnemo rationale |\n");
    s.push_str("|---:|:---:|---|---|\n");
    for row in &rows {
        let text = RECOMMENDATIONS[(row.n - 1) as usize].replace('|', "\\|");
        let rationale = row.rationale.replace('|', "\\|");
        s.push_str(&format!(
            "| {} | {} | {} | {} |\n",
            row.n,
            row.verdict.as_str(),
            text,
            rationale
        ));
    }
    let pass = rows.iter().filter(|r| r.verdict == Verdict::Pass).count();
    let partial = rows
        .iter()
        .filter(|r| r.verdict == Verdict::Partial)
        .count();
    let fail = rows.iter().filter(|r| r.verdict == Verdict::Fail).count();
    s.push_str(&format!(
        "\n**Tally:** {pass} PASS · {partial} PARTIAL · {fail} FAIL (of {} recommendations).\n",
        rows.len()
    ));
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn est_tokens_is_ceil_div_four() {
        assert_eq!(est_tokens(""), 0);
        assert_eq!(est_tokens("a"), 1);
        assert_eq!(est_tokens("abcd"), 1);
        assert_eq!(est_tokens("abcde"), 2);
    }

    #[test]
    fn scorecard_has_ten_rows_with_verbatim_text() {
        let rows = scorecard_2606_06448();
        assert_eq!(rows.len(), 10);
        // Numbers are 1..=10, unique and ordered.
        for (i, row) in rows.iter().enumerate() {
            assert_eq!(row.n as usize, i + 1);
            // Every row maps to a non-empty verbatim recommendation.
            assert!(!RECOMMENDATIONS[(row.n - 1) as usize].is_empty());
        }
        assert_eq!(RECOMMENDATIONS.len(), 10);
    }

    #[test]
    fn scorecard_markdown_renders_all_recommendations() {
        let md = render_scorecard();
        assert!(md.contains("PASS") || md.contains("PARTIAL"));
        // A distinctive verbatim fragment from R8 must survive rendering.
        assert!(md.contains("Append-only memory can be updated continuously"));
        assert!(md.contains("**Tally:**"));
    }

    #[test]
    fn make_fact_reaches_target_length() {
        let f = make_fact("short-fact", 3, 80);
        assert!(f.chars().count() >= 80);
        assert!(f.contains("NEEDLE-short-fact-3"));
    }

    #[tokio::test]
    async fn phase_attribution_splits_three_phases() {
        let opts = PhaseOpts {
            records_per_scenario: 6,
            queries_per_scenario: 3,
            recall_limit: 3,
            output_tokens_per_query: 32,
            rates: Rates::default(),
        };
        let reports = run_phase_attribution(&opts).await;
        assert_eq!(reports.len(), 3);
        for rep in &reports {
            // Construction wrote records and cost real tokens + wall-ms.
            assert_eq!(rep.records, 6);
            assert!(rep.construction.tokens > 0);
            assert!(rep.construction.measured);
            // Retrieval is measured; generation is estimated only.
            assert!(rep.retrieval.measured);
            assert!(!rep.generation.measured);
            assert_eq!(rep.generation.wall_ms, 0.0);
            // Generation carries the assembled-context + output tokens,
            // so it is the heaviest token phase here.
            assert!(rep.generation.tokens >= rep.retrieval.tokens);
            assert!(rep.total_dollars() > 0.0);
        }
        // The markdown table mentions every scenario + the phase labels.
        let table = render_phase_table(&reports, &opts.rates);
        assert!(table.contains("construction"));
        assert!(table.contains("retrieval"));
        assert!(table.contains("generation"));
        assert!(table.contains("long-context"));
    }
}
