//! TOML loader for the `[tool_catalog_pin]` block in the manifest
//! (v0.4.0 P0-1).
//!
//! Schema:
//!
//! ```toml
//! [tool_catalog_pin]
//! signer = "mnemo-prod:catalog-pin-2026-04"
//! signed_at = "2026-04-27T00:00:00Z"
//!
//! [[tool_catalog_pin.tools]]
//! name = "mnemo.recall"
//! schema_sha256 = "abc123..."
//! ```
//!
//! Stored alongside (not inside) the keystore so an operator can
//! rotate the HMAC key without re-signing the catalog pin and
//! vice-versa.

use std::path::Path;
use std::time::SystemTime;

use serde::Deserialize;

use super::{PinnedToolCatalog, ToolFingerprint};

#[derive(Debug, Deserialize)]
struct PinFile {
    tool_catalog_pin: PinSection,
}

#[derive(Debug, Deserialize)]
struct PinSection {
    signer: String,
    /// RFC3339 timestamp; converted to `SystemTime` after load.
    signed_at: String,
    tools: Vec<PinnedToolEntry>,
}

#[derive(Debug, Deserialize)]
struct PinnedToolEntry {
    name: String,
    schema_sha256: String,
}

#[derive(Debug, thiserror::Error)]
pub enum CatalogPinError {
    #[error("catalog-pin file not found at {0}")]
    NotFound(std::path::PathBuf),
    #[error("catalog-pin IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("catalog-pin TOML parse error: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("catalog-pin {field} is invalid: {reason}")]
    Invalid { field: &'static str, reason: String },
}

pub fn load(path: &Path) -> Result<PinnedToolCatalog, CatalogPinError> {
    if !path.exists() {
        return Err(CatalogPinError::NotFound(path.to_path_buf()));
    }
    let text = std::fs::read_to_string(path)?;
    let parsed: PinFile = toml::from_str(&text)?;
    let signed_at = chrono::DateTime::parse_from_rfc3339(&parsed.tool_catalog_pin.signed_at)
        .map_err(|e| CatalogPinError::Invalid {
            field: "signed_at",
            reason: e.to_string(),
        })?
        .with_timezone(&chrono::Utc);
    let signed_at: SystemTime = signed_at.into();

    let mut tools = Vec::with_capacity(parsed.tool_catalog_pin.tools.len());
    for t in parsed.tool_catalog_pin.tools {
        let bytes = hex::decode(&t.schema_sha256).map_err(|e| CatalogPinError::Invalid {
            field: "schema_sha256",
            reason: format!("not valid hex for tool {:?}: {e}", t.name),
        })?;
        if bytes.len() != 32 {
            return Err(CatalogPinError::Invalid {
                field: "schema_sha256",
                reason: format!(
                    "expected 32 bytes for tool {:?} (got {})",
                    t.name,
                    bytes.len()
                ),
            });
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        tools.push(ToolFingerprint {
            name: t.name,
            schema_sha256: arr,
        });
    }

    Ok(PinnedToolCatalog {
        signer: parsed.tool_catalog_pin.signer,
        signed_at,
        tools,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn load_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pin.toml");
        let mut f = std::fs::File::create(&path).unwrap();
        let body = r#"
[tool_catalog_pin]
signer = "test-signer"
signed_at = "2026-04-27T12:00:00Z"

[[tool_catalog_pin.tools]]
name = "mnemo.recall"
schema_sha256 = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff"

[[tool_catalog_pin.tools]]
name = "mnemo.verify"
schema_sha256 = "ffeeddccbbaa99887766554433221100ffeeddccbbaa99887766554433221100"
"#;
        f.write_all(body.as_bytes()).unwrap();
        let pin = load(&path).unwrap();
        assert_eq!(pin.signer, "test-signer");
        assert_eq!(pin.tools.len(), 2);
        assert_eq!(pin.tools[0].name, "mnemo.recall");
    }

    #[test]
    fn invalid_hex_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pin.toml");
        std::fs::write(
            &path,
            r#"
[tool_catalog_pin]
signer = "x"
signed_at = "2026-04-27T12:00:00Z"
[[tool_catalog_pin.tools]]
name = "t"
schema_sha256 = "zz"
"#,
        )
        .unwrap();
        let err = load(&path).unwrap_err();
        assert!(matches!(
            err,
            CatalogPinError::Invalid {
                field: "schema_sha256",
                ..
            }
        ));
    }

    #[test]
    fn missing_file_is_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing.toml");
        let err = load(&path).unwrap_err();
        assert!(matches!(err, CatalogPinError::NotFound(_)));
    }
}
