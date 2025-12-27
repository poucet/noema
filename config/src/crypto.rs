//! Encryption utilities for secure API key storage.
//!
//! Uses AES-256-GCM encryption with a machine-specific key derived from
//! the hostname and username.

use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use rand::Rng;
use sha2::{Digest, Sha256};

const NONCE_SIZE: usize = 12;

/// Derive a 256-bit encryption key from machine-specific identifiers.
fn derive_key() -> [u8; 32] {
    let hostname = whoami::fallible::hostname().unwrap_or_else(|_| "unknown".to_string());
    let username = whoami::username();

    // Combine hostname and username with a static salt
    let mut hasher = Sha256::new();
    hasher.update(b"noema-api-key-encryption-v1");
    hasher.update(hostname.as_bytes());
    hasher.update(b":");
    hasher.update(username.as_bytes());

    let result = hasher.finalize();
    let mut key = [0u8; 32];
    key.copy_from_slice(&result);
    key
}

/// Encrypt a string using AES-256-GCM with a machine-specific key.
///
/// Returns a base64-encoded string containing the nonce and ciphertext.
pub fn encrypt_string(plaintext: &str) -> Result<String, String> {
    let key = derive_key();
    let cipher =
        Aes256Gcm::new_from_slice(&key).map_err(|e| format!("Failed to create cipher: {}", e))?;

    // Generate a random nonce
    let mut nonce_bytes = [0u8; NONCE_SIZE];
    rand::rng().fill(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    // Encrypt the plaintext
    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| format!("Encryption failed: {}", e))?;

    // Combine nonce and ciphertext, then base64 encode
    let mut combined = Vec::with_capacity(NONCE_SIZE + ciphertext.len());
    combined.extend_from_slice(&nonce_bytes);
    combined.extend_from_slice(&ciphertext);

    Ok(BASE64.encode(&combined))
}

/// Decrypt a base64-encoded encrypted string using AES-256-GCM.
///
/// The input should be the output from `encrypt_string`.
pub fn decrypt_string(encrypted: &str) -> Result<String, String> {
    let key = derive_key();
    let cipher =
        Aes256Gcm::new_from_slice(&key).map_err(|e| format!("Failed to create cipher: {}", e))?;

    // Decode base64
    let combined = BASE64
        .decode(encrypted)
        .map_err(|e| format!("Failed to decode base64: {}", e))?;

    if combined.len() < NONCE_SIZE {
        return Err("Encrypted data too short".to_string());
    }

    // Split nonce and ciphertext
    let (nonce_bytes, ciphertext) = combined.split_at(NONCE_SIZE);
    let nonce = Nonce::from_slice(nonce_bytes);

    // Decrypt
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| format!("Decryption failed: {}", e))?;

    String::from_utf8(plaintext).map_err(|e| format!("Invalid UTF-8 in decrypted data: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let original = "sk-test-api-key-12345";
        let encrypted = encrypt_string(original).expect("encryption failed");
        let decrypted = decrypt_string(&encrypted).expect("decryption failed");
        assert_eq!(original, decrypted);
    }

    #[test]
    fn test_encrypt_produces_different_output_each_time() {
        let original = "test-key";
        let encrypted1 = encrypt_string(original).expect("encryption failed");
        let encrypted2 = encrypt_string(original).expect("encryption failed");
        // Due to random nonce, encrypted values should differ
        assert_ne!(encrypted1, encrypted2);
    }

    #[test]
    fn test_decrypt_invalid_base64_fails() {
        let result = decrypt_string("not-valid-base64!!!");
        assert!(result.is_err());
    }

    #[test]
    fn test_decrypt_too_short_fails() {
        let result = decrypt_string(&BASE64.encode(b"short"));
        assert!(result.is_err());
    }
}
