//! Counterparty discovery (v0.4.1 P1-5).
//!
//! Anthropic's Project Deal (2026-04-25) opened up agent-on-agent
//! commerce but published no built-in discovery surface. Mnemo
//! ships the `/.well-known/mnemo-deal-agent.json` advertisement
//! shape: each agent puts a small JSON document at a stable URL
//! that says "I exist, here's my capabilities, here's my Ed25519
//! public key, here's where my deal-ledger anchor is".

use serde::{Deserialize, Serialize};

/// Deal capability vocabulary. Open-ended; new verbs land as
/// constants when the Project Deal catalog grows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DealCapability {
    DataLookup,
    DataTransform,
    Compute,
    Verification,
    /// Free-form capability the directory tags but does not
    /// validate. Enables forward-compat with Project Deal's
    /// evolving vocabulary without crate releases.
    Custom(String),
}

impl DealCapability {
    pub fn as_str(&self) -> &str {
        match self {
            DealCapability::DataLookup => "data_lookup",
            DealCapability::DataTransform => "data_transform",
            DealCapability::Compute => "compute",
            DealCapability::Verification => "verification",
            DealCapability::Custom(s) => s.as_str(),
        }
    }
}

/// Public-key bytes (32 bytes for Ed25519). Stored as hex on the
/// wire so a curl + jq inspection is human-readable.
pub type Ed25519PubBytes = [u8; 32];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AgentAdvertisement {
    pub agent: String,
    pub capabilities: Vec<DealCapability>,
    /// Hex-encoded 32-byte Ed25519 public key.
    pub public_key_hex: String,
    pub ledger_anchor_url: String,
    pub terms_template: serde_json::Value,
}

impl AgentAdvertisement {
    pub fn new(
        agent: impl Into<String>,
        capabilities: Vec<DealCapability>,
        public_key: &Ed25519PubBytes,
        ledger_anchor_url: impl Into<String>,
    ) -> Self {
        Self {
            agent: agent.into(),
            capabilities,
            public_key_hex: hex::encode(public_key),
            ledger_anchor_url: ledger_anchor_url.into(),
            terms_template: serde_json::json!({}),
        }
    }

    /// Serialize to the canonical `/.well-known/mnemo-deal-agent.json` body.
    pub fn to_well_known(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Parse a `/.well-known/mnemo-deal-agent.json` body.
    pub fn from_well_known(body: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advertisement_round_trips_through_json() {
        let pk = [7u8; 32];
        let ad = AgentAdvertisement::new(
            "agent-runner-42",
            vec![DealCapability::DataLookup, DealCapability::Compute],
            &pk,
            "https://agent-42.example.com/deal-ledger",
        );
        let body = ad.to_well_known().unwrap();
        let restored = AgentAdvertisement::from_well_known(&body).unwrap();
        assert_eq!(restored, ad);
    }

    #[test]
    fn capability_strings_round_trip() {
        assert_eq!(DealCapability::DataLookup.as_str(), "data_lookup");
        let custom = DealCapability::Custom("research".into());
        assert_eq!(custom.as_str(), "research");
    }

    #[test]
    fn body_includes_required_fields() {
        let ad = AgentAdvertisement::new(
            "x",
            vec![DealCapability::DataLookup],
            &[1u8; 32],
            "https://x/y",
        );
        let body = ad.to_well_known().unwrap();
        for required in [
            "agent",
            "capabilities",
            "public_key_hex",
            "ledger_anchor_url",
        ] {
            assert!(
                body.contains(required),
                "missing field {required} in: {body}"
            );
        }
    }
}
