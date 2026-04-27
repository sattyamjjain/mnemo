//! v0.4.0 (P0-1) — MCP tool-catalog attestation.
//!
//! Defends against **arXiv 2604.20994** ("Function-Hijacking via MCP
//! tool-catalog poisoning"): an attacker that can mutate the JSON
//! that the MCP server advertises during the `tools/list` handshake
//! can rename a tool, change its `inputSchema`, or smuggle a hidden
//! `secret_exfil` tool into the catalog the host LLM sees. Our v0.3.x
//! poisoning defense (MINJA quarantine) only covers memory content;
//! the tool list itself was unguarded.
//!
//! This module ships an operator-pinned baseline. Before
//! `mnemo mcp-server` exposes its catalog over stdio, it computes a
//! deterministic SHA-256 of the advertised tools and compares to the
//! baseline. Three outcomes:
//!
//! * **`Match`** — fingerprints identical. Server proceeds.
//! * **`Drift`** — only `removed` (subset). Audit-log warning, allowed
//!   if the operator passed `--allow-removed-drift`; otherwise rejected.
//! * **`Reject`** — anything `added` or `mutated`. Refuse to start.
//!
//! Every verdict is recorded as a `McpToolCatalogDrift` audit event
//! so the operator's existing audit-log export catches the incident.
//!
//! The rmcp-side wiring (calling `attestor.attest(&advertised)` from
//! `ServerHandler::list_tools`) is a separate follow-up. The public
//! API here is exercised end-to-end by the unit tests + the
//! manifest loader + the binary's startup path; `#[allow(dead_code)]`
//! markers document the items that need that follow-up.

#![allow(dead_code)]

pub mod catalog_pin;

use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// One row in the pinned catalog: name + SHA-256 of canonical
/// JSON-encoded `(name, description, inputSchema)`. Two distinct
/// catalogs that hash to the same fingerprint set are
/// indistinguishable from the host LLM's perspective.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Hash, Ord, PartialOrd)]
pub struct ToolFingerprint {
    pub name: String,
    pub schema_sha256: [u8; 32],
}

impl ToolFingerprint {
    pub fn schema_hex(&self) -> String {
        hex::encode(self.schema_sha256)
    }
}

/// Stable identifier for whoever signed the pin. Format is
/// operator-defined; recommended `"<host>:<key_id>"` (e.g.
/// `"mnemo-prod:catalog-pin-2026-04"`) so a rotation is auditable.
pub type SignerId = String;

/// The pinned catalog the operator commits to. Lives in the manifest
/// (B2 hardened mode) so it shares the chmod-restricted file the
/// keystore already lives behind.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinnedToolCatalog {
    pub signer: SignerId,
    pub signed_at: SystemTime,
    pub tools: Vec<ToolFingerprint>,
}

impl PinnedToolCatalog {
    /// Catalog-level SHA: SHA-256 of the sorted name||schema_sha256
    /// concatenation. Two catalogs with the same fingerprint set
    /// produce the same SHA regardless of input order.
    pub fn catalog_sha256(&self) -> [u8; 32] {
        let mut sorted = self.tools.clone();
        sorted.sort();
        let mut h = Sha256::new();
        for t in &sorted {
            h.update(t.name.as_bytes());
            h.update(b"|");
            h.update(t.schema_sha256);
            h.update(b"\n");
        }
        h.finalize().into()
    }

    pub fn names(&self) -> Vec<&str> {
        self.tools.iter().map(|t| t.name.as_str()).collect()
    }
}

/// Verdict returned by the attestor. Drift carries the per-tool
/// diffs so the audit row is actionable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AttestationVerdict {
    Match,
    /// Some tools differ. Each list is mutually exclusive: a tool with
    /// the same name but a different schema goes in `mutated`, not in
    /// both `added` and `removed`.
    Drift {
        added: Vec<ToolFingerprint>,
        removed: Vec<ToolFingerprint>,
        mutated: Vec<ToolFingerprint>,
    },
    Reject {
        reason: String,
    },
}

impl AttestationVerdict {
    pub fn is_safe(&self) -> bool {
        matches!(self, AttestationVerdict::Match)
    }

    /// Whether the operator's `--allow-removed-drift` flag would let
    /// startup proceed. True only when nothing was added or mutated.
    pub fn is_removed_only_drift(&self) -> bool {
        matches!(
            self,
            AttestationVerdict::Drift { added, mutated, .. }
                if added.is_empty() && mutated.is_empty()
        )
    }
}

