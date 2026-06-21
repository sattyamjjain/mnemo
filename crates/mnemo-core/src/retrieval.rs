//! v0.4.4 — `RetrievalMode` typed enum + 5 starter `HarnessAware`
//! adapters.
//!
//! # What this module is
//!
//! A typed superset of the existing
//! [`RecallRequest::strategy: Option<String>`][crate::query::recall::RecallRequest]
//! field, plus a new `HarnessAware` variant that lets the recall
//! response envelope be reshaped per agent harness (Claude Code,
//! Codex, Gemini CLI, Chronos, generic) per the framing in arXiv
//! 2605.15184: *"overall scores still depend strongly on which
//! harness and tool-calling style is used, even when the underlying
//! conversation data are the same."*
//!
//! # Backwards-compatible introduction
//!
//! [`RecallRequest`][crate::query::recall::RecallRequest] gains an
//! optional `mode: Option<RetrievalMode>` field in this release. The
//! legacy `strategy: Option<String>` field stays in place; if `mode`
//! is set it takes precedence, otherwise the engine continues to
//! parse `strategy` exactly as before. Existing SDK callers
//! (Python `mnemo-db`, TypeScript `@mndfreek/mnemo-sdk`, Go
//! `mnemo.Recall`) continue to work unchanged because they all
//! marshal through the string-typed field.
//!
//! # `HarnessAware` semantics
//!
//! `HarnessAware { harness, format }` does NOT change which records
//! are retrieved — under the hood it delegates to the default
//! `HybridRrf` retrieval path. What it changes is how the
//! [`crate::query::recall::ScoredMemory`] hits are *shaped* into a
//! string envelope that a specific agent harness prefers (inline
//! fenced blocks, file-based side-channel pointers with line
//! numbers, generic line-numbered list, …). The
//! [`HarnessEnvelope::shape`] method returns the rendered envelope
//! string; the recall response continues to carry the typed
//! `ScoredMemory` hits so downstream consumers that want the typed
//! payload are not blocked.
//!
//! # Not in scope for v0.4.4
//!
//! - **No SDK ripple.** The Python / TypeScript / Go SDKs are NOT
//!   updated in this release. They continue to use the string-typed
//!   `strategy` field. SDK migration to a typed `mode` field is a
//!   follow-up tracked separately.
//! - **No REST / gRPC / pgwire schema bump.** The new `mode` field
//!   serialises through the same `RecallRequest` Serde definition;
//!   inbound JSON that omits `mode` continues to work.
//! - **No envelope-trait stabilisation.** The
//!   [`HarnessEnvelope`] trait + the five adapter structs are
//!   intentionally minimal — each adapter produces a deterministic
//!   string with the shape the corresponding harness expects, but
//!   the *contents* of those strings are not a stability surface in
//!   v0.4.4. Operators relying on a specific envelope shape should
//!   pin the mnemo minor version.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::query::recall::ScoredMemory;

