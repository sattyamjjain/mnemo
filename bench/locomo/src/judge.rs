//! Pluggable LoCoMo judge (v0.4.1 P0-1).

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum JudgeModel {
    Gpt5_1,
    Claude3_7Sonnet,
    Mock,
}

impl JudgeModel {
    pub fn as_str(&self) -> &'static str {
        match self {
            JudgeModel::Gpt5_1 => "gpt-5.1",
            JudgeModel::Claude3_7Sonnet => "claude-3.7-sonnet",
            JudgeModel::Mock => "mock",
        }
    }
}

/// One judge's verdict on one (dialogue, candidate) pair.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JudgeVerdict {
    pub correct: bool,
    pub confidence: f32,
    pub rationale: String,
    pub judge: JudgeModel,
}

#[async_trait]
pub trait LoCoMoJudge: Send + Sync {
    async fn score(
        &self,
        dialogue: &crate::runner::Dialogue,
        gold: &crate::runner::GoldAnswer,
        candidate: &str,
    ) -> JudgeVerdict;

    fn model(&self) -> JudgeModel;
}

/// Deterministic mock used by the smoke test. Marks the candidate
/// as correct if the gold answer string appears (case-insensitively)
/// in the candidate.
pub struct MockJudge;

#[async_trait]
impl LoCoMoJudge for MockJudge {
    async fn score(
        &self,
        _dialogue: &crate::runner::Dialogue,
        gold: &crate::runner::GoldAnswer,
        candidate: &str,
    ) -> JudgeVerdict {
        let correct = candidate
            .to_lowercase()
            .contains(&gold.answer.to_lowercase());
        JudgeVerdict {
            correct,
            confidence: if correct { 1.0 } else { 0.0 },
            rationale: if correct {
                "exact substring".into()
            } else {
                "no match".into()
            },
            judge: JudgeModel::Mock,
        }
    }
    fn model(&self) -> JudgeModel {
        JudgeModel::Mock
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::{Dialogue, GoldAnswer};

    fn dialogue() -> Dialogue {
        Dialogue {
            id: "d1".into(),
            turns: vec!["the patient is allergic to penicillin".into()],
        }
    }

    #[tokio::test]
    async fn mock_judge_substring_match() {
        let j = MockJudge;
        let g = GoldAnswer {
            id: "d1".into(),
            answer: "penicillin".into(),
        };
        let v = j
            .score(&dialogue(), &g, "the patient is allergic to penicillin")
            .await;
        assert!(v.correct);
        assert_eq!(v.judge, JudgeModel::Mock);
    }

    #[tokio::test]
    async fn mock_judge_substring_miss() {
        let j = MockJudge;
        let g = GoldAnswer {
            id: "d1".into(),
            answer: "amoxicillin".into(),
        };
        let v = j
            .score(&dialogue(), &g, "the patient is allergic to penicillin")
            .await;
        assert!(!v.correct);
    }
}
