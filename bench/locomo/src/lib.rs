//! v0.4.1 (P0-1) — LoCoMo benchmark harness.
//!
//! LoCoMo (Long-Context Memory) is the dialogue-grounded recall
//! benchmark MemMachine (84.87%, 2026-04-24) and Memori (81.95%,
//! 2026-04-24) used to publish their public scores. mnemo has been
//! measuring LoCoMo internally for weeks but has never published a
//! number — this crate is the authenticated nightly runner that
//! closes that gap.
//!
//! Three pieces:
//!
//! 1. [`runner::LoCoMoRun`] — one authenticated execution against a
//!    pinned dataset slice + judge configuration.
//! 2. [`scoring::LoCoMoResult`] — official metric (overall +
//!    temporal + multi_session + open_domain) with cross-judge
//!    variance bands.
//! 3. [`judge::LoCoMoJudge`] — pluggable LLM judge (GPT-5.1 +
//!    Claude-3.7 Sonnet by default; swap to a deterministic mock
//!    for unit tests).
//!
//! The CI workflow `.github/workflows/locomo-nightly.yml` calls the
//! `mnemo-locomo` binary nightly with secrets-gated API keys; output
//! lands at `docs/benchmarks/locomo-<date>.md` with a SHA of the raw
//! run log so anyone can recompute.

pub mod judge;
pub mod phase_cost;
pub mod runner;
pub mod scoring;

pub use judge::{JudgeModel, JudgeVerdict, LoCoMoJudge, MockJudge};
pub use phase_cost::{
    Phase, PhaseCost, PhaseOpts, Rates, Recommendation, ScenarioPhases, Verdict,
    render_phase_table, render_scorecard, run_phase_attribution, scorecard_2606_06448,
};
pub use runner::{Dialogue, GoldAnswer, LoCoMoRun, RecallMode};
pub use scoring::{LoCoMoResult, ScoreSlice};