/// Typed recall strategy. Superset of the legacy
/// `RecallRequest.strategy: Option<String>` API — the variant ↔ string
/// mapping is documented on each variant.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetrievalMode {
    /// Maps to legacy `strategy = "semantic"` — vector-only path.
    VectorOnly,
    /// Maps to legacy `strategy = "lexical"` — Tantivy BM25-only
    /// path.
    Bm25Only,
    /// Maps to legacy `strategy = "auto"` — default RRF fusion across
    /// vector + BM25 + recency + decay. Weight overrides continue to
    /// be carried on [`RecallRequest.hybrid_weights`][crate::query::recall::RecallRequest::hybrid_weights]
    /// and [`RecallRequest.rrf_k`][crate::query::recall::RecallRequest::rrf_k]
    /// to keep wire compatibility with v0.4.3 SDK clients.
    HybridRrf,
    /// Maps to legacy `strategy = "graph"` — vector-seeded +
    /// graph-expanded path.
    Graph,
    /// New in v0.4.4 — harness-aware envelope reshaping. Inside the
    /// recall path this delegates to [`RetrievalMode::HybridRrf`];
    /// the difference is post-processing: a
    /// [`HarnessEnvelope`] adapter renders the typed
    /// [`ScoredMemory`] hits into a string envelope shaped for the
    /// nominated agent harness.
    HarnessAware {
        harness: HarnessKind,
        format: EnvelopeFormat,
    },
    /// New in v0.4.15 — **domain-scoped** recall (anti
    /// vector-search-dilution; MASDR-RAG, arXiv:2606.11350). Restricts
    /// the candidate set to a metadata-defined sub-corpus *before* the
    /// dense similarity step, then runs a single vector pass — so at
    /// scale, off-domain-but-semantically-similar records cannot dilute
    /// the top-k. The predicate rides on
    /// [`RecallRequest.domain_scope`][crate::query::recall::RecallRequest::domain_scope]
    /// (a [`DomainScope`]); selecting this mode without a predicate
    /// degrades gracefully to a plain vector pass.
    DomainScoped,
    /// New in v0.5.1 — **active reconstruction** (MRAgent,
    /// arXiv:2606.06036). Retrieves candidate memories for the cue, walks
    /// the existing memory-graph edges to gather linked/causal context,
    /// and synthesises a deterministic *belief-state* summary node that
    /// the caller receives ALONGSIDE the raw hits in
    /// [`RecallResponse.reconstruction`][crate::query::recall::RecallResponse::reconstruction].
    /// Additive: the `memories` top-k is exactly what the default hybrid
    /// (`auto`) path returns, so the raw read path is unchanged — this is
    /// an option to A/B reconstruction vs. plain retrieval, not a
    /// replacement for it.
    Reconstruct,
}

impl RetrievalMode {
    /// Map the typed variant back to the legacy strategy string the
    /// engine dispatcher understands. `HarnessAware` delegates to
    /// `"auto"` (HybridRrf) for the underlying retrieval; the envelope
    /// adapter handles the post-processing separately.
    pub fn to_strategy_str(&self) -> &'static str {
        match self {
            Self::VectorOnly => "semantic",
            Self::Bm25Only => "lexical",
            Self::HybridRrf | Self::HarnessAware { .. } => "auto",
            Self::Graph => "graph",
            Self::DomainScoped => "domain_scoped",
            Self::Reconstruct => "reconstruct",
        }
    }

    /// Optional envelope adapter for `HarnessAware`; returns `None`
    /// for every other variant. Each adapter is a unit struct (or
    /// a small config struct); call
    /// [`HarnessEnvelope::shape`] to render the envelope string.
    pub fn envelope_adapter(&self) -> Option<Box<dyn HarnessEnvelope>> {
        let Self::HarnessAware { harness, format } = self else {
            return None;
        };
        Some(adapter_for(*harness, format.clone()))
    }
}

/// Metadata predicate that defines a recall **sub-corpus** for
/// [`RetrievalMode::DomainScoped`] (MASDR-RAG, arXiv:2606.11350).
///
/// A record is *in domain* iff it matches **every** populated field
/// (logical AND); empty fields are ignored. `org_id` matches the record's
/// tenant; `namespace` matches either a record tag or
/// `metadata["namespace"]`; `doc_class` matches `metadata["doc_class"]`;
/// `tags` requires the record to carry **all** listed tags. An entirely
/// empty scope ([`DomainScope::is_empty`]) imposes no restriction.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DomainScope {
    /// Restrict to a single tenant / organization.
    pub org_id: Option<String>,
    /// Restrict to a namespace — matched against the record's tags or
    /// its `metadata["namespace"]` value.
    pub namespace: Option<String>,
    /// Restrict to a document class — matched against the record's
    /// `metadata["doc_class"]` value.
    pub doc_class: Option<String>,
    /// Require the record to carry all of these tags.
    pub tags: Option<Vec<String>>,
}

