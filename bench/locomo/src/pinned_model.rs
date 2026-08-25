//! Fail-closed model pinning: a benchmark may only load weights whose SHA-256
//! matches a digest committed in the source tree.
//!
//! # Why a path is not a pin
//!
//! [`crate::real_embedder::guard_real_embedder`] answers "is this a real
//! embedder"; it cannot answer "is this the *same* real embedder". Two
//! checkpoints can sit at the same path under the same directory name and
//! produce different numbers, so a result file that records only
//! `--onnx-model /path/to/model.onnx` does not let a stranger reproduce it —
//! they can run the command and legitimately get a different figure.
//!
//! `locomo_v1_bench` already records the digest of the weights it loaded, which
//! makes a published number *auditable after the fact*. This module is the
//! stronger property: the run **refuses to start** unless the weights are the
//! pinned ones, so a mismatched checkpoint cannot produce a number that then has
//! to be caught by someone reading the JSON.
//!
//! Fail-closed in both directions that matter:
//!
//! * a digest that does not match is an error, never a warning;
//! * an *absent* expected digest is also an error — "nothing to compare against"
//!   must not read as "comparison passed", which is the usual way a pin quietly
//!   stops pinning.

use std::fmt;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// Why a pinned-model load was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelPinError {
    /// The file could not be read.
    Unreadable { path: PathBuf, detail: String },
    /// The file was read but hashes to something other than the pin.
    DigestMismatch {
        path: PathBuf,
        expected: String,
        actual: String,
    },
    /// No expected digest was supplied. Deliberately an error: a pin with
    /// nothing to compare against is not a pin.
    NoExpectedDigest { path: PathBuf },
}

impl fmt::Display for ModelPinError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unreadable { path, detail } => write!(
                f,
                "refusing to score: cannot read model weights at {} ({detail}). \
                 Fetch the pinned artifact first; see the bench README for the URL.",
                path.display()
            ),
            Self::DigestMismatch {
                path,
                expected,
                actual,
            } => write!(
                f,
                "refusing to score: model weights at {} do NOT match the pinned digest.\n  \
                 expected sha256 {expected}\n  actual   sha256 {actual}\n\
                 A number produced by different weights is not the published number, and \
                 nobody downstream can tell the difference from the result file alone. \
                 Re-fetch the pinned artifact, or update the pin deliberately in source.",
                path.display()
            ),
            Self::NoExpectedDigest { path } => write!(
                f,
                "refusing to score: no expected digest was supplied for {}. \
                 An absent pin is not a satisfied pin.",
                path.display()
            ),
        }
    }
}

impl std::error::Error for ModelPinError {}

/// SHA-256 of a file, lowercase hex. Streams the file so a ~90 MB checkpoint
/// does not have to be held in memory twice.
pub fn file_sha256(path: &Path) -> Result<String, ModelPinError> {
    use std::io::Read;

    let mut f = std::fs::File::open(path).map_err(|e| ModelPinError::Unreadable {
        path: path.to_path_buf(),
        detail: e.to_string(),
    })?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf).map_err(|e| ModelPinError::Unreadable {
            path: path.to_path_buf(),
            detail: e.to_string(),
        })?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// Verify `path` hashes to `expected`, or refuse.
///
/// `expected` is `Option` only so the "no pin supplied" case is representable
/// and can be rejected explicitly rather than skipped.
pub fn verify_pinned(path: &Path, expected: Option<&str>) -> Result<String, ModelPinError> {
    let Some(expected) = expected.filter(|s| !s.trim().is_empty()) else {
        return Err(ModelPinError::NoExpectedDigest {
            path: path.to_path_buf(),
        });
    };
    let actual = file_sha256(path)?;
    // Case-insensitive: a hex digest pasted from a different tool may be upper.
    if !actual.eq_ignore_ascii_case(expected.trim()) {
        return Err(ModelPinError::DigestMismatch {
            path: path.to_path_buf(),
            expected: expected.trim().to_string(),
            actual,
        });
    }
    Ok(actual)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn tmp_with(bytes: &[u8]) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("model.onnx");
        std::fs::File::create(&p).unwrap().write_all(bytes).unwrap();
        (dir, p)
    }

    /// Known answer, so a broken hasher cannot pass by agreeing with itself.
    #[test]
    fn sha256_matches_the_known_digest_of_abc() {
        let (_d, p) = tmp_with(b"abc");
        assert_eq!(
            file_sha256(&p).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn matching_digest_is_accepted() {
        let (_d, p) = tmp_with(b"abc");
        let want = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert_eq!(verify_pinned(&p, Some(want)).unwrap(), want);
        // Upper-case pin is the same pin.
        assert!(verify_pinned(&p, Some(&want.to_uppercase())).is_ok());
    }

    /// The direction that matters: a different checkpoint must be refused, and
    /// the error must name both digests so the failure is actionable.
    #[test]
    fn mismatched_digest_is_refused_and_names_both() {
        let (_d, p) = tmp_with(b"a different checkpoint");
        let err = verify_pinned(&p, Some("0".repeat(64).as_str())).unwrap_err();
        match &err {
            ModelPinError::DigestMismatch {
                expected, actual, ..
            } => {
                assert_eq!(expected, &"0".repeat(64));
                assert_ne!(expected, actual);
            }
            other => panic!("expected DigestMismatch, got {other:?}"),
        }
        let msg = err.to_string();
        assert!(msg.contains("do NOT match"), "{msg}");
        assert!(msg.contains("expected sha256"), "{msg}");
    }

    /// An absent pin must NOT read as a satisfied pin. This is the failure mode
    /// that turns a pin into decoration without anything going red.
    #[test]
    fn absent_or_blank_pin_is_an_error_not_a_pass() {
        let (_d, p) = tmp_with(b"abc");
        assert!(matches!(
            verify_pinned(&p, None),
            Err(ModelPinError::NoExpectedDigest { .. })
        ));
        assert!(matches!(
            verify_pinned(&p, Some("   ")),
            Err(ModelPinError::NoExpectedDigest { .. })
        ));
    }

    #[test]
    fn missing_file_is_unreadable_not_a_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let err = verify_pinned(&dir.path().join("absent.onnx"), Some("aa")).unwrap_err();
        assert!(matches!(err, ModelPinError::Unreadable { .. }), "{err:?}");
    }
}
