//! AES-256-GCM encryption for memory content at rest.
//!
//! Provides encrypt/decrypt operations for memory content before storage.
//! The encryption key is loaded from an environment variable or passed directly.

use crate::error::{Error, Result};

use aes_gcm::{
    Aes256Gcm, Key, Nonce,
    aead::{Aead, KeyInit, OsRng},
};
use aes_gcm::aead::rand_core::RngCore;

/// AES-256-GCM encryption provider for at-rest memory content.
pub struct ContentEncryption {
    key: [u8; 32],
}

impl ContentEncryption {
    /// Create from a 32-byte key.
    pub fn new(key: [u8; 32]) -> Self {
        Self { key }
    }

    /// Create from a hex-encoded key string (64 hex chars = 32 bytes).
    pub fn from_hex(hex_key: &str) -> Result<Self> {
        let bytes = hex::decode(hex_key)
            .map_err(|e| Error::Validation(format!("invalid hex key: {e}")))?;
        if bytes.len() != 32 {
            return Err(Error::Validation(format!(
                "key must be 32 bytes, got {}",
                bytes.len()
            )));
        }
        let mut key = [0u8; 32];
        key.copy_from_slice(&bytes);
        Ok(Self { key })
    }

    /// Create from the `MNEMO_ENCRYPTION_KEY` environment variable.
    pub fn from_env() -> Result<Self> {
        let hex_key = std::env::var("MNEMO_ENCRYPTION_KEY")
            .map_err(|_| Error::Validation("MNEMO_ENCRYPTION_KEY not set".to_string()))?;
        Self::from_hex(&hex_key)
    }

    /// Encrypt plaintext content. Returns `nonce(12) || ciphertext+tag` as bytes.
    ///
    /// Uses AES-256-GCM with a random 12-byte nonce.
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>> {
        let key = Key::<Aes256Gcm>::from_slice(&self.key);
        let cipher = Aes256Gcm::new(key);

        let mut nonce_bytes = [0u8; 12];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce, plaintext)
            .map_err(|e| Error::Internal(format!("encryption failed: {e}")))?;

        let mut output = Vec::with_capacity(12 + ciphertext.len());
        output.extend_from_slice(&nonce_bytes);
        output.extend_from_slice(&ciphertext);
        Ok(output)
    }

    /// Decrypt content encrypted by [`encrypt`].
    pub fn decrypt(&self, data: &[u8]) -> Result<Vec<u8>> {
        if data.len() < 28 {
            // 12 nonce + 16 tag minimum
            return Err(Error::Validation("encrypted data too short".to_string()));
        }

        let key = Key::<Aes256Gcm>::from_slice(&self.key);
        let cipher = Aes256Gcm::new(key);

        let nonce = Nonce::from_slice(&data[..12]);
        let ciphertext = &data[12..];

        cipher
            .decrypt(nonce, ciphertext)
            .map_err(|_| Error::Validation("decryption tag mismatch".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encryption_round_trip() {
        let key = [0x42u8; 32];
        let enc = ContentEncryption::new(key);

        let plaintext = b"Hello, encrypted world!";
        let encrypted = enc.encrypt(plaintext).unwrap();

        assert_ne!(&encrypted[12..encrypted.len() - 16], plaintext);

        let decrypted = enc.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_encryption_from_hex() {
        let hex_key = "42".repeat(32);
        let enc = ContentEncryption::from_hex(&hex_key).unwrap();

        let plaintext = b"test";
        let encrypted = enc.encrypt(plaintext).unwrap();
        let decrypted = enc.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_invalid_hex_key_length() {
        let result = ContentEncryption::from_hex("abcd");
        assert!(result.is_err());
    }

    #[test]
    fn test_tampered_ciphertext_fails() {
        let key = [0x42u8; 32];
        let enc = ContentEncryption::new(key);

        let encrypted = enc.encrypt(b"secret data").unwrap();
        let mut tampered = encrypted.clone();
        tampered[15] ^= 0xff; // flip a byte in the ciphertext

        let result = enc.decrypt(&tampered);
        assert!(result.is_err());
    }

    #[test]
    fn test_aes_gcm_round_trip() {
        let key = [0xABu8; 32];
        let enc = ContentEncryption::new(key);

        // Test various sizes
        for size in [0, 1, 16, 100, 1024, 65536] {
            let plaintext: Vec<u8> = (0..size).map(|i| (i % 256) as u8).collect();
            let encrypted = enc.encrypt(&plaintext).unwrap();
            let decrypted = enc.decrypt(&encrypted).unwrap();
            assert_eq!(decrypted, plaintext, "round-trip failed for size {size}");
        }
    }

    #[test]
    fn test_aes_gcm_tamper_detection() {
        let key = [0xCDu8; 32];
        let enc = ContentEncryption::new(key);
        let encrypted = enc.encrypt(b"sensitive data").unwrap();

        // Tamper with nonce
        let mut tampered = encrypted.clone();
        tampered[0] ^= 0x01;
        assert!(enc.decrypt(&tampered).is_err());

        // Tamper with ciphertext body
        let mut tampered = encrypted.clone();
        tampered[14] ^= 0x01;
        assert!(enc.decrypt(&tampered).is_err());

        // Tamper with last byte (tag)
        let mut tampered = encrypted.clone();
        let last = tampered.len() - 1;
        tampered[last] ^= 0x01;
        assert!(enc.decrypt(&tampered).is_err());
    }
}