impl DomainScope {
    /// `true` when no predicate field is set (imposes no restriction).
    pub fn is_empty(&self) -> bool {
        self.org_id.is_none()
            && self.namespace.is_none()
            && self.doc_class.is_none()
            && self.tags.as_ref().map(|t| t.is_empty()).unwrap_or(true)
    }

    /// Whether `record` belongs to this sub-corpus (logical AND over the
    /// populated fields).
    pub fn matches(&self, record: &crate::model::memory::MemoryRecord) -> bool {
        if let Some(ref org) = self.org_id
            && record.org_id.as_deref() != Some(org.as_str())
        {
            return false;
        }
        if let Some(ref ns) = self.namespace {
            let tag_hit = record.tags.iter().any(|t| t == ns);
            let meta_hit = record
                .metadata
                .get("namespace")
                .and_then(|v| v.as_str())
                .map(|v| v == ns)
                .unwrap_or(false);
            if !tag_hit && !meta_hit {
                return false;
            }
        }
        if let Some(ref dc) = self.doc_class {
            let meta_hit = record
                .metadata
                .get("doc_class")
                .and_then(|v| v.as_str())
                .map(|v| v == dc)
                .unwrap_or(false);
            if !meta_hit {
                return false;
            }
        }
        if let Some(ref tags) = self.tags
            && !tags.iter().all(|t| record.tags.contains(t))
        {
            return false;
        }
        true
    }
}

/// Which agent harness the response envelope should be shaped for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HarnessKind {
    ClaudeCode,
    Codex,
    GeminiCli,
    Chronos,
    Generic,
}

/// Where the envelope payload lives — inline in the response, written
/// to a file the harness reads via a side-channel pointer, or written
/// to a side-channel out-of-band stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnvelopeFormat {
    Inline,
    FileBased { path_root: PathBuf },
    SideChannel,
}

/// Trait implemented by each per-harness adapter. The contract is
/// minimal: take a slice of typed [`ScoredMemory`] hits and return a
/// rendered string envelope shaped for the harness.
pub trait HarnessEnvelope {
    fn shape(&self, hits: &[ScoredMemory]) -> String;
}

fn adapter_for(kind: HarnessKind, format: EnvelopeFormat) -> Box<dyn HarnessEnvelope> {
    match kind {
        HarnessKind::ClaudeCode => Box::new(ClaudeCodeEnvelope {
            inline: matches!(format, EnvelopeFormat::Inline),
        }),
        HarnessKind::Codex => Box::new(CodexEnvelope {
            file_based: matches!(format, EnvelopeFormat::FileBased { .. }),
        }),
        HarnessKind::GeminiCli => Box::new(GeminiCliEnvelope),
        HarnessKind::Chronos => Box::new(ChronosEnvelope),
        HarnessKind::Generic => Box::new(GenericEnvelope),
    }
}

/// Claude Code envelope — fenced markdown blocks with `recall://<id>`
/// anchors for inline; line-numbered file-pointer summary for the
/// non-inline branch.
#[derive(Debug, Clone, Copy)]
pub struct ClaudeCodeEnvelope {
    pub inline: bool,
}

impl HarnessEnvelope for ClaudeCodeEnvelope {
    fn shape(&self, hits: &[ScoredMemory]) -> String {
        let mut out = String::new();
        out.push_str("# mnemo.recall (Claude Code envelope)\n\n");
        for (i, m) in hits.iter().enumerate() {
            if self.inline {
                out.push_str(&format!(
                    "## hit {} (recall://{} • score {:.3})\n```\n{}\n```\n\n",
                    i + 1,
                    m.id,
                    m.score,
                    m.content
                ));
            } else {
                let first_line = m.content.lines().next().unwrap_or("").trim();
                out.push_str(&format!(
                    "- hit {} → `recall://{}` (score {:.3}): {}\n",
                    i + 1,
                    m.id,
                    m.score,
                    first_line
                ));
            }
        }
        out
    }
}

