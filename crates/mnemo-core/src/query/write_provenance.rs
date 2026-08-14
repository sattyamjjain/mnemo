//! Engine surface for write provenance.
//!
//! - **Record** a tamper-evident provenance entry on the REMEMBER / SHARE write
//!   path (chained onto the store's head).
//! - **Query** it: by memory id, by principal, by session/trace id.
//! - **Verify** the whole chain is intact (tamper-evidence).
//! - **FORGET BY PROVENANCE**: revoke everything a principal or session wrote in
//!   one call. This is the point — after a poisoning incident, targeted cleanup
//!   by the responsible principal/session instead of wiping the store.

use uuid::Uuid;

use crate::error::{Error, Result};
use crate::hash::ChainVerificationResult;
use crate::model::capability::Capability;
use crate::model::write_provenance::{
    WriteFlag, WriteOp, WriteProvenance, verify_provenance_chain,
};
use crate::query::MnemoEngine;
use crate::query::forget::{self, ForgetRequest, ForgetResponse, ForgetStrategy};

impl MnemoEngine {
    /// Record a tamper-evident write-provenance record for `memory_id`, chained
    /// onto the store's current head. No-op if the backend does not record
    /// provenance. Called from the REMEMBER / SHARE write paths.
    pub(crate) async fn record_write_provenance(
        &self,
        memory_id: Uuid,
        principal: String,
        capability_id: Option<Uuid>,
        session_id: Option<String>,
        op: WriteOp,
        flags: Vec<WriteFlag>,
    ) -> Result<()> {
        if !self.storage.records_write_provenance() {
            return Ok(());
        }
        let prev_hash = self.storage.get_latest_provenance_hash().await?;
        let prov = WriteProvenance::new(
            memory_id,
            principal,
            capability_id,
            session_id,
            op,
            flags,
            prev_hash,
        );
        self.storage.insert_write_provenance(&prov).await
    }

    /// Verify a presented [`Capability`] against the configured issuer. Errors if
    /// no issuer is attached or the capability is invalid / expired.
    pub fn verify_capability(&self, cap: &Capability) -> Result<()> {
        let issuer = self.capability_issuer.as_ref().ok_or_else(|| {
            Error::PermissionDenied(
                "a capability was presented but no CapabilityIssuer is configured".to_string(),
            )
        })?;
        issuer
            .verify(cap)
            .map_err(|e| Error::PermissionDenied(format!("capability rejected: {e}")))
    }

    /// Provenance for one memory (its most recent write), if recorded.
    pub async fn write_provenance_for(&self, memory_id: Uuid) -> Result<Option<WriteProvenance>> {
        self.storage.get_write_provenance(memory_id).await
    }

    /// Everything `principal` wrote, newest first (up to `limit`).
    pub async fn writes_by_principal(
        &self,
        principal: &str,
        limit: usize,
    ) -> Result<Vec<WriteProvenance>> {
        self.storage
            .list_provenance_by_principal(principal, limit)
            .await
    }

    /// Everything written under `session_id`, newest first (up to `limit`).
    pub async fn writes_by_session(
        &self,
        session_id: &str,
        limit: usize,
    ) -> Result<Vec<WriteProvenance>> {
        self.storage
            .list_provenance_by_session(session_id, limit)
            .await
    }

    /// Verify the whole write-provenance chain is intact — tamper-evidence over
    /// the append history. `limit` bounds how far back to walk.
    pub async fn verify_provenance_chain(&self, limit: usize) -> Result<ChainVerificationResult> {
        let recs = self.storage.list_all_provenance(limit).await?;
        Ok(verify_provenance_chain(&recs))
    }

    /// FORGET BY PROVENANCE: revoke everything `principal` authored (REMEMBER),
    /// using `strategy` (SoftDelete / HardDelete / Redact), in one call. This is
    /// remediation — targeted at the responsible principal — not a wipe.
    pub async fn forget_by_principal(
        &self,
        principal: &str,
        strategy: ForgetStrategy,
    ) -> Result<ForgetResponse> {
        let ids = self.storage.list_memory_ids_by_principal(principal).await?;
        self.forget_ids(ids, strategy).await
    }

    /// FORGET BY PROVENANCE by session / trace id.
    pub async fn forget_by_session(
        &self,
        session_id: &str,
        strategy: ForgetStrategy,
    ) -> Result<ForgetResponse> {
        let ids = self.storage.list_memory_ids_by_session(session_id).await?;
        self.forget_ids(ids, strategy).await
    }

    async fn forget_ids(&self, ids: Vec<Uuid>, strategy: ForgetStrategy) -> Result<ForgetResponse> {
        if ids.is_empty() {
            return Ok(ForgetResponse {
                forgotten: Vec::new(),
                errors: Vec::new(),
            });
        }
        let request = ForgetRequest {
            memory_ids: ids,
            agent_id: None,
            strategy: Some(strategy),
            criteria: None,
        };
        forget::execute(self, request).await
    }
}
