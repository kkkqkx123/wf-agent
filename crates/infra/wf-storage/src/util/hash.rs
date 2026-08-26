use sha2::{Digest, Sha256};

use crate::error::StorageError;

/// Compute the integrity hash of a blob: full-stream SHA-256 over the entire
/// payload. The earlier sampling-based hash (head/middle/tail 64KB for blobs
/// over 1MB) was removed — this project does not keep backward-compatible
/// read paths, and the sampled hash could not detect mutations in the middle
/// of a large blob.
pub fn compute_hash(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let result = hasher.finalize();
    result.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Verify a blob against its stored hash (full-stream SHA-256 only).
pub fn verify_integrity(id: &str, data: &[u8], expected: &str) -> Result<(), StorageError> {
    let actual = compute_hash(data);
    if actual == expected {
        Ok(())
    } else {
        Err(StorageError::Integrity {
            id: id.to_string(),
            expected: expected.into(),
            actual,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_hash_is_sha256_of_entire_blob() {
        let data = b"hello world";
        let hash = compute_hash(data);
        assert_eq!(hash.len(), 64);
        assert_eq!(
            hash,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
        assert!(verify_integrity("id", data, &hash).is_ok());
    }

    #[test]
    fn large_blob_hash_covers_middle_bytes() {
        // 2MB blob with a distinctive byte only in the middle: the full hash
        // must change when that byte flips.
        let mut data = vec![0u8; 2 * 1024 * 1024];
        data[1024 * 1024] = 0xAB;
        let hash = compute_hash(&data);
        data[1024 * 1024] = 0xAC;
        assert_ne!(
            hash,
            compute_hash(&data),
            "middle mutation must change the hash"
        );
    }

    #[test]
    fn corrupted_blob_fails_verification() {
        let data = b"payload";
        let hash = compute_hash(data);
        let mut corrupted = data.to_vec();
        corrupted[0] ^= 0xFF;
        assert!(verify_integrity("id", &corrupted, &hash).is_err());
    }
}
