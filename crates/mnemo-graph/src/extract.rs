//! Edge extraction from natural-language memory content.
//!
//! Behind the `graph-extract` feature flag, [`TemporalEdge::extract`]
//! is meant to call an LLM to pull relations out of incoming
//! memories so the graph stays up-to-date without operator effort.
//! v0.4.0-rc1 ships the gate but the extractor itself returns an
//! empty `Vec` — the prompt + ICL examples are still being tuned and
//! shipping a half-tuned extractor would land bad edges in everyone's
//! databases.
//!
//! The full extractor lands in v0.4.0 final. See issue tracking when
//! it cuts.

use crate::model::TemporalEdge;

impl TemporalEdge {
    /// Extract edges from `content` given `prior_edges` for the same
    /// subject.
    ///
    /// Today: stub that returns an empty `Vec`. The
    /// `MNEMO_GRAPH_EXTRACT_MODEL` env var (default
    /// `claude-haiku-4-5-20251001`) is documented but not yet read.
    /// `prior_edges` is accepted in the signature so call-sites don't
    /// have to change once the real extractor lands.
    pub fn extract(_content: &str, _prior_edges: &[TemporalEdge]) -> Vec<TemporalEdge> {
        // v0.4.0-rc1 stub. See module docs.
        Vec::new()
    }
}
