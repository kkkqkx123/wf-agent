use std::io::{Read, Write};

use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use flate2::Compression;

use crate::error::StorageError;

pub fn compress(data: &[u8]) -> Result<Vec<u8>, StorageError> {
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
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
    let mut decoder = ZlibDecoder::new(data);
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
