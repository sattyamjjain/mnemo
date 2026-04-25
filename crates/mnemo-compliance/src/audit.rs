//! EU AI Act audit-log export.
//!
//! The AI Office has outlined two consumption shapes for GPAI providers:
//!
//! * A machine-verifiable NDJSON trail where each line is an `AgentEvent`
//!   plus a detached Ed25519 signature chaining to the previous line.
//! * A columnar CSV mirroring the AI Office's internal template.
//!
//! This module exposes `export_audit_log(since, until, format)` and returns
//! bytes the caller can stream to disk, object storage, or the Office's
//! upload endpoint.

use std::collections::HashMap;

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use mnemo_core::model::event::AgentEvent;
use sha2::{Digest, Sha256};

use crate::error::ComplianceError;

/// Supported audit-log export formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditFormat {
    /// NDJSON with a detached signature chain per line.
    NdjsonSigned,
    /// AI Office GPAI template CSV.
    EuAiOfficeCsv,
}

/// Output of an audit export.
#[derive(Debug, Clone)]
pub struct AuditBundle {
    pub format: AuditFormat,
    /// Exported bytes. NDJSON uses UTF-8 newline-delimited JSON; CSV uses
    /// RFC4180 encoding with a leading header row.
    pub bytes: Vec<u8>,
    /// Public key for the signer, hex-encoded. Only populated for
    /// `NdjsonSigned`.
    pub verifying_key_hex: Option<String>,
    /// Number of events exported.
    pub event_count: usize,
}

/// Ed25519 signer used to produce chain signatures. Keys are supplied by
/// the caller so operators can back them with HSMs or KMS.
pub struct AuditSigner {
    signing_key: SigningKey,
}

impl AuditSigner {
    /// Build a signer from a 32-byte private key.
    pub fn from_secret_bytes(bytes: &[u8; 32]) -> Self {
        Self {
            signing_key: SigningKey::from_bytes(bytes),
        }
    }

    /// Generate a fresh ephemeral signer. Use only for tests — operators
    /// should manage their own long-lived key material.
    pub fn generate_ephemeral() -> Self {
        use rand::Rng;
        let mut bytes = [0u8; 32];
        rand::rng().fill_bytes(&mut bytes);
        Self::from_secret_bytes(&bytes)
    }

    pub fn verifying_key_hex(&self) -> String {
        hex::encode(self.signing_key.verifying_key().to_bytes())
    }
}

// The NDJSON line shape is { "i": <usize>, "e": <AgentEvent-as-Value>,
// "prev": <hex>, "sig": <hex> }. We build and emit it from `build_ndjson_signed`
// via `serde_json::json!` so the signer and verifier agree on a single
// canonicalization path (AgentEvent -> Value -> String -> SHA-256 digest).

/// Export `events` in the requested format. Events must already be in
/// chronological order; callers should fetch via
/// `storage.list_events(...)`, reverse, and slice by timestamp before
/// handing them in here.
pub fn export_audit_log(
    events: &[AgentEvent],
    format: AuditFormat,
    signer: Option<&AuditSigner>,
) -> Result<AuditBundle, ComplianceError> {
    if events.is_empty() {
        return Err(ComplianceError::EmptyAuditWindow);
    }
    match format {
        AuditFormat::NdjsonSigned => {
            let signer = signer.ok_or_else(|| {
                ComplianceError::Signature(
                    "NdjsonSigned export requires an AuditSigner".to_string(),
                )
            })?;
            let bytes = build_ndjson_signed(events, signer)?;
            Ok(AuditBundle {
                format,
                bytes,
                verifying_key_hex: Some(signer.verifying_key_hex()),
                event_count: events.len(),
            })
        }
        AuditFormat::EuAiOfficeCsv => {
            let bytes = build_ai_office_csv(events);
            Ok(AuditBundle {
                format,
                bytes,
                verifying_key_hex: None,
                event_count: events.len(),
            })
        }
    }
}