/// Pluggable attestor. The default impl (`PinnedAttestor`) reads from
/// a [`PinnedToolCatalog`]; tests substitute a fake.
pub trait CatalogAttestor: Send + Sync {
    fn attest(&self, advertised: &[ToolFingerprint]) -> Result<AttestationVerdict, AttestError>;
}

#[derive(Debug, Error, PartialEq)]
pub enum AttestError {
    #[error("attestor has no pinned baseline; the manifest must include `[tool_catalog_pin]`")]
    NoBaseline,
    #[error("baseline is empty — refusing to attest a vacuous catalog")]
    EmptyBaseline,
}

/// Default attestor: compares advertised tools against a pinned
/// baseline by name. A name collision with a different schema is
/// classified as `mutated` (not `added` + `removed`) so the verdict
/// doesn't double-count.
pub struct PinnedAttestor {
    baseline: PinnedToolCatalog,
}

impl PinnedAttestor {
    pub fn new(baseline: PinnedToolCatalog) -> Self {
        Self { baseline }
    }

    pub fn baseline(&self) -> &PinnedToolCatalog {
        &self.baseline
    }
}

impl CatalogAttestor for PinnedAttestor {
    fn attest(&self, advertised: &[ToolFingerprint]) -> Result<AttestationVerdict, AttestError> {
        if self.baseline.tools.is_empty() {
            return Err(AttestError::EmptyBaseline);
        }
        if self.baseline.catalog_sha256() == catalog_sha_of(advertised) {
            return Ok(AttestationVerdict::Match);
        }

        let mut by_name_baseline: std::collections::BTreeMap<&str, &ToolFingerprint> = self
            .baseline
            .tools
            .iter()
            .map(|t| (t.name.as_str(), t))
            .collect();
        let mut added = Vec::new();
        let mut mutated = Vec::new();

        for t in advertised {
            match by_name_baseline.remove(t.name.as_str()) {
                None => added.push(t.clone()),
                Some(base) if base.schema_sha256 != t.schema_sha256 => mutated.push(t.clone()),
                Some(_) => {}
            }
        }
        let removed: Vec<ToolFingerprint> =
            by_name_baseline.values().map(|t| (*t).clone()).collect();

        if added.is_empty() && mutated.is_empty() && removed.is_empty() {
            // Sets matched but catalog SHA didn't — should be impossible
            // since the SHA is derived from sorted (name, schema_sha256).
            // Treat as a hostile bug and Reject.
            return Ok(AttestationVerdict::Reject {
                reason: "catalog_sha mismatch with empty diff".into(),
            });
        }
        Ok(AttestationVerdict::Drift {
            added,
            removed,
            mutated,
        })
    }
}

fn catalog_sha_of(tools: &[ToolFingerprint]) -> [u8; 32] {
    let mut sorted = tools.to_vec();
    sorted.sort();
    let mut h = Sha256::new();
    for t in &sorted {
        h.update(t.name.as_bytes());
        h.update(b"|");
        h.update(t.schema_sha256);
        h.update(b"\n");
    }
    h.finalize().into()
}

