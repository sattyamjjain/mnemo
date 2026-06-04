//! AMP / memorywire wire format.
//!
//! AMP ("Agent Memory Protocol" / *memorywire*) models an agent's
//! memory surface as **5 operations** — `remember` / `recall` /
//! `forget` / `merge` / `expire` — over **4 memory types** —
//! `episodic` / `semantic` / `procedural` / `working`. The wire shape
//! is a single self-describing JSON envelope validated against a
//! JSON-Schema 2020-12 document (see [`schema`]).
//!
//! This module is transport-agnostic: it only defines the request
//! ([`AmpEnvelope`]) and response ([`AmpResult`]) shapes plus the
//! schema. The mapping onto a real [`MnemoEngine`](mnemo_core::query::MnemoEngine)
//! lives in [`crate::store`].

use serde::{Deserialize, Serialize};

/// The five AMP operations. 1:1 with the cross-adapter conformance
/// suite's op axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AmpOp {
    Remember,
    Recall,
    Forget,
    Merge,
    Expire,
}

impl AmpOp {
    pub fn as_str(self) -> &'static str {
        match self {
            AmpOp::Remember => "remember",
            AmpOp::Recall => "recall",
            AmpOp::Forget => "forget",
            AmpOp::Merge => "merge",
            AmpOp::Expire => "expire",
        }
    }

    /// All five ops, in canonical order.
    pub const ALL: [AmpOp; 5] = [
        AmpOp::Remember,
        AmpOp::Recall,
        AmpOp::Forget,
        AmpOp::Merge,
        AmpOp::Expire,
    ];
}

/// The four AMP memory types. Map 1:1 onto
/// [`mnemo_core::model::memory::MemoryType`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AmpMemoryType {
    Episodic,
    Semantic,
    Procedural,
    Working,
}

impl AmpMemoryType {
    pub fn as_str(self) -> &'static str {
        match self {
            AmpMemoryType::Episodic => "episodic",
            AmpMemoryType::Semantic => "semantic",
            AmpMemoryType::Procedural => "procedural",
            AmpMemoryType::Working => "working",
        }
    }

    /// `true` for the long-term tiers (`semantic` / `procedural`) that
    /// the HITL diff-and-approve hook gates by default. Episodic and
    /// working memories are short-lived and bypass approval.
    pub fn is_long_term(self) -> bool {
        matches!(self, AmpMemoryType::Semantic | AmpMemoryType::Procedural)
    }

    /// All four memory types, in canonical order.
    pub const ALL: [AmpMemoryType; 4] = [
        AmpMemoryType::Episodic,
        AmpMemoryType::Semantic,
        AmpMemoryType::Procedural,
        AmpMemoryType::Working,
    ];
}

/// A single AMP request envelope.
///
/// Every field except `op` and `memory_type` is optional; which
/// fields are *meaningful* depends on `op` (e.g. `query` for
/// `recall`, `memory_ids` for `forget` / `merge` / `expire`,
/// `content` for `remember`). [`crate::store`] enforces the
/// per-op requirements and returns a typed error on a malformed
/// envelope rather than panicking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AmpEnvelope {
    /// AMP protocol version. Currently `"amp/1"`.
    #[serde(default = "default_amp_version")]
    pub amp_version: String,
    pub op: AmpOp,
    pub memory_type: AmpMemoryType,
    /// Agent scope. Falls back to the engine default when omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    /// `remember` payload.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// `recall` query string.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    /// Target memory ids for `forget` / `merge` / `expire`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub memory_ids: Vec<String>,
    /// `recall` top-k. Defaults to 5 (the conformance suite's
    /// recall@5).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_k: Option<usize>,
    /// Free-form tags attached on `remember` / used to scope `recall`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// `expire`: seconds from now after which the memory expires. When
    /// omitted (or `0`), `expire` takes effect immediately.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttl_seconds: Option<u64>,
    /// Optional opaque metadata round-tripped onto the stored record.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

fn default_amp_version() -> String {
    "amp/1".to_string()
}

impl AmpEnvelope {
    /// Construct a minimal envelope for `op` over `memory_type`.
    pub fn new(op: AmpOp, memory_type: AmpMemoryType) -> Self {
        Self {
            amp_version: default_amp_version(),
            op,
            memory_type,
            agent_id: None,
            content: None,
            query: None,
            memory_ids: Vec::new(),
            top_k: None,
            tags: Vec::new(),
            ttl_seconds: None,
            metadata: None,
        }
    }
}

/// One recalled item in an [`AmpResult`].
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AmpHit {
    pub id: String,
    pub content: String,
    pub memory_type: AmpMemoryType,
    pub score: f32,
    pub tags: Vec<String>,
}