fn build_ndjson_signed(
    events: &[AgentEvent],
    signer: &AuditSigner,
) -> Result<Vec<u8>, ComplianceError> {
    let mut out = Vec::new();
    let mut prev_hash_hex = "0".repeat(64);
    for (i, event) in events.iter().enumerate() {
        // Canonicalize through `serde_json::Value` so the signer and
        // verifier hash identical byte strings regardless of how serde
        // orders struct fields — the verifier only ever sees the Value.
        let event_value = serde_json::to_value(event)?;
        let event_json = serde_json::to_string(&event_value)?;
        let mut hasher = Sha256::new();
        hasher.update(i.to_string().as_bytes());
        hasher.update(prev_hash_hex.as_bytes());
        hasher.update(event_json.as_bytes());
        let digest = hasher.finalize();
        let signature: Signature = signer.signing_key.sign(&digest);
        let line = serde_json::json!({
            "i": i,
            "e": event_value,
            "prev": prev_hash_hex,
            "sig": hex::encode(signature.to_bytes()),
        });
        let serialized = serde_json::to_string(&line)?;
        out.extend_from_slice(serialized.as_bytes());
        out.push(b'\n');
        prev_hash_hex = hex::encode(digest);
    }
    Ok(out)
}

fn build_ai_office_csv(events: &[AgentEvent]) -> Vec<u8> {
    let mut out = String::new();
    out.push_str("event_id,timestamp,agent_id,event_type,model,thread_id,tokens_input,tokens_output,content_hash\n");
    for e in events {
        out.push_str(&csv_escape(&e.id.to_string()));
        out.push(',');
        out.push_str(&csv_escape(&e.timestamp));
        out.push(',');
        out.push_str(&csv_escape(&e.agent_id));
        out.push(',');
        out.push_str(&csv_escape(&e.event_type.to_string()));
        out.push(',');
        out.push_str(&csv_escape(e.model.as_deref().unwrap_or("")));
        out.push(',');
        out.push_str(&csv_escape(e.thread_id.as_deref().unwrap_or("")));
        out.push(',');
        out.push_str(&e.tokens_input.map(|v| v.to_string()).unwrap_or_default());
        out.push(',');
        out.push_str(&e.tokens_output.map(|v| v.to_string()).unwrap_or_default());
        out.push(',');
        out.push_str(&csv_escape(&hex::encode(&e.content_hash)));
        out.push('\n');
    }
    out.into_bytes()
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        let escaped = s.replace('"', "\"\"");
        format!("\"{escaped}\"")
    } else {
        s.to_string()
    }
}

