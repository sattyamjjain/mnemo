//! v0.4.0-rc3 (Task B4) — Mannsetu DPDPA consent-manager adapter +
//! consent-token-per-write guard.
//!
//! Mannsetu is one of the consent managers India's Data Protection
//! Board has signalled it will register first under DPDPA Section 7
//! (the "Consent Manager" rule). The wire shape used here mirrors the
//! draft Mannsetu OpenAPI spec the DPB published for public comment;
//! when the production endpoints harden we update one [`MannsetuConfig`]
//! constant rather than the whole adapter.
//!
//! # What this gives you
//!
//! * [`MannsetuConsentSource`] — drop-in [`ConsentSource`] that calls
//!   the Mannsetu consent-lookup endpoint, returns a [`ConsentState`],
//!   and surfaces the per-token hash so it lands in the audit trail.
//!
//! * [`ConsentTokenGuard`] — a small helper that wraps every write
//!   with an explicit consent-token check. Operators call
//!   `guard.authorize(subject_id, scope, token)` BEFORE
//!   `engine.remember(...)`. A missing/expired/wrong-scope token
//!   becomes a [`ComplianceError::ConsentDenied`].
//!
//! The guard intentionally does NOT call `engine.remember` itself —
//! that keeps the consent layer composable with whatever request
//! shape the operator already uses (REST / gRPC / direct engine).

use std::collections::HashSet;
use std::sync::RwLock;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::consent::{ConsentSource, ConsentState};
use crate::error::ComplianceError;

/// Production Mannsetu base URL — operators override via config when
/// pointing at the DPB sandbox.
pub const MANNSETU_PROD_BASE_URL: &str = "https://api.mannsetu.gov.in/v1";

/// Sandbox base URL the DPB exposes for integration testing.
pub const MANNSETU_SANDBOX_BASE_URL: &str = "https://sandbox.mannsetu.gov.in/v1";

/// Configuration for the Mannsetu consent-manager binding.
#[derive(Debug, Clone)]
pub struct MannsetuConfig {
    /// Base URL — defaults to `MANNSETU_PROD_BASE_URL`.
    pub base_url: String,
    /// Operator's API key issued by the DPB during onboarding. Sent as
    /// `Authorization: Bearer <key>`.
    pub api_key: String,
    /// `data_fiduciary_id` — the operator's DPB-issued identifier.
    pub fiduciary_id: String,
    /// HTTP timeout in seconds. The DPB SLA is 5 s p95; we default
    /// slightly above to absorb transient retries.
    pub timeout_seconds: u64,
}

impl MannsetuConfig {
    pub fn new(api_key: impl Into<String>, fiduciary_id: impl Into<String>) -> Self {
        Self {
            base_url: MANNSETU_PROD_BASE_URL.to_string(),
            api_key: api_key.into(),
            fiduciary_id: fiduciary_id.into(),
            timeout_seconds: 8,
        }
    }

    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    pub fn with_timeout(mut self, seconds: u64) -> Self {
        self.timeout_seconds = seconds;
        self
    }
}

/// Wire shape returned by `GET {base}/consent/lookup?subject_id=...`.
/// Mirrors the Mannsetu OpenAPI v0.3 draft (2026-02). We deliberately
/// over-derive `Deserialize` so the adapter survives forward-compatible
/// fields the DPB may add later — extra keys are ignored.
#[derive(Debug, Clone, Deserialize)]
struct MannsetuConsentResponse {
    subject_id: String,
    granted_scopes: Vec<String>,
    /// RFC3339 timestamp; absent means "no expiry".
    expires_at: Option<String>,
    /// SHA-256 hex of the canonical consent token the CM issued.
    token_sha256: String,
}

/// Mannsetu adapter backed by `reqwest`. Deliberately stateless —
/// every lookup is a fresh HTTP call; the DPB does not promise
/// caching invariants and a stale cache here would constitute a
/// regulatory violation.
pub struct MannsetuConsentSource {
    config: MannsetuConfig,
    client: reqwest::Client,
}

