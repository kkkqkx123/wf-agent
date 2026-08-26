use std::io::{Read, Write};

use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;

use crate::error::StorageError;

/// Compression format unified with the checkpoint serializer:
/// gzip instead of the earlier zlib streams. The checkpoint layer keeps a
/// magic-byte auto-detection; entity stores track compression with the
/// `compressed` metadata flag, so the algorithm swap is transparent on read
/// for data written after this change.
pub fn compress(data: &[u8]) -> Result<Vec<u8>, StorageError> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data).map_err(|e| StorageError::General {
        operation: "compress".into(),
        message: e.to_string(),
        source: Some(Box::new(e)),
    })?;
    encoder.finish().map_err(|e| StorageError::General {
        operation: "compress".into(),
        message: e.to_string(),
        source: Some(Box::new(e)),
    })
}

pub fn decompress(data: &[u8]) -> Result<Vec<u8>, StorageError> {
    let mut decoder = GzDecoder::new(data);
    let mut result = Vec::new();
    decoder
        .read_to_end(&mut result)
        .map_err(|e| StorageError::General {
            operation: "decompress".into(),
            message: e.to_string(),
            source: Some(Box::new(e)),
        })?;
    Ok(result)
}

/// True when the bytes start with the gzip magic header (`1f 8b`), i.e. they
/// were produced by [`compress`]. Useful for probing blobs whose compression
/// state was not recorded in metadata.
pub fn is_gzip(data: &[u8]) -> bool {
    data.len() >= 2 && data[0] == 0x1F && data[1] == 0x8B
}

pub fn maybe_compress(data: &[u8]) -> Result<(Vec<u8>, bool), StorageError> {
    if data.len() < 1024 {
        return Ok((data.to_vec(), false));
    }
    let compressed = compress(data)?;
    if compressed.len() < data.len() {
        Ok((compressed, true))
    } else {
        Ok((data.to_vec(), false))
    }
}

pub fn maybe_decompress(data: &[u8], compressed: bool) -> Result<Vec<u8>, StorageError> {
    if compressed {
        decompress(data)
    } else {
        Ok(data.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compress_decompress_roundtrip() {
        let payload = b"hello compression world".repeat(64);
        let compressed = compress(&payload).unwrap();
        assert!(is_gzip(&compressed));
        assert_eq!(decompress(&compressed).unwrap(), payload);
    }

    #[test]
    fn gzip_magic_matches_checkpoint_serializer_format() {
        // The checkpoint serializer emits the same gzip container; entity
        // stores must interoperate with the magic-byte probe.
        let payload = vec![0xAB; 2048];
        let compressed = compress(&payload).unwrap();
        assert_eq!(&compressed[..2], &[0x1F, 0x8B]);
    }

    #[test]
    fn maybe_compress_skips_small_payloads() {
        let small = b"tiny".to_vec();
        let (out, was) = maybe_compress(&small).unwrap();
        assert!(!was);
        assert_eq!(out, small);
    }

    #[test]
    fn maybe_compress_compresses_when_worthwhile() {
        let payload = vec![0u8; 4096];
        let (out, was) = maybe_compress(&payload).unwrap();
        assert!(was);
        assert_eq!(maybe_decompress(&out, was).unwrap(), payload);
    }

    #[test]
    fn maybe_decompress_plain_when_flag_false() {
        let data = vec![1, 2, 3];
        assert_eq!(maybe_decompress(&data, false).unwrap(), data);
    }
}