/// Verify the detached signature chain produced by a `NdjsonSigned` export.
///
/// Returns the number of lines that verified. Any broken chain entry
/// produces [`ComplianceError::ChainBroken`] with the offending index.
pub fn verify_ndjson_signed(
    bytes: &[u8],
    verifying_key_hex: &str,
) -> Result<usize, ComplianceError> {
    let key_bytes = hex::decode(verifying_key_hex)
        .map_err(|e| ComplianceError::Signature(format!("bad hex key: {e}")))?;
    let key_array: [u8; 32] = key_bytes
        .as_slice()
        .try_into()
        .map_err(|_| ComplianceError::Signature("key must be 32 bytes".into()))?;
    let verifying_key = VerifyingKey::from_bytes(&key_array)
        .map_err(|e| ComplianceError::Signature(format!("bad key: {e}")))?;

    let mut prev_hash_hex = "0".repeat(64);
    let mut verified = 0usize;
    // We deserialize to an intermediate HashMap to avoid the 'de-lifetime
    // contortions of serde borrowing a `&'a AgentEvent` off the wire.
    for (idx, line) in bytes.split(|b| *b == b'\n').enumerate() {
        if line.is_empty() {
            continue;
        }
        let obj: HashMap<String, serde_json::Value> = serde_json::from_slice(line)?;
        let index =
            obj.get("i")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| ComplianceError::ChainBroken {
                    index: idx,
                    reason: "missing index".into(),
                })? as usize;
        let prev = obj.get("prev").and_then(|v| v.as_str()).ok_or_else(|| {
            ComplianceError::ChainBroken {
                index,
                reason: "missing prev".into(),
            }
        })?;
        if prev != prev_hash_hex {
            return Err(ComplianceError::ChainBroken {
                index,
                reason: "prev hash mismatch".into(),
            });
        }
        let event_val = obj.get("e").ok_or_else(|| ComplianceError::ChainBroken {
            index,
            reason: "missing event".into(),
        })?;
        let event_json = serde_json::to_string(event_val)?;
        let mut hasher = Sha256::new();
        hasher.update(index.to_string().as_bytes());
        hasher.update(prev.as_bytes());
        hasher.update(event_json.as_bytes());
        let digest = hasher.finalize();
        let sig_hex = obj.get("sig").and_then(|v| v.as_str()).ok_or_else(|| {
            ComplianceError::ChainBroken {
                index,
                reason: "missing signature".into(),
            }
        })?;
        let sig_bytes = hex::decode(sig_hex).map_err(|e| ComplianceError::ChainBroken {
            index,
            reason: format!("bad sig hex: {e}"),
        })?;
        let sig_array: [u8; 64] =
            sig_bytes
                .as_slice()
                .try_into()
                .map_err(|_| ComplianceError::ChainBroken {
                    index,
                    reason: "signature must be 64 bytes".into(),
                })?;
        let sig = Signature::from_bytes(&sig_array);
        verifying_key
            .verify(&digest, &sig)
            .map_err(|e| ComplianceError::ChainBroken {
                index,
                reason: format!("signature verification failed: {e}"),
            })?;
        prev_hash_hex = hex::encode(digest);
        verified += 1;
    }
    Ok(verified)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mnemo_core::model::event::EventType;

    fn sample_event(agent: &str, idx: u64) -> AgentEvent {
        AgentEvent {
            id: uuid::Uuid::now_v7(),
            agent_id: agent.to_string(),
            thread_id: None,
            run_id: None,
            parent_event_id: None,
            event_type: EventType::MemoryWrite,
            payload: serde_json::json!({"seq": idx}),
            trace_id: None,
            span_id: None,
            model: Some("claude-opus-4-7".into()),
            tokens_input: Some(100),
            tokens_output: Some(200),
            latency_ms: Some(42),
            cost_usd: Some(0.01),
            timestamp: format!("2026-04-20T00:00:{:02}Z", idx),
            logical_clock: idx as i64,
            content_hash: vec![idx as u8; 32],
            prev_hash: None,
            embedding: None,
        }
    }

    #[test]
    fn ndjson_signed_round_trips() {
        let signer = AuditSigner::generate_ephemeral();
        let events: Vec<_> = (0..5).map(|i| sample_event("agent-a", i)).collect();
        let bundle = export_audit_log(&events, AuditFormat::NdjsonSigned, Some(&signer)).unwrap();
        assert_eq!(bundle.event_count, 5);
        let verified =
            verify_ndjson_signed(&bundle.bytes, bundle.verifying_key_hex.as_ref().unwrap())
                .unwrap();
        assert_eq!(verified, 5);
    }

    #[test]
    fn tampered_ndjson_fails_verification() {
        let signer = AuditSigner::generate_ephemeral();
        let events: Vec<_> = (0..3).map(|i| sample_event("agent-a", i)).collect();
        let bundle = export_audit_log(&events, AuditFormat::NdjsonSigned, Some(&signer)).unwrap();
        // Replace the first `0` after the second line with a `9` — the
        // timestamp `2026-04-20T00:00:00Z` reliably contains `0` digits.
        let mut tampered = bundle.bytes.clone();
        let first_nl = tampered.iter().position(|b| *b == b'\n').unwrap();
        let zero_pos = tampered[first_nl + 1..]
            .iter()
            .position(|b| *b == b'0')
            .map(|p| first_nl + 1 + p)
            .expect("ndjson must contain a '0' after the first newline");
        tampered[zero_pos] = b'9';
        let err = verify_ndjson_signed(&tampered, bundle.verifying_key_hex.as_ref().unwrap())
            .unwrap_err();
        assert!(
            matches!(err, ComplianceError::ChainBroken { .. }),
            "expected ChainBroken, got {err:?}"
        );
    }

    #[test]
    fn csv_export_has_expected_header() {
        let events = vec![sample_event("agent-a", 0)];
        let bundle = export_audit_log(&events, AuditFormat::EuAiOfficeCsv, None).unwrap();
        let text = String::from_utf8(bundle.bytes).unwrap();
        assert!(text.starts_with("event_id,timestamp,agent_id,"));
        assert!(text.contains("claude-opus-4-7"));
    }

    #[test]
    fn empty_window_errors() {
        let events: Vec<AgentEvent> = vec![];
        let err = export_audit_log(&events, AuditFormat::EuAiOfficeCsv, None).unwrap_err();
        assert!(matches!(err, ComplianceError::EmptyAuditWindow));
    }
}