impl MannsetuConsentSource {
    pub fn new(config: MannsetuConfig) -> Result<Self, ComplianceError> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_seconds))
            .build()
            .map_err(|e| ComplianceError::Transport(format!("client build: {e}")))?;
        Ok(Self { config, client })
    }
}

#[async_trait]
impl ConsentSource for MannsetuConsentSource {
    async fn fetch_consent(&self, subject_id: &str) -> Result<ConsentState, ComplianceError> {
        // Inline subject_id into the URL — matches the existing
        // HttpConsentManager pattern in this crate, which deliberately
        // avoids reqwest's optional query-builder feature so the
        // workspace reqwest dep stays minimal.
        let url = format!(
            "{}/consent/lookup?subject_id={}&fiduciary_id={}",
            self.config.base_url,
            urlencoding(subject_id),
            urlencoding(&self.config.fiduciary_id),
        );
        let resp = self
            .client
            .get(&url)
            .bearer_auth(&self.config.api_key)
            .send()
            .await
            .map_err(|e| ComplianceError::Transport(format!("send: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            // 404 is the DPB's signal for "no record of consent for
            // this subject" — surface as a clean denial, not as a
            // transport blip.
            if status == reqwest::StatusCode::NOT_FOUND {
                return Err(ComplianceError::InvalidConsent(format!(
                    "no consent record for subject {subject_id}"
                )));
            }
            return Err(ComplianceError::Transport(format!("{status}: {body}")));
        }

        let body: MannsetuConsentResponse = resp
            .json()
            .await
            .map_err(|e| ComplianceError::InvalidConsent(format!("decode: {e}")))?;

        Ok(ConsentState {
            subject_id: body.subject_id,
            scopes: body.granted_scopes,
            expires_at: body.expires_at,
            token_hash: body.token_sha256,
        })
    }
}

/// A signed consent-token snippet the operator presents on every
/// write. Carries enough state to authorize the write without a
/// network round-trip when the token has not expired.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConsentToken {
    /// SHA-256 hex of the canonical CM-issued token. Lands in the
    /// audit trail so a subject can later trace back which grant
    /// authorized which write.
    pub token_sha256: String,
    pub subject_id: String,
    pub granted_scopes: Vec<String>,
    /// RFC3339 expiry; required for tokens used by the guard so we
    /// never accept eternal tokens by accident.
    pub expires_at: String,
}

impl ConsentToken {
    pub fn is_active_now(&self) -> bool {
        chrono::DateTime::parse_from_rfc3339(&self.expires_at)
            .map(|t| t.with_timezone(&chrono::Utc) > chrono::Utc::now())
            .unwrap_or(false)
    }

    pub fn covers(&self, scope: &str) -> bool {
        self.granted_scopes.iter().any(|s| s == scope)
    }
}

/// Per-write guard: the operator presents a [`ConsentToken`] alongside
/// every `remember` call, and the guard refuses anything missing /
/// expired / wrong-scope BEFORE the engine sees the data.
///
/// Holds an optional set of currently-revoked token hashes — operators
/// can plumb a webhook from Mannsetu's revocation feed to populate it.
pub struct ConsentTokenGuard {
    revoked: RwLock<HashSet<String>>,
}

impl Default for ConsentTokenGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl ConsentTokenGuard {
    pub fn new() -> Self {
        Self {
            revoked: RwLock::new(HashSet::new()),
        }
    }

    /// Mark a token hash as revoked. Future `authorize` calls citing
    /// this hash refuse with [`ComplianceError::ConsentDenied`].
    pub fn revoke(&self, token_sha256: impl Into<String>) {
        self.revoked
            .write()
            .expect("revoked-set poisoned")
            .insert(token_sha256.into());
    }

    pub fn is_revoked(&self, token_sha256: &str) -> bool {
        self.revoked
            .read()
            .expect("revoked-set poisoned")
            .contains(token_sha256)
    }

