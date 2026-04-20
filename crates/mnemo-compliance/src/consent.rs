//! DPDPA consent-manager adapter surface.
//!
//! Indian data fiduciaries operating under the Digital Personal Data
//! Protection Act (enforceable 2026-11-13) must consult a DPB-registered
//! Consent Manager before processing personal data. `ConsentSource` models
//! the query shape; `HttpConsentManager` is a generic HTTP binding users
//! can point at whichever CM their DPO has approved.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::ComplianceError;

/// A single processing purpose the subject has (or has not) granted to the
/// operator. The vocabulary is intentionally open-ended; operators should
/// mirror whatever scope taxonomy their consent manager publishes.
pub type Scope = String;

/// The consent snapshot returned by a [`ConsentSource`] for a given subject.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConsentState {
    /// Subject the consent applies to.
    pub subject_id: String,
    /// Scopes the subject has granted. Missing scopes are treated as denied.
    pub scopes: Vec<Scope>,
    /// Optional wall-clock expiry. A `ConsentSource` is free to enforce
    /// earlier expirations; Mnemo rejects any state whose `expires_at` is
    /// in the past.
    pub expires_at: Option<String>,
    /// SHA-256 of the signed consent token the CM issued. Stored in the
    /// audit trail so requests can be traced back to a specific grant.
    pub token_hash: String,
}

impl ConsentState {
    pub fn has_scope(&self, scope: &str) -> bool {
        self.scopes.iter().any(|s| s == scope)
    }

    pub fn is_active(&self) -> bool {
        match self.expires_at.as_deref() {
            None => true,
            Some(ts) => chrono::DateTime::parse_from_rfc3339(ts)
                .map(|t| t.with_timezone(&chrono::Utc) > chrono::Utc::now())
                .unwrap_or(false),
        }
    }
}

/// Pluggable backend for looking up consent for a subject.
#[async_trait]
pub trait ConsentSource: Send + Sync {
    async fn fetch_consent(
        &self,
        subject_id: &str,
    ) -> Result<ConsentState, ComplianceError>;
}

/// Generic HTTP consent-manager binding.
///
/// The remote endpoint is expected to accept a `GET {base_url}/consent/{subject_id}`
/// and return a body matching [`ConsentState`] shape. A bearer token is
/// attached under `Authorization: Bearer <token>` when configured.
pub struct HttpConsentManager {
    base_url: String,
    bearer_token: Option<String>,
    client: reqwest::Client,
}

impl HttpConsentManager {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            bearer_token: None,
            client: reqwest::Client::new(),
        }
    }

    pub fn with_bearer(mut self, token: impl Into<String>) -> Self {
        self.bearer_token = Some(token.into());
        self
    }
}

#[async_trait]
impl ConsentSource for HttpConsentManager {
    async fn fetch_consent(
        &self,
        subject_id: &str,
    ) -> Result<ConsentState, ComplianceError> {
        let url = format!("{}/consent/{}", self.base_url, subject_id);
        let mut req = self.client.get(&url);
        if let Some(ref token) = self.bearer_token {
            req = req.bearer_auth(token);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| ComplianceError::Transport(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(ComplianceError::Transport(format!(
                "consent manager returned {}",
                resp.status()
            )));
        }
        let state: ConsentState = resp
            .json()
            .await
            .map_err(|e| ComplianceError::InvalidConsent(e.to_string()))?;
        if !state.is_active() {
            return Err(ComplianceError::InvalidConsent(
                "consent has expired".to_string(),
            ));
        }
        Ok(state)
    }
}

/// In-memory consent source for tests and single-tenant self-hosting.
pub struct StaticConsentSource {
    entries: std::collections::HashMap<String, ConsentState>,
}

impl StaticConsentSource {
    pub fn new() -> Self {
        Self {
            entries: std::collections::HashMap::new(),
        }
    }

    pub fn grant(&mut self, state: ConsentState) {
        self.entries.insert(state.subject_id.clone(), state);
    }
}

impl Default for StaticConsentSource {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ConsentSource for StaticConsentSource {
    async fn fetch_consent(
        &self,
        subject_id: &str,
    ) -> Result<ConsentState, ComplianceError> {
        self.entries
            .get(subject_id)
            .cloned()
            .filter(|s| s.is_active())
            .ok_or_else(|| ComplianceError::ConsentDenied {
                subject_id: subject_id.to_string(),
                scope: "*".to_string(),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn static_source_returns_active_grant() {
        let mut src = StaticConsentSource::new();
        src.grant(ConsentState {
            subject_id: "user-42".into(),
            scopes: vec!["remember".into(), "recall".into()],
            expires_at: None,
            token_hash: "abc".into(),
        });
        let state = src.fetch_consent("user-42").await.unwrap();
        assert!(state.has_scope("remember"));
        assert!(!state.has_scope("export"));
    }

    #[tokio::test]
    async fn static_source_denies_unknown_subject() {
        let src = StaticConsentSource::new();
        let err = src.fetch_consent("user-unknown").await.unwrap_err();
        assert!(matches!(err, ComplianceError::ConsentDenied { .. }));
    }

    #[test]
    fn expired_consent_is_inactive() {
        let state = ConsentState {
            subject_id: "u".into(),
            scopes: vec![],
            expires_at: Some("2020-01-01T00:00:00Z".into()),
            token_hash: "h".into(),
        };
        assert!(!state.is_active());
    }
}
