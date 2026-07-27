use sha2::{Digest, Sha256};

use crate::error::StorageError;

const SAMPLE_SIZE: usize = 65536;
const FULL_HASH_THRESHOLD: usize = 1_048_576;

pub fn compute_hash(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    if data.len() <= FULL_HASH_THRESHOLD {
        hasher.update(data);
    } else {
        let mid = data.len() / 2;
        hasher.update(&data[..SAMPLE_SIZE]);
        hasher.update(&data[mid - SAMPLE_SIZE / 2..mid + SAMPLE_SIZE / 2]);
        hasher.update(&data[data.len() - SAMPLE_SIZE..]);
    }
    let result = hasher.finalize();
    result.iter().map(|b| format!("{:02x}", b)).collect()
}

pub fn verify_integrity(id: &str, data: &[u8], expected: &str) -> Result<(), StorageError> {
    let actual = compute_hash(data);
    if actual != expected {
        return Err(StorageError::Integrity {
            id: id.to_string(),
            expected: expected.into(),
            actual,
        });
    }
    Ok(())
}
