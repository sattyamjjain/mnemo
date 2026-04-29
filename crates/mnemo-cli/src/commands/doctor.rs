//! `mnemo doctor` — operator-grade self-diagnosis (v0.4.1 P2-6).
//!
//! Pure-fn diagnostics that produce a typed [`DoctorReport`] +
//! actionable [`DoctorFix`] suggestions. The CLI binary wires the
//! concrete checks; this module ships the report shape + the
//! recommendation logic so it's unit-testable without spinning up
//! a real engine.
//!
//! The `mnemo doctor` + `mnemo dashboard` clap subcommands land in a
//! follow-up that wires this report through the binary's command
//! enum (mirrors the v0.4.0-rc3 B2 / B6 wiring pattern).
//! `#[allow(dead_code)]` documents the gap precisely.

#![allow(dead_code)]

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DoctorFix {
    RebuildVectorIndex,
    RotateHmacKey,
    RepinMcpCatalog,
    EnableDecayLane,
    UpgradeRmcp,
}

impl DoctorFix {
    pub fn as_str(&self) -> &'static str {
        match self {
            DoctorFix::RebuildVectorIndex => "rebuild_vector_index",
            DoctorFix::RotateHmacKey => "rotate_hmac_key",
            DoctorFix::RepinMcpCatalog => "repin_mcp_catalog",
            DoctorFix::EnableDecayLane => "enable_decay_lane",
            DoctorFix::UpgradeRmcp => "upgrade_rmcp",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DoctorReport {
    pub duckdb_ok: bool,
    pub hmac_chain_intact: bool,
    pub mcp_catalog_pinned: bool,
    pub mesh_identity_present: Option<bool>,
    pub recall_p50_ms: f32,
    pub fixes: Vec<DoctorFix>,
}

impl DoctorReport {
    /// Build a report and infer fixes from the boolean signals.
    pub fn build(
        duckdb_ok: bool,
        hmac_chain_intact: bool,
        mcp_catalog_pinned: bool,
        mesh_identity_present: Option<bool>,
        recall_p50_ms: f32,
    ) -> Self {
        let mut fixes = Vec::new();
        if !duckdb_ok {
            fixes.push(DoctorFix::RebuildVectorIndex);
        }
        if !hmac_chain_intact {
            fixes.push(DoctorFix::RotateHmacKey);
        }
        if !mcp_catalog_pinned {
            fixes.push(DoctorFix::RepinMcpCatalog);
        }
        if recall_p50_ms > 50.0 {
            fixes.push(DoctorFix::EnableDecayLane);
        }
        Self {
            duckdb_ok,
            hmac_chain_intact,
            mcp_catalog_pinned,
            mesh_identity_present,
            recall_p50_ms,
            fixes,
        }
    }

    pub fn is_clean(&self) -> bool {
        self.fixes.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_install_reports_no_fixes() {
        let r = DoctorReport::build(true, true, true, Some(true), 5.0);
        assert!(r.is_clean());
        assert!(r.fixes.is_empty());
    }

    #[test]
    fn broken_chain_recommends_rotate() {
        let r = DoctorReport::build(true, false, true, None, 5.0);
        assert!(r.fixes.contains(&DoctorFix::RotateHmacKey));
    }

    #[test]
    fn slow_recall_suggests_decay_lane() {
        let r = DoctorReport::build(true, true, true, None, 75.0);
        assert!(r.fixes.contains(&DoctorFix::EnableDecayLane));
    }

    #[test]
    fn fix_strings_round_trip() {
        for f in [
            DoctorFix::RebuildVectorIndex,
            DoctorFix::RotateHmacKey,
            DoctorFix::RepinMcpCatalog,
            DoctorFix::EnableDecayLane,
            DoctorFix::UpgradeRmcp,
        ] {
            assert!(!f.as_str().is_empty());
        }
    }
}
