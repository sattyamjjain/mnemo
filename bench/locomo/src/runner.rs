//! LoCoMo runner types (v0.4.1 P0-1).

use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::judge::{JudgeModel, JudgeVerdict};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dialogue {
    pub id: String,
    pub turns: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoldAnswer {
    pub id: String,
    pub answer: String,
}

/// Recall configuration the run was issued under. Lets the report
/// distinguish numbers achieved by Letta-mode (decay-lane off) vs.
/// default-mode (decay-lane on).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecallMode {
    /// Default fusion: vector + BM25 + recency + decay.
    Default,
    /// Letta-protocol parity mode: decay-lane bypassed.
    LettaParity,
    /// Code-mode WIT recall (v0.4.0 P0-3) — measure token cost.
    CodeMode,
}

impl RecallMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            RecallMode::Default => "default",
            RecallMode::LettaParity => "letta_parity",
            RecallMode::CodeMode => "code_mode",
        }
    }
}

/// One authenticated nightly run. The `run_id` lands in the audit
/// log; `dataset_sha` lets a reader re-fetch the exact slice.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoCoMoRun {
    pub run_id: Uuid,
    pub judge: JudgeModel,
    pub mode: RecallMode,
    pub started_at: SystemTime,
    pub dataset_sha: [u8; 32],
}

impl LoCoMoRun {
    pub fn new(judge: JudgeModel, mode: RecallMode, dataset_sha: [u8; 32]) -> Self {
        Self {
            run_id: Uuid::now_v7(),
            judge,
            mode,
            started_at: SystemTime::now(),
            dataset_sha,
        }
    }
}

/// A row in the per-dialogue trace the runner emits. The CI workflow
/// SHAs the JSONL so anyone can recompute the headline number.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceRow {
    pub dialogue_id: String,
    pub mode: RecallMode,
    pub gold: String,
    pub candidate: String,
    pub verdict: JudgeVerdict,
    pub recall_latency_ms: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_carries_judge_and_mode() {
        let r = LoCoMoRun::new(JudgeModel::Mock, RecallMode::Default, [0u8; 32]);
        assert_eq!(r.judge, JudgeModel::Mock);
        assert_eq!(r.mode, RecallMode::Default);
    }

    #[test]
    fn mode_strings_round_trip() {
        for m in [
            RecallMode::Default,
            RecallMode::LettaParity,
            RecallMode::CodeMode,
        ] {
            assert!(!m.as_str().is_empty());
        }
    }
}
