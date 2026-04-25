use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// One bitemporal edge in the graph.
///
/// `valid_from` / `valid_to` are *fact validity* — when the relation
/// is true in the world. `recorded_at` is when the row was written
/// to the database. The two clocks lets us reconstruct an "as_of"
/// view that asks "what did we believe at time T?" without losing
/// later corrections.
///
/// `relation` is a free-form string today (`"works_at"`,
/// `"located_in"`, `"reports_to"`). Once the LLM extractor lands we
/// may pin it to a small enum, but doing that without real corpus
/// data risks codifying the wrong relation set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemporalEdge {
    pub id: Uuid,
    pub src: Uuid,
    pub dst: Uuid,
    pub relation: String,
    pub valid_from: DateTime<Utc>,
    /// `None` means "still true" — open-ended interval.
    pub valid_to: Option<DateTime<Utc>>,
    /// In `[0.0, 1.0]`. Higher confidences supersede lower ones during
    /// conflict resolution; ties go to the more recent `recorded_at`.
    pub confidence: f32,
    /// Audit-replay clock — when we wrote the row, regardless of the
    /// fact's own validity window.
    pub recorded_at: DateTime<Utc>,
}

impl TemporalEdge {
    /// Convenience constructor; sets `id = Uuid::now_v7()` and
    /// `recorded_at = Utc::now()`. Most call-sites should use this
    /// rather than building the struct directly.
    pub fn new(
        src: Uuid,
        dst: Uuid,
        relation: impl Into<String>,
        valid_from: DateTime<Utc>,
        valid_to: Option<DateTime<Utc>>,
        confidence: f32,
    ) -> Self {
        Self {
            id: Uuid::now_v7(),
            src,
            dst,
            relation: relation.into(),
            valid_from,
            valid_to,
            confidence: confidence.clamp(0.0, 1.0),
            recorded_at: Utc::now(),
        }
    }

    /// Is this edge valid at `as_of`?
    ///
    /// `valid_from <= as_of` and (`valid_to is None` or `as_of < valid_to`).
    pub fn valid_at(&self, as_of: DateTime<Utc>) -> bool {
        if as_of < self.valid_from {
            return false;
        }
        match self.valid_to {
            None => true,
            Some(end) => as_of < end,
        }
    }
}
