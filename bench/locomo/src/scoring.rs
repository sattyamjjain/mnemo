//! LoCoMo scoring + cross-judge variance bands (v0.4.1 P0-1).

use serde::{Deserialize, Serialize};

use crate::judge::JudgeVerdict;

/// One sliced score: e.g. `temporal` or `multi_session`. Variance
/// across judges is reported in `variance_pp` (percentage points).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScoreSlice {
    pub label: String,
    pub correct: u32,
    pub total: u32,
    pub variance_pp: f32,
}

impl ScoreSlice {
    pub fn pct(&self) -> f32 {
        if self.total == 0 {
            0.0
        } else {
            self.correct as f32 / self.total as f32 * 100.0
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoCoMoResult {
    pub overall: ScoreSlice,
    pub temporal: ScoreSlice,
    pub multi_session: ScoreSlice,
    pub open_domain: ScoreSlice,
}

impl LoCoMoResult {
    /// Aggregate per-dialogue verdicts into the four official slices.
    /// `slice_for(dialogue_id)` tells the function which slice the
    /// dialogue belongs to — the LoCoMo dataset publishes that
    /// labelling alongside each gold answer.
    pub fn from_verdicts(
        verdicts: &[(String, JudgeVerdict)],
        slice_for: impl Fn(&str) -> SliceTag,
    ) -> Self {
        let mut tally = [(0u32, 0u32); 4];
        for (id, v) in verdicts {
            let idx = slice_for(id) as usize;
            tally[idx].1 += 1;
            if v.correct {
                tally[idx].0 += 1;
            }
        }
        let mk = |label: &str, t: (u32, u32)| ScoreSlice {
            label: label.to_string(),
            correct: t.0,
            total: t.1,
            variance_pp: 0.0,
        };
        // Overall is sum, NOT mean of slices — matches the LoCoMo
        // paper's headline definition.
        let total_correct: u32 = tally.iter().map(|(c, _)| c).sum();
        let total_count: u32 = tally.iter().map(|(_, t)| t).sum();
        Self {
            overall: ScoreSlice {
                label: "overall".to_string(),
                correct: total_correct,
                total: total_count,
                variance_pp: 0.0,
            },
            temporal: mk("temporal", tally[SliceTag::Temporal as usize]),
            multi_session: mk("multi_session", tally[SliceTag::MultiSession as usize]),
            open_domain: mk("open_domain", tally[SliceTag::OpenDomain as usize]),
        }
    }

    /// Update each slice's `variance_pp` with the absolute delta
    /// against another judge's result. Used to surface judge drift.
    pub fn record_variance(&mut self, other: &LoCoMoResult) {
        self.overall.variance_pp = (self.overall.pct() - other.overall.pct()).abs();
        self.temporal.variance_pp = (self.temporal.pct() - other.temporal.pct()).abs();
        self.multi_session.variance_pp =
            (self.multi_session.pct() - other.multi_session.pct()).abs();
        self.open_domain.variance_pp = (self.open_domain.pct() - other.open_domain.pct()).abs();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SliceTag {
    Temporal,
    MultiSession,
    OpenDomain,
    Other,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::judge::JudgeModel;

    fn v(id: &str, correct: bool) -> (String, JudgeVerdict) {
        (
            id.to_string(),
            JudgeVerdict {
                correct,
                confidence: 1.0,
                rationale: "t".into(),
                judge: JudgeModel::Mock,
            },
        )
    }

    fn slice_label(id: &str) -> SliceTag {
        match id.chars().next() {
            Some('t') => SliceTag::Temporal,
            Some('m') => SliceTag::MultiSession,
            Some('o') => SliceTag::OpenDomain,
            _ => SliceTag::Other,
        }
    }

    #[test]
    fn aggregates_per_slice() {
        let verdicts = vec![
            v("t1", true),
            v("t2", false),
            v("m1", true),
            v("o1", true),
            v("o2", true),
        ];
        let r = LoCoMoResult::from_verdicts(&verdicts, slice_label);
        assert_eq!(r.temporal.correct, 1);
        assert_eq!(r.temporal.total, 2);
        assert_eq!(r.multi_session.correct, 1);
        assert_eq!(r.multi_session.total, 1);
        assert_eq!(r.open_domain.correct, 2);
        assert_eq!(r.open_domain.total, 2);
        assert_eq!(r.overall.correct, 4);
        assert_eq!(r.overall.total, 5);
    }

    #[test]
    fn variance_records_absolute_delta() {
        let verdicts_a = vec![v("t1", true), v("t2", true), v("m1", false)];
        let verdicts_b = vec![v("t1", true), v("t2", false), v("m1", false)];
        let mut a = LoCoMoResult::from_verdicts(&verdicts_a, slice_label);
        let b = LoCoMoResult::from_verdicts(&verdicts_b, slice_label);
        a.record_variance(&b);
        // a.temporal = 100%, b.temporal = 50% → 50 pp gap.
        assert!((a.temporal.variance_pp - 50.0).abs() < 0.01);
    }
}