/// Codex envelope — file-based by default (writes hits to a path-root
/// the caller chose), with an inline JSON pointer summary in the
/// response. The Inline branch keeps the raw content in the response.
#[derive(Debug, Clone, Copy)]
pub struct CodexEnvelope {
    pub file_based: bool,
}

impl HarnessEnvelope for CodexEnvelope {
    fn shape(&self, hits: &[ScoredMemory]) -> String {
        if self.file_based {
            let pointers: Vec<String> = hits
                .iter()
                .map(|m| format!("{{\"id\":\"{}\",\"score\":{:.3}}}", m.id, m.score))
                .collect();
            format!(
                "{{\"envelope\":\"codex_file_based\",\"hits\":[{}]}}",
                pointers.join(",")
            )
        } else {
            let blocks: Vec<String> = hits
                .iter()
                .map(|m| {
                    format!(
                        "{{\"id\":\"{}\",\"score\":{:.3},\"content\":{}}}",
                        m.id,
                        m.score,
                        serde_json::to_string(&m.content).unwrap_or_default()
                    )
                })
                .collect();
            format!(
                "{{\"envelope\":\"codex_inline\",\"hits\":[{}]}}",
                blocks.join(",")
            )
        }
    }
}

/// Gemini CLI envelope — plain numbered list with `[N]` markers + the
/// hit content; tool-call-style framing the Gemini CLI surfaces well.
#[derive(Debug, Clone, Copy)]
pub struct GeminiCliEnvelope;

impl HarnessEnvelope for GeminiCliEnvelope {
    fn shape(&self, hits: &[ScoredMemory]) -> String {
        let mut out = String::new();
        out.push_str("mnemo recall (Gemini CLI envelope)\n");
        for (i, m) in hits.iter().enumerate() {
            out.push_str(&format!(
                "[{}] score={:.3} id={} — {}\n",
                i + 1,
                m.score,
                m.id,
                m.content
            ));
        }
        out
    }
}

/// Chronos envelope — timeline-shaped: one line per hit with the hit
/// `id`, score, and the first line of content. Chronos prefers
/// temporally-anchored single-line summaries.
#[derive(Debug, Clone, Copy)]
pub struct ChronosEnvelope;

impl HarnessEnvelope for ChronosEnvelope {
    fn shape(&self, hits: &[ScoredMemory]) -> String {
        let mut out = String::new();
        out.push_str("chronos recall envelope\n");
        for m in hits {
            let first_line = m.content.lines().next().unwrap_or("").trim();
            out.push_str(&format!("t={:.3} id={} :: {}\n", m.score, m.id, first_line));
        }
        out
    }
}

/// Generic envelope — minimal `id\tscore\tcontent` TSV one line per
/// hit. The fallback when no harness-specific adapter applies.
#[derive(Debug, Clone, Copy)]
pub struct GenericEnvelope;