    /// Approve a write. Returns the same `token_sha256` for the audit
    /// trail when the check passes; refuses with the right denial
    /// reason otherwise.
    pub fn authorize(
        &self,
        subject_id: &str,
        scope: &str,
        token: &ConsentToken,
    ) -> Result<String, ComplianceError> {
        if token.subject_id != subject_id {
            return Err(ComplianceError::ConsentDenied {
                subject_id: subject_id.to_string(),
                scope: format!("{scope} (token bound to {})", token.subject_id),
            });
        }
        if !token.is_active_now() {
            return Err(ComplianceError::InvalidConsent(format!(
                "token expired at {}",
                token.expires_at
            )));
        }
        if !token.covers(scope) {
            return Err(ComplianceError::ConsentDenied {
                subject_id: subject_id.to_string(),
                scope: scope.to_string(),
            });
        }
        if self.is_revoked(&token.token_sha256) {
            return Err(ComplianceError::ConsentDenied {
                subject_id: subject_id.to_string(),
                scope: format!("{scope} (token revoked)"),
            });
        }
        Ok(token.token_sha256.clone())
    }
}

/// Minimal percent-encoder for the few characters that show up in
/// subject IDs and fiduciary IDs (alphanumerics + `-`, `_`, `.`, `~`
/// pass through; everything else is `%XX`-escaped). Avoids pulling
/// the `urlencoding` crate just for two query-param interpolations.
fn urlencoding(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};

    fn fresh_token(scopes: &[&str]) -> ConsentToken {
        ConsentToken {
            token_sha256: "deadbeef".to_string(),
            subject_id: "subj-1".to_string(),
            granted_scopes: scopes.iter().map(|s| s.to_string()).collect(),
            expires_at: (Utc::now() + Duration::hours(1)).to_rfc3339(),
        }
    }

    #[test]
    fn covering_token_passes() {
        let g = ConsentTokenGuard::new();
        let t = fresh_token(&["clinical-notes"]);
        let h = g.authorize("subj-1", "clinical-notes", &t).unwrap();
        assert_eq!(h, "deadbeef");
    }

    #[test]
    fn missing_scope_is_denied() {
        let g = ConsentTokenGuard::new();
        let t = fresh_token(&["clinical-notes"]);
        let err = g.authorize("subj-1", "billing", &t).unwrap_err();
        assert!(matches!(err, ComplianceError::ConsentDenied { .. }));
    }

    #[test]
    fn wrong_subject_is_denied() {
        let g = ConsentTokenGuard::new();
        let t = fresh_token(&["clinical-notes"]);
        let err = g.authorize("subj-2", "clinical-notes", &t).unwrap_err();
        assert!(matches!(err, ComplianceError::ConsentDenied { .. }));
    }

    #[test]
    fn expired_token_is_invalid() {
        let g = ConsentTokenGuard::new();
        let t = ConsentToken {
            token_sha256: "x".into(),
            subject_id: "subj-1".into(),
            granted_scopes: vec!["clinical-notes".into()],
            expires_at: (Utc::now() - Duration::minutes(1)).to_rfc3339(),
        };
        let err = g.authorize("subj-1", "clinical-notes", &t).unwrap_err();
        assert!(matches!(err, ComplianceError::InvalidConsent(_)));
    }

    #[test]
    fn revoked_token_is_denied() {
        let g = ConsentTokenGuard::new();
        let t = fresh_token(&["clinical-notes"]);
        g.revoke("deadbeef");
        let err = g.authorize("subj-1", "clinical-notes", &t).unwrap_err();
        assert!(matches!(err, ComplianceError::ConsentDenied { .. }));
    }

    #[test]
    fn mannsetu_config_defaults_to_prod() {
        let c = MannsetuConfig::new("k", "f");
        assert_eq!(c.base_url, MANNSETU_PROD_BASE_URL);
        assert_eq!(c.timeout_seconds, 8);
    }

    #[test]
    fn mannsetu_config_can_swap_to_sandbox() {
        let c = MannsetuConfig::new("k", "f").with_base_url(MANNSETU_SANDBOX_BASE_URL);
        assert_eq!(c.base_url, MANNSETU_SANDBOX_BASE_URL);
    }
}