/// The response envelope returned by every AMP op.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AmpResult {
    pub op: AmpOp,
    pub ok: bool,
    /// Ids written / affected (`remember` → \[new id\]; `merge` → \[merged
    /// id\]; `forget` / `expire` → affected ids).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ids: Vec<String>,
    /// `recall` hits, highest score first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hits: Vec<AmpHit>,
    /// `true` when a HITL approval hook gated this write and approved
    /// it; `false`/absent when no approval was required.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approved: Option<bool>,
    /// Human-readable diagnostic; empty on success.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub detail: String,
}

impl AmpResult {
    pub fn ok(op: AmpOp) -> Self {
        Self {
            op,
            ok: true,
            ids: Vec::new(),
            hits: Vec::new(),
            approved: None,
            detail: String::new(),
        }
    }

    pub fn rejected(op: AmpOp, detail: impl Into<String>) -> Self {
        Self {
            op,
            ok: false,
            ids: Vec::new(),
            hits: Vec::new(),
            approved: Some(false),
            detail: detail.into(),
        }
    }
}

/// The AMP envelope JSON-Schema 2020-12 document.
///
/// Returned as a `serde_json::Value` so callers can serve it from a
/// `.well-known/amp-schema.json` endpoint, validate inbound envelopes,
/// or diff it against another adapter's schema in the cross-adapter
/// conformance suite. The `enum` lists pin the 5-op × 4-type surface.
pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": "https://mnemo.dev/schemas/amp/1/envelope.json",
        "title": "AMP memory envelope",
        "description": "AMP / memorywire request envelope: 5 operations over 4 memory types.",
        "type": "object",
        "required": ["op", "memory_type"],
        "additionalProperties": false,
        "properties": {
            "amp_version": { "type": "string", "default": "amp/1" },
            "op": {
                "type": "string",
                "enum": ["remember", "recall", "forget", "merge", "expire"]
            },
            "memory_type": {
                "type": "string",
                "enum": ["episodic", "semantic", "procedural", "working"]
            },
            "agent_id": { "type": ["string", "null"] },
            "content": { "type": ["string", "null"] },
            "query": { "type": ["string", "null"] },
            "memory_ids": {
                "type": "array",
                "items": { "type": "string", "format": "uuid" }
            },
            "top_k": { "type": ["integer", "null"], "minimum": 1 },
            "tags": { "type": "array", "items": { "type": "string" } },
            "ttl_seconds": { "type": ["integer", "null"], "minimum": 0 },
            "metadata": { "type": ["object", "null"] }
        },
        "allOf": [
            {
                "if": { "properties": { "op": { "const": "remember" } } },
                "then": { "required": ["content"] }
            },
            {
                "if": { "properties": { "op": { "const": "recall" } } },
                "then": { "required": ["query"] }
            },
            {
                "if": { "properties": { "op": { "const": "forget" } } },
                "then": { "required": ["memory_ids"] }
            },
            {
                "if": { "properties": { "op": { "const": "merge" } } },
                "then": { "required": ["memory_ids"] }
            },
            {
                "if": { "properties": { "op": { "const": "expire" } } },
                "then": { "required": ["memory_ids"] }
            }
        ]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn op_and_type_axes_are_complete() {
        assert_eq!(AmpOp::ALL.len(), 5);
        assert_eq!(AmpMemoryType::ALL.len(), 4);
        // 5 ops × 4 types = the 20-cell AMP surface.
        assert_eq!(AmpOp::ALL.len() * AmpMemoryType::ALL.len(), 20);
    }

    #[test]
    fn envelope_round_trips_through_json() {
        let mut env = AmpEnvelope::new(AmpOp::Remember, AmpMemoryType::Semantic);
        env.content = Some("the capital of France is Paris".into());
        env.tags = vec!["geo".into()];
        let s = serde_json::to_string(&env).unwrap();
        let back: AmpEnvelope = serde_json::from_str(&s).unwrap();
        assert_eq!(back.op, AmpOp::Remember);
        assert_eq!(back.memory_type, AmpMemoryType::Semantic);
        assert_eq!(
            back.content.as_deref(),
            Some("the capital of France is Paris")
        );
        assert_eq!(back.amp_version, "amp/1");
    }

    #[test]
    fn schema_is_2020_12_and_pins_the_surface() {
        let s = schema();
        assert_eq!(s["$schema"], "https://json-schema.org/draft/2020-12/schema");
        let ops = s["properties"]["op"]["enum"].as_array().unwrap();
        assert_eq!(ops.len(), 5);
        let types = s["properties"]["memory_type"]["enum"].as_array().unwrap();
        assert_eq!(types.len(), 4);
    }

    #[test]
    fn long_term_classification() {
        assert!(AmpMemoryType::Semantic.is_long_term());
        assert!(AmpMemoryType::Procedural.is_long_term());
        assert!(!AmpMemoryType::Episodic.is_long_term());
        assert!(!AmpMemoryType::Working.is_long_term());
    }
}
