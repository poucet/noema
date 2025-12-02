//! Content-Addressable Storage (CAS) for binary assets
//!
//! Files are stored by their SHA-256 hash, enabling:
//! - Deduplication (same content stored once)
//! - Integrity verification (hash validates content)
//! - Efficient storage (no Base64 overhead)

use sha2::{Digest, Sha256};
use std::fs;
use std::io::{self, Read, Write};
use std::path::PathBuf;

/// Content-addressable blob storage
///
/// Files are stored in a sharded directory structure based on the first 2 characters
/// of their SHA-256 hash: `blob_storage/{hash[0:2]}/{hash}`
#[derive(Debug, Clone)]
pub struct BlobStore {
    root: PathBuf,
}

/// Result of storing a blob
#[derive(Debug, Clone)]
pub struct StoredBlob {
    /// SHA-256 hash of the content (also serves as the blob ID)
    pub hash: String,
    /// Size in bytes
    pub size: usize,
    /// Whether this was a new blob (false if already existed)
    pub is_new: bool,
}

impl BlobStore {
    /// Create a new BlobStore with the given root directory
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Store binary data and return its SHA-256 hash
    ///
    /// If the blob already exists (same hash), this is a no-op and returns the existing hash.
    pub fn store(&self, data: &[u8]) -> io::Result<StoredBlob> {
        let hash = Self::compute_hash(data);
        let path = self.path_for(&hash);

        // Check if already exists (deduplication)
        if path.exists() {
            return Ok(StoredBlob {
                hash,
                size: data.len(),
                is_new: false,
            });
        }

        // Create shard directory if needed
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Write atomically using a temp file
        let temp_path = path.with_extension("tmp");
        let mut file = fs::File::create(&temp_path)?;
        file.write_all(data)?;
        file.sync_all()?;
        fs::rename(&temp_path, &path)?;

        Ok(StoredBlob {
            hash,
            size: data.len(),
            is_new: true,
        })
    }

    /// Retrieve blob data by hash
    pub fn get(&self, hash: &str) -> io::Result<Vec<u8>> {
        let path = self.path_for(hash);
        fs::read(&path)
    }

    /// Read blob into a pre-allocated buffer
    pub fn get_into(&self, hash: &str, buf: &mut Vec<u8>) -> io::Result<usize> {
        let path = self.path_for(hash);
        let mut file = fs::File::open(&path)?;
        buf.clear();
        file.read_to_end(buf)
    }

    /// Check if a blob exists
    pub fn exists(&self, hash: &str) -> bool {
        self.path_for(hash).exists()
    }

    /// Delete a blob by hash
    ///
    /// Returns Ok(true) if deleted, Ok(false) if didn't exist
    pub fn delete(&self, hash: &str) -> io::Result<bool> {
        let path = self.path_for(hash);
        if path.exists() {
            fs::remove_file(&path)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Get the filesystem path for a blob
    pub fn path_for(&self, hash: &str) -> PathBuf {
        if hash.len() < 2 {
            return self.root.join(hash);
        }
        let shard = &hash[0..2];
        self.root.join(shard).join(hash)
    }

    /// Get the size of a blob without reading its contents
    pub fn size(&self, hash: &str) -> io::Result<u64> {
        let path = self.path_for(hash);
        let metadata = fs::metadata(&path)?;
        Ok(metadata.len())
    }

    /// Compute SHA-256 hash of data
    pub fn compute_hash(data: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(data);
        let result = hasher.finalize();
        hex::encode(result)
    }

    /// Verify that a blob's content matches its hash
    pub fn verify(&self, hash: &str) -> io::Result<bool> {
        let data = self.get(hash)?;
        let computed = Self::compute_hash(&data);
        Ok(computed == hash)
    }

    /// List all blob hashes in the store
    pub fn list_all(&self) -> io::Result<Vec<String>> {
        let mut hashes = Vec::new();

        if !self.root.exists() {
            return Ok(hashes);
        }

        for shard_entry in fs::read_dir(&self.root)? {
            let shard_entry = shard_entry?;
            let shard_path = shard_entry.path();

            if !shard_path.is_dir() {
                continue;
            }

            for blob_entry in fs::read_dir(&shard_path)? {
                let blob_entry = blob_entry?;
                let blob_path = blob_entry.path();

                if blob_path.is_file() {
                    if let Some(filename) = blob_path.file_name() {
                        if let Some(hash) = filename.to_str() {
                            // Skip temp files
                            if !hash.ends_with(".tmp") {
                                hashes.push(hash.to_string());
                            }
                        }
                    }
                }
            }
        }

        Ok(hashes)
    }

    /// Get total size of all blobs in bytes
    pub fn total_size(&self) -> io::Result<u64> {
        let mut total = 0u64;

        for hash in self.list_all()? {
            total += self.size(&hash)?;
        }

        Ok(total)
    }

    /// Clean up orphaned temp files
    pub fn cleanup_temp_files(&self) -> io::Result<usize> {
        let mut cleaned = 0;

        if !self.root.exists() {
            return Ok(0);
        }

        for shard_entry in fs::read_dir(&self.root)? {
            let shard_entry = shard_entry?;
            let shard_path = shard_entry.path();

            if !shard_path.is_dir() {
                continue;
            }

            for blob_entry in fs::read_dir(&shard_path)? {
                let blob_entry = blob_entry?;
                let blob_path = blob_entry.path();

                if let Some(ext) = blob_path.extension() {
                    if ext == "tmp" {
                        fs::remove_file(&blob_path)?;
                        cleaned += 1;
                    }
                }
            }
        }

        Ok(cleaned)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    fn temp_blob_store() -> BlobStore {
        let dir = env::temp_dir().join(format!("blob_test_{}", uuid::Uuid::new_v4()));
        BlobStore::new(dir)
    }

    #[test]
    fn test_store_and_retrieve() {
        let store = temp_blob_store();
        let data = b"Hello, World!";

        let stored = store.store(data).unwrap();
        assert!(stored.is_new);
        assert_eq!(stored.size, data.len());

        let retrieved = store.get(&stored.hash).unwrap();
        assert_eq!(retrieved, data);

        // Clean up
        fs::remove_dir_all(&store.root).ok();
    }

    #[test]
    fn test_deduplication() {
        let store = temp_blob_store();
        let data = b"Duplicate data";

        let first = store.store(data).unwrap();
        assert!(first.is_new);

        let second = store.store(data).unwrap();
        assert!(!second.is_new);
        assert_eq!(first.hash, second.hash);

        // Clean up
        fs::remove_dir_all(&store.root).ok();
    }

    #[test]
    fn test_hash_computation() {
        let data = b"test";
        let hash = BlobStore::compute_hash(data);
        // Known SHA-256 hash of "test"
        assert_eq!(
            hash,
            "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"
        );
    }

    #[test]
    fn test_verify() {
        let store = temp_blob_store();
        let data = b"Verify me";

        let stored = store.store(data).unwrap();
        assert!(store.verify(&stored.hash).unwrap());

        // Clean up
        fs::remove_dir_all(&store.root).ok();
    }

    #[test]
    fn test_delete() {
        let store = temp_blob_store();
        let data = b"Delete me";

        let stored = store.store(data).unwrap();
        assert!(store.exists(&stored.hash));

        assert!(store.delete(&stored.hash).unwrap());
        assert!(!store.exists(&stored.hash));
        assert!(!store.delete(&stored.hash).unwrap());

        // Clean up
        fs::remove_dir_all(&store.root).ok();
    }
}
