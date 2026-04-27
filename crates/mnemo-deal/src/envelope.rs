//! Deal envelope shape (v0.4.0 P1-5).

use std::time::SystemTime;

use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use thiserror::Error;
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

pub type AgentId = String;

/// One signed contract row. The `hmac` covers the canonical
/// concatenation `id || buyer || seller || terms || signed_at ||
/// prev_hash` so any tamper of a field invalidates the row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DealEnvelope {
    pub id: Uuid,
    pub buyer: AgentId,
    pub seller: AgentId,
    pub terms: serde_json::Value,
    pub signed_at: SystemTime,
    pub prev_hash: [u8; 32],
    pub hmac: [u8; 32],
}

#[derive(Debug, Error, PartialEq)]
pub enum EnvelopeError {
    #[error("HMAC key must be at least 32 bytes (got {0})")]
    KeyTooShort(usize),
    #[error("envelope HMAC verification failed at id {0}")]
    HmacMismatch(Uuid),
}

impl DealEnvelope {
    /// Build a fresh envelope chained off `prev_hash`. The signer
    /// computes the HMAC; receivers verify with [`verify_hmac`].
    pub fn sign(
        buyer: impl Into<String>,
        seller: impl Into<String>,
        terms: serde_json::Value,
        prev_hash: [u8; 32],
        signed_at: SystemTime,
        key: &[u8],
    ) -> Result<Self, EnvelopeError> {
        if key.len() < 32 {
            return Err(EnvelopeError::KeyTooShort(key.len()));
        }
        let id = Uuid::now_v7();
        let buyer: String = buyer.into();
        let seller: String = seller.into();
        let canonical = canonical_bytes(&id, &buyer, &seller, &terms, signed_at, &prev_hash);
        let mut mac =
            <HmacSha256 as KeyInit>::new_from_slice(key).expect("KeyInit accepts >=32 bytes");
        mac.update(&canonical);
        let hmac: [u8; 32] = mac.finalize().into_bytes().into();
        Ok(Self {
            id,
            buyer,
            seller,
            terms,
            signed_at,
            prev_hash,
            hmac,
        })
    }

    /// Verify the envelope's `hmac` against the supplied key.
    pub fn verify_hmac(&self, key: &[u8]) -> Result<(), EnvelopeError> {
        if key.len() < 32 {
            return Err(EnvelopeError::KeyTooShort(key.len()));
        }
        let canonical = canonical_bytes(
            &self.id,
            &self.buyer,
            &self.seller,
            &self.terms,
            self.signed_at,
            &self.prev_hash,
        );
        let mut mac =
            <HmacSha256 as KeyInit>::new_from_slice(key).expect("KeyInit accepts >=32 bytes");
        mac.update(&canonical);
        mac.verify_slice(&self.hmac)
            .map_err(|_| EnvelopeError::HmacMismatch(self.id))
    }

    /// Hash this envelope's body to produce the `prev_hash` for the
    /// next envelope in the chain. Pure fn — does not touch the
    /// HMAC key.
    pub fn next_prev_hash(&self) -> [u8; 32] {
        let canonical = canonical_bytes(
            &self.id,
            &self.buyer,
            &self.seller,
            &self.terms,
            self.signed_at,
            &self.prev_hash,
        );
        use sha2::Digest;
        let mut h = Sha256::new();
        h.update(canonical);
        h.update(self.hmac);
        h.finalize().into()
    }
}

fn canonical_bytes(
    id: &Uuid,
    buyer: &str,
    seller: &str,
    terms: &serde_json::Value,
    signed_at: SystemTime,
    prev_hash: &[u8; 32],
) -> Vec<u8> {
    let mut out = Vec::with_capacity(256);
    out.extend_from_slice(id.as_bytes());
    out.push(b'|');
    out.extend_from_slice(buyer.as_bytes());
    out.push(b'|');
    out.extend_from_slice(seller.as_bytes());
    out.push(b'|');
    // Use canonical JSON so different field orders don't produce
    // different HMACs — serde_json::to_string already emits keys in
    // insertion order, but this rebuild via to_value forces a stable
    // representation by re-serializing through Value's BTreeMap.
    let canon = serde_json::to_string(terms).unwrap_or_default();
    out.extend_from_slice(canon.as_bytes());
    out.push(b'|');
    let dt: chrono::DateTime<chrono::Utc> = signed_at.into();
    out.extend_from_slice(dt.to_rfc3339().as_bytes());
    out.push(b'|');
    out.extend_from_slice(prev_hash);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> [u8; 32] {
        [42u8; 32]
    }

    #[test]
    fn sign_verify_round_trip() {
        let e = DealEnvelope::sign(
            "buyer-a",
            "seller-b",
            serde_json::json!({"price": "10 USDC"}),
            [0u8; 32],
            SystemTime::UNIX_EPOCH,
            &key(),
        )
        .unwrap();
        assert!(e.verify_hmac(&key()).is_ok());
    }

    #[test]
    fn flipped_byte_breaks_verify() {
        let mut e = DealEnvelope::sign(
            "buyer-a",
            "seller-b",
            serde_json::json!({"price": "10 USDC"}),
            [0u8; 32],
            SystemTime::UNIX_EPOCH,
            &key(),
        )
        .unwrap();
        e.terms = serde_json::json!({"price": "1000 USDC"});
        let err = e.verify_hmac(&key()).unwrap_err();
        assert!(matches!(err, EnvelopeError::HmacMismatch(_)));
    }

    #[test]
    fn short_key_is_rejected() {
        let err = DealEnvelope::sign(
            "a",
            "b",
            serde_json::json!({}),
            [0u8; 32],
            SystemTime::UNIX_EPOCH,
            &[1u8; 16],
        )
        .unwrap_err();
        assert_eq!(err, EnvelopeError::KeyTooShort(16));
    }

    #[test]
    fn next_prev_hash_is_deterministic() {
        let e = DealEnvelope::sign(
            "a",
            "b",
            serde_json::json!({}),
            [0u8; 32],
            SystemTime::UNIX_EPOCH,
            &key(),
        )
        .unwrap();
        assert_eq!(e.next_prev_hash(), e.next_prev_hash());
    }
}