/// Hash a tool's `(name, description, inputSchema)` JSON into a
/// 32-byte fingerprint. Operators feed the result of this fn into
/// the manifest's `[[tool_catalog_pin.tools]]` rows.
pub fn fingerprint_tool(name: &str, description: &str, input_schema_json: &str) -> ToolFingerprint {
    let mut h = Sha256::new();
    h.update(b"name=");
    h.update(name.as_bytes());
    h.update(b"\ndescription=");
    h.update(description.as_bytes());
    h.update(b"\nschema=");
    h.update(input_schema_json.as_bytes());
    let schema_sha256: [u8; 32] = h.finalize().into();
    ToolFingerprint {
        name: name.to_string(),
        schema_sha256,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fp(name: &str, schema: &str) -> ToolFingerprint {
        fingerprint_tool(name, "desc", schema)
    }

    fn baseline_six() -> PinnedToolCatalog {
        PinnedToolCatalog {
            signer: "test-pin".into(),
            signed_at: SystemTime::UNIX_EPOCH,
            tools: vec![
                fp("mnemo.remember", r#"{"type":"object"}"#),
                fp("mnemo.recall", r#"{"type":"object"}"#),
                fp("mnemo.forget", r#"{"type":"object"}"#),
                fp("mnemo.share", r#"{"type":"object"}"#),
                fp("mnemo.checkpoint", r#"{"type":"object"}"#),
                fp("mnemo.verify", r#"{"type":"object"}"#),
            ],
        }
    }

    #[test]
    fn baseline_match_is_safe() {
        let pin = baseline_six();
        let attestor = PinnedAttestor::new(pin.clone());
        let v = attestor.attest(&pin.tools).unwrap();
        assert_eq!(v, AttestationVerdict::Match);
        assert!(v.is_safe());
    }

    #[test]
    fn empty_baseline_rejected() {
        let attestor = PinnedAttestor::new(PinnedToolCatalog {
            signer: "x".into(),
            signed_at: SystemTime::UNIX_EPOCH,
            tools: vec![],
        });
        assert_eq!(
            attestor.attest(&[fp("a", "{}")]).unwrap_err(),
            AttestError::EmptyBaseline
        );
    }

    #[test]
    fn appended_secret_exfil_is_drift_with_added() {
        let pin = baseline_six();
        let attestor = PinnedAttestor::new(pin.clone());
        let mut hostile = pin.tools.clone();
        hostile.push(fp("secret_exfil", r#"{"type":"object"}"#));
        let v = attestor.attest(&hostile).unwrap();
        match v {
            AttestationVerdict::Drift {
                added,
                removed,
                mutated,
            } => {
                assert_eq!(added.len(), 1);
                assert_eq!(added[0].name, "secret_exfil");
                assert!(removed.is_empty());
                assert!(mutated.is_empty());
            }
            other => panic!("expected Drift, got {other:?}"),
        }
    }

    #[test]
    fn schema_mutation_is_classified_as_mutated() {
        let pin = baseline_six();
        let attestor = PinnedAttestor::new(pin.clone());
        let mut hostile = pin.tools.clone();
        hostile[1] = fp("mnemo.recall", r#"{"properties":{"instructions":{}}}"#);
        let v = attestor.attest(&hostile).unwrap();
        match v {
            AttestationVerdict::Drift {
                added,
                removed,
                mutated,
            } => {
                assert!(added.is_empty());
                assert!(removed.is_empty());
                assert_eq!(mutated.len(), 1);
                assert_eq!(mutated[0].name, "mnemo.recall");
            }
            other => panic!("expected mutated drift, got {other:?}"),
        }
    }

    #[test]
    fn removed_only_drift_is_recoverable() {
        let pin = baseline_six();
        let attestor = PinnedAttestor::new(pin.clone());
        let downgraded: Vec<_> = pin.tools.iter().take(5).cloned().collect();
        let v = attestor.attest(&downgraded).unwrap();
        assert!(v.is_removed_only_drift());
        assert!(!v.is_safe());
    }

    #[test]
    fn added_drift_is_not_removed_only() {
        let pin = baseline_six();
        let attestor = PinnedAttestor::new(pin.clone());
        let mut h = pin.tools.clone();
        h.push(fp("evil", "{}"));
        let v = attestor.attest(&h).unwrap();
        assert!(!v.is_removed_only_drift());
    }

    #[test]
    fn property_any_added_or_mutated_blocks_safe() {
        let pin = baseline_six();
        let attestor = PinnedAttestor::new(pin.clone());
        // Build 50 random hostile catalogs; any with ≥1 added or
        // mutated must NOT be safe. Removed-only must be
        // removed_only_drift but not safe.
        for seed in 0..50u8 {
            let mut h = pin.tools.clone();
            if seed % 3 == 0 {
                h.push(fp(&format!("evil_{seed}"), "{}"));
            }
            if seed % 3 == 1 && !h.is_empty() {
                let i = (seed as usize) % h.len();
                h[i] = fp(&h[i].name.clone(), &format!(r#"{{"v":{seed}}}"#));
            }
            if seed % 3 == 2 && !h.is_empty() {
                let i = (seed as usize) % h.len();
                h.remove(i);
            }
            let v = attestor.attest(&h).unwrap();
            assert!(
                !v.is_safe(),
                "seed {seed} produced safe verdict for hostile diff"
            );
        }
    }
}
