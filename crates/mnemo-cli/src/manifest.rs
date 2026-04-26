//! TOML manifest the operator hands `mnemo-mcp-server` via
//! `--manifest <path>`. Designed so an attacker who can spawn the
//! binary cannot pass arbitrary capabilities via env vars or
//! command-line flags — every privileged knob lives in this file.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    pub keystore_path: PathBuf,
    pub audit_log_path: PathBuf,
    #[serde(default = "default_allowed_tools")]
    pub allowed_tools: BTreeSet<String>,
    #[serde(default)]
    pub allowed_agents: BTreeSet<String>,
    #[serde(default = "default_allowed_parents")]
    pub allowed_parents: BTreeSet<String>,
    #[serde(default = "default_lease_ttl_seconds")]
    pub lease_ttl_seconds: u64,
}

fn default_allowed_tools() -> BTreeSet<String> {
    ["mnemo.recall", "mnemo.verify"]
        .into_iter()
        .map(String::from)
        .collect()
}

fn default_allowed_parents() -> BTreeSet<String> {
    ["claude", "claude-code", "systemd", "supervisord"]
        .into_iter()
        .map(String::from)
        .collect()
}

fn default_lease_ttl_seconds() -> u64 {
    60
}

#[derive(Debug, Error)]
pub enum ManifestError {
    #[error("manifest file not found at {path}")]
    NotFound { path: PathBuf },
    #[error("manifest IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("manifest TOML parse error: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("manifest field {field} is invalid: {reason}")]
    Invalid { field: &'static str, reason: String },
}

impl Manifest {
    pub fn load(path: &Path) -> Result<Self, ManifestError> {
        if !path.exists() {
            return Err(ManifestError::NotFound { path: path.into() });
        }
        let text = std::fs::read_to_string(path)?;
        let m: Manifest = toml::from_str(&text)?;
        m.validate()?;
        Ok(m)
    }

    fn validate(&self) -> Result<(), ManifestError> {
        if self.lease_ttl_seconds == 0 {
            return Err(ManifestError::Invalid {
                field: "lease_ttl_seconds",
                reason: "must be > 0".into(),
            });
        }
        if self.lease_ttl_seconds > 3600 {
            return Err(ManifestError::Invalid {
                field: "lease_ttl_seconds",
                reason: "must be <= 3600 (1 hour)".into(),
            });
        }
        for t in &self.allowed_tools {
            if !KNOWN_TOOLS.contains(&t.as_str()) {
                return Err(ManifestError::Invalid {
                    field: "allowed_tools",
                    reason: format!("unknown tool {:?}; valid: {:?}", t, KNOWN_TOOLS),
                });
            }
        }
        Ok(())
    }
}

pub const KNOWN_TOOLS: &[&str] = &[
    "mnemo.remember",
    "mnemo.recall",
    "mnemo.reflect",
    "mnemo.forget_subject",
    "mnemo.export_audit_log",
    "mnemo.verify",
];

/// On-disk keystore the manifest points at via `keystore_path`. The
/// HMAC key is hex-encoded so operators can `chmod 0400` a single
/// readable file and rotate by writing a new `key_id` next to a new
/// `key_hex`. Used by the mcp-server hardened mode to attach a
/// `ProvenanceSigner` to the engine without ever passing key material
/// through env vars or argv.
#[derive(Debug, Deserialize)]
pub struct Keystore {
    pub key_id: String,
    pub key_hex: String,
}

impl Keystore {
    pub fn load(path: &Path) -> Result<Self, ManifestError> {
        if !path.exists() {
            return Err(ManifestError::NotFound { path: path.into() });
        }
        let text = std::fs::read_to_string(path)?;
        let k: Keystore = toml::from_str(&text)?;
        if k.key_id.is_empty() {
            return Err(ManifestError::Invalid {
                field: "key_id",
                reason: "must not be empty".into(),
            });
        }
        let bytes = hex::decode(&k.key_hex).map_err(|e| ManifestError::Invalid {
            field: "key_hex",
            reason: format!("not valid hex: {e}"),
        })?;
        if bytes.len() < 32 {
            return Err(ManifestError::Invalid {
                field: "key_hex",
                reason: format!("must decode to >= 32 bytes (got {})", bytes.len()),
            });
        }
        Ok(k)
    }

    pub fn key_bytes(&self) -> Result<Vec<u8>, ManifestError> {
        hex::decode(&self.key_hex).map_err(|e| ManifestError::Invalid {
            field: "key_hex",
            reason: format!("not valid hex: {e}"),
        })
    }
}