impl HarnessEnvelope for GenericEnvelope {
    fn shape(&self, hits: &[ScoredMemory]) -> String {
        let mut out = String::new();
        for m in hits {
            // TSV-safe: replace tabs/newlines in content so the
            // generic envelope stays parseable.
            let content_safe = m.content.replace(['\t', '\n', '\r'], " ");
            out.push_str(&format!("{}\t{:.3}\t{}\n", m.id, m.score, content_safe));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::memory::{MemoryType, Scope};
    use uuid::Uuid;

    fn make_hit(content: &str, score: f32) -> ScoredMemory {
        ScoredMemory {
            id: Uuid::now_v7(),
            content: content.to_string(),
            agent_id: "test-agent".to_string(),
            memory_type: MemoryType::Episodic,
            scope: Scope::Private,
            importance: 0.5,
            tags: vec![],
            metadata: serde_json::Value::Null,
            score,
            access_count: 0,
            created_at: "2026-05-17T00:00:00Z".to_string(),
            updated_at: "2026-05-17T00:00:00Z".to_string(),
            score_breakdown: None,
        }
    }

    #[test]
    fn retrieval_mode_round_trip_strategy_string() {
        assert_eq!(RetrievalMode::VectorOnly.to_strategy_str(), "semantic");
        assert_eq!(RetrievalMode::Bm25Only.to_strategy_str(), "lexical");
        assert_eq!(RetrievalMode::HybridRrf.to_strategy_str(), "auto");
        assert_eq!(RetrievalMode::Graph.to_strategy_str(), "graph");
        assert_eq!(
            RetrievalMode::DomainScoped.to_strategy_str(),
            "domain_scoped"
        );
        assert_eq!(RetrievalMode::Reconstruct.to_strategy_str(), "reconstruct");
        let harness = RetrievalMode::HarnessAware {
            harness: HarnessKind::ClaudeCode,
            format: EnvelopeFormat::Inline,
        };
        // HarnessAware delegates to "auto" for the underlying
        // retrieval — the adapter handles envelope post-processing.
        assert_eq!(harness.to_strategy_str(), "auto");
    }

    fn rec(
        org: Option<&str>,
        tags: &[&str],
        metadata: serde_json::Value,
    ) -> crate::model::memory::MemoryRecord {
        use crate::model::memory::{ConsolidationState, SourceType};
        crate::model::memory::MemoryRecord {
            id: Uuid::now_v7(),
            agent_id: "a".to_string(),
            content: "c".to_string(),
            memory_type: MemoryType::Episodic,
            scope: Scope::Private,
            importance: 0.5,
            tags: tags.iter().map(|t| t.to_string()).collect(),
            metadata,
            embedding: None,
            content_hash: vec![],
            prev_hash: None,
            source_type: SourceType::Agent,
            source_id: None,
            consolidation_state: ConsolidationState::Raw,
            access_count: 0,
            org_id: org.map(str::to_string),
            thread_id: None,
            created_at: "2026-06-13T00:00:00Z".to_string(),
            updated_at: "2026-06-13T00:00:00Z".to_string(),
            last_accessed_at: None,
            expires_at: None,
            deleted_at: None,
            decay_rate: None,
            created_by: None,
            version: 1,
            prev_version_id: None,
            quarantined: false,
            quarantine_reason: None,
            decay_function: None,
        }
    }

    #[test]
    fn domain_scope_matches_logical_and() {
        // Empty scope matches everything.
        let empty = DomainScope::default();
        assert!(empty.is_empty());
        assert!(empty.matches(&rec(Some("alpha"), &[], serde_json::Value::Null)));

        // org_id predicate.
        let by_org = DomainScope {
            org_id: Some("alpha".to_string()),
            ..Default::default()
        };
        assert!(by_org.matches(&rec(Some("alpha"), &[], serde_json::Value::Null)));
        assert!(!by_org.matches(&rec(Some("beta"), &[], serde_json::Value::Null)));

        // namespace via tag OR metadata.
        let by_ns = DomainScope {
            namespace: Some("legal".to_string()),
            ..Default::default()
        };
        assert!(by_ns.matches(&rec(None, &["legal"], serde_json::Value::Null)));
        assert!(by_ns.matches(&rec(None, &[], serde_json::json!({"namespace": "legal"}))));
        assert!(!by_ns.matches(&rec(None, &["hr"], serde_json::json!({"namespace": "hr"}))));

        // doc_class via metadata; AND with org.
        let combo = DomainScope {
            org_id: Some("alpha".to_string()),
            doc_class: Some("contract".to_string()),
            ..Default::default()
        };
        assert!(combo.matches(&rec(
            Some("alpha"),
            &[],
            serde_json::json!({"doc_class": "contract"})
        )));
        // right doc_class, wrong org → rejected (AND).
        assert!(!combo.matches(&rec(
            Some("beta"),
            &[],
            serde_json::json!({"doc_class": "contract"})
        )));
        // right org, wrong doc_class → rejected.
        assert!(!combo.matches(&rec(
            Some("alpha"),
            &[],
            serde_json::json!({"doc_class": "memo"})
        )));
    }

    #[test]
    fn retrieval_mode_serde_round_trip() {
        for mode in [
            RetrievalMode::VectorOnly,
            RetrievalMode::Bm25Only,
            RetrievalMode::HybridRrf,
            RetrievalMode::Graph,
            RetrievalMode::DomainScoped,
            RetrievalMode::Reconstruct,
            RetrievalMode::HarnessAware {
                harness: HarnessKind::ClaudeCode,
                format: EnvelopeFormat::Inline,
            },
            RetrievalMode::HarnessAware {
                harness: HarnessKind::Codex,
                format: EnvelopeFormat::FileBased {
                    path_root: PathBuf::from("/tmp/codex"),
                },
            },
            RetrievalMode::HarnessAware {
                harness: HarnessKind::Generic,
                format: EnvelopeFormat::SideChannel,
            },
        ] {
            let s = serde_json::to_string(&mode).unwrap();
            let back: RetrievalMode = serde_json::from_str(&s).unwrap();
            assert_eq!(mode, back, "round-trip failed for {mode:?} via {s}");
        }
    }

    #[test]
    fn harness_aware_returns_envelope_adapter() {
        let mode = RetrievalMode::HarnessAware {
            harness: HarnessKind::ClaudeCode,
            format: EnvelopeFormat::Inline,
        };
        assert!(mode.envelope_adapter().is_some());
        assert!(RetrievalMode::HybridRrf.envelope_adapter().is_none());
    }

    #[test]
    fn five_adapters_produce_distinct_envelope_shapes() {
        let hits = vec![
            make_hit("first hit content line\nsecond line", 0.91),
            make_hit("another hit", 0.42),
        ];
        let cc = ClaudeCodeEnvelope { inline: true }.shape(&hits);
        let codex = CodexEnvelope { file_based: true }.shape(&hits);
        let gemini = GeminiCliEnvelope.shape(&hits);
        let chronos = ChronosEnvelope.shape(&hits);
        let generic = GenericEnvelope.shape(&hits);
        // Each adapter must produce a distinct shape — the whole
        // point of HarnessAware is per-harness reshaping.
        let shapes = [&cc, &codex, &gemini, &chronos, &generic];
        for (i, a) in shapes.iter().enumerate() {
            for (j, b) in shapes.iter().enumerate() {
                if i != j {
                    assert_ne!(
                        a, b,
                        "adapter shapes {} and {} collided (both produced:\n{a})",
                        i, j
                    );
                }
            }
        }
    }

    #[test]
    fn claude_code_envelope_inline_vs_non_inline_differ() {
        let hits = vec![make_hit("hello world", 0.5)];
        let inline = ClaudeCodeEnvelope { inline: true }.shape(&hits);
        let non_inline = ClaudeCodeEnvelope { inline: false }.shape(&hits);
        assert!(inline.contains("```"), "inline must contain fenced block");
        assert!(
            !non_inline.contains("```"),
            "non-inline must not contain fenced block"
        );
    }

    #[test]
    fn generic_envelope_is_tsv_safe() {
        let hits = vec![make_hit("has\ttab\nand newline", 0.5)];
        let env = GenericEnvelope.shape(&hits);
        // Exactly one record line — the inner \t and \n in content
        // must have been replaced with spaces.
        assert_eq!(env.lines().count(), 1);
        let parts: Vec<&str> = env.trim_end().split('\t').collect();
        assert_eq!(
            parts.len(),
            3,
            "TSV envelope must have id\\tscore\\tcontent"
        );
    }
}
