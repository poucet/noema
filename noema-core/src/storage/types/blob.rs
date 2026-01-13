//! Blob storage types

use std::str::FromStr;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Result of storing a blob
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BlobHash(String);

impl FromStr for BlobHash {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.to_string()))
    }
}

impl BlobHash {
    /// Create a BlobHash from raw data by computing its SHA-256 hash
    pub fn hash(data: &[u8]) -> Self {
        let mut hasher = sha2::Sha256::new();
        hasher.update(data);
        let hash = hex::encode(hasher.finalize());
        Self(hash)
    }

    /// Get the inner hash string
    pub fn as_str(&self) -> &str {
        &self.0
    }
}