use crate::error::CheckpointError;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression as GzCompression;
use serde::{de::DeserializeOwned, Serialize};
use std::io::{Read, Write};
use wf_types::checkpoint::CompressionStrategy;

const BINCODE_MAGIC: u8 = 0xBC;
const GZIP_MAGIC: [u8; 2] = [0x1F, 0x8B];
/// Compression threshold aligned with the TS state codec (compresses blobs
/// larger than 512 bytes).
pub const COMPRESSION_THRESHOLD: usize = 512;

pub enum CheckpointCodec {
    Bincode,
    Json,
}

pub struct CheckpointSerializer;

impl CheckpointSerializer {
    pub fn serialize<T: Serialize>(
        value: &T,
        codec: CheckpointCodec,
    ) -> Result<Vec<u8>, CheckpointError> {
        Self::serialize_with_compression(value, codec, CompressionStrategy::None)
    }

    /// Serialize with an optional compression strategy. `Auto` compresses
    /// only when the encoded payload exceeds `COMPRESSION_THRESHOLD` bytes,
    /// matching the TS `state-codec` behavior.
    pub fn serialize_with_compression<T: Serialize>(
        value: &T,
        codec: CheckpointCodec,
        compression: CompressionStrategy,
    ) -> Result<Vec<u8>, CheckpointError> {
        let data = match codec {
            CheckpointCodec::Bincode => {
                let data = bincode::serialize(value)?;
                let mut result = Vec::with_capacity(data.len() + 1);
                result.push(BINCODE_MAGIC);
                result.extend_from_slice(&data);
                result
            }
            CheckpointCodec::Json => serde_json::to_vec(value)?,
        };

        let compress = match compression {
            CompressionStrategy::None => false,
            CompressionStrategy::Gzip => true,
            CompressionStrategy::Auto => data.len() > COMPRESSION_THRESHOLD,
        };

        if compress {
            compress_gzip(&data)
        } else {
            Ok(data)
        }
    }

    pub fn deserialize<T: DeserializeOwned>(
        data: &[u8],
        codec: CheckpointCodec,
    ) -> Result<T, CheckpointError> {
        let data = decompress_if_gzip(data)?;
        match codec {
            CheckpointCodec::Bincode => {
                let data = if data.first() == Some(&BINCODE_MAGIC) {
                    &data[1..]
                } else {
                    &data
                };
                Ok(bincode::deserialize(data)?)
            }
            CheckpointCodec::Json => Ok(serde_json::from_slice(&data)?),
        }
    }

    pub fn auto_deserialize<T: DeserializeOwned>(data: &[u8]) -> Result<T, CheckpointError> {
        let data = decompress_if_gzip(data)?;
        if data.first() == Some(&BINCODE_MAGIC) {
            Self::deserialize(&data, CheckpointCodec::Bincode)
        } else {
            Self::deserialize(&data, CheckpointCodec::Json)
        }
    }
}

fn compress_gzip(data: &[u8]) -> Result<Vec<u8>, CheckpointError> {
    let mut encoder = GzEncoder::new(Vec::new(), GzCompression::default());
    encoder
        .write_all(data)
        .map_err(|e| CheckpointError::Serialization(format!("gzip compress failed: {e}")))?;
    encoder
        .finish()
        .map_err(|e| CheckpointError::Serialization(format!("gzip compress failed: {e}")))
}

fn decompress_if_gzip(data: &[u8]) -> Result<Vec<u8>, CheckpointError> {
    if data.len() >= 2 && data[0] == GZIP_MAGIC[0] && data[1] == GZIP_MAGIC[1] {
        let mut decoder = GzDecoder::new(data);
        let mut out = Vec::new();
        decoder
            .read_to_end(&mut out)
            .map_err(|e| CheckpointError::Serialization(format!("gzip decompress failed: {e}")))?;
        Ok(out)
    } else {
        Ok(data.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct TestData {
        id: String,
        value: u64,
    }

    fn payload() -> TestData {
        TestData {
            id: "test".to_string(),
            value: 42,
        }
    }

    #[test]
    fn bincode_roundtrip() {
        let original = payload();
        let bytes = CheckpointSerializer::serialize(&original, CheckpointCodec::Bincode).unwrap();
        assert_eq!(bytes[0], BINCODE_MAGIC);

        let restored: TestData =
            CheckpointSerializer::deserialize(&bytes, CheckpointCodec::Bincode).unwrap();
        assert_eq!(original, restored);
    }

    #[test]
    fn json_roundtrip() {
        let original = payload();
        let bytes = CheckpointSerializer::serialize(&original, CheckpointCodec::Json).unwrap();
        let restored: TestData =
            CheckpointSerializer::deserialize(&bytes, CheckpointCodec::Json).unwrap();
        assert_eq!(original, restored);
    }

    #[test]
    fn auto_detect_bincode() {
        let original = payload();
        let bytes = CheckpointSerializer::serialize(&original, CheckpointCodec::Bincode).unwrap();
        let restored: TestData = CheckpointSerializer::auto_deserialize(&bytes).unwrap();
        assert_eq!(original, restored);
    }

    #[test]
    fn auto_detect_json() {
        let original = payload();
        let bytes = CheckpointSerializer::serialize(&original, CheckpointCodec::Json).unwrap();
        let restored: TestData = CheckpointSerializer::auto_deserialize(&bytes).unwrap();
        assert_eq!(original, restored);
    }

    #[test]
    fn bincode_read_compat_without_magic() {
        let original = payload();
        let bytes = bincode::serialize(&original).unwrap();
        let restored: TestData =
            CheckpointSerializer::deserialize(&bytes, CheckpointCodec::Bincode).unwrap();
        assert_eq!(original, restored);
    }

    #[test]
    fn gzip_roundtrip() {
        let original = payload();
        let bytes = CheckpointSerializer::serialize_with_compression(
            &original,
            CheckpointCodec::Json,
            CompressionStrategy::Gzip,
        )
        .unwrap();
        assert_eq!(&bytes[..2], &GZIP_MAGIC);

        let restored: TestData = CheckpointSerializer::auto_deserialize(&bytes).unwrap();
        assert_eq!(original, restored);
    }

    #[test]
    fn auto_compresses_large_payloads_only() {
        let large = TestData {
            id: "x".repeat(COMPRESSION_THRESHOLD + 200),
            value: 1,
        };
        let compressed = CheckpointSerializer::serialize_with_compression(
            &large,
            CheckpointCodec::Json,
            CompressionStrategy::Auto,
        )
        .unwrap();
        assert_eq!(&compressed[..2], &GZIP_MAGIC);

        let small = payload();
        let plain = CheckpointSerializer::serialize_with_compression(
            &small,
            CheckpointCodec::Json,
            CompressionStrategy::Auto,
        )
        .unwrap();
        assert_ne!(&plain[..2], &GZIP_MAGIC);
    }

    #[test]
    fn ts_style_camelcase_json_deserializes() {
        use wf_types::checkpoint::{BaseCheckpointCore, CheckpointType};

        let ts_json = r#"{
            "id": "cp-1",
            "type": "FULL",
            "baseCheckpointId": null,
            "previousCheckpointId": "cp-0",
            "timestamp": 1700000000000,
            "metadata": {
                "description": "test",
                "tags": ["auto"],
                "customFields": {"chainPosition": 3, "formatVersion": {"major": 1, "minor": 0}}
            },
            "snapshot": {"executionId": "exec-1", "status": "completed"}
        }"#;

        let envelope: BaseCheckpointCore<serde_json::Value, serde_json::Value> =
            serde_json::from_str(ts_json).unwrap();
        assert_eq!(envelope.id, "cp-1");
        assert_eq!(envelope.r#type, Some(CheckpointType::Full));
        assert_eq!(envelope.previous_checkpoint_id.as_deref(), Some("cp-0"));
        assert_eq!(envelope.timestamp, Some(1700000000000));
        let metadata = envelope.metadata.unwrap();
        assert_eq!(
            metadata.get("description").and_then(|v| v.as_str()),
            Some("test")
        );
        assert_eq!(
            metadata.get("tags").and_then(|v| v.as_array()),
            Some(&serde_json::json!(["auto"]).as_array().unwrap().clone())
        );
        assert!(metadata.get("customFields").is_some());
    }

    #[test]
    fn legacy_snake_case_json_still_deserializes() {
        use wf_types::checkpoint::BaseCheckpointCore;

        let legacy_json = r#"{
            "id": "cp-legacy",
            "type": "delta",
            "base_checkpoint_id": "cp-full",
            "previous_checkpoint_id": "cp-full",
            "timestamp": 1700000000000,
            "delta": {"added_variables": {"a": 1}}
        }"#;

        let envelope: BaseCheckpointCore<serde_json::Value, serde_json::Value> =
            serde_json::from_str(legacy_json).unwrap();
        assert_eq!(
            envelope.r#type,
            Some(wf_types::checkpoint::CheckpointType::Delta)
        );
        assert_eq!(envelope.base_checkpoint_id.as_deref(), Some("cp-full"));
        assert_eq!(envelope.previous_checkpoint_id.as_deref(), Some("cp-full"));
    }

    #[test]
    fn serializes_to_ts_compatible_uppercase_type_and_camelcase() {
        use wf_types::checkpoint::BaseCheckpointCore;

        let envelope: BaseCheckpointCore<serde_json::Value, serde_json::Value> =
            BaseCheckpointCore {
                id: "cp-1".to_string(),
                r#type: Some(wf_types::checkpoint::CheckpointType::Full),
                base_checkpoint_id: None,
                previous_checkpoint_id: Some("cp-0".to_string()),
                delta: None,
                snapshot: Some(serde_json::json!({"executionId": "e1"})),
                timestamp: Some(1700000000000),
                metadata: Some(std::collections::HashMap::from([(
                    "description".to_string(),
                    serde_json::json!("d"),
                )])),
                format_version: Some("1.0".to_string()),
            };

        let json = CheckpointSerializer::serialize(&envelope, CheckpointCodec::Json).unwrap();
        let text = String::from_utf8(json).unwrap();
        assert!(text.contains("\"type\":\"FULL\""));
        assert!(text.contains("\"previousCheckpointId\":\"cp-0\""));
        assert!(text.contains("\"formatVersion\":\"1.0\""));
    }

    #[test]
    fn field_change_round_trips_ts_shape() {
        use wf_types::checkpoint::FieldChange;

        let change = FieldChange {
            from: Some("running".to_string()),
            to: Some("completed".to_string()),
        };
        let json = serde_json::to_string(&change).unwrap();
        assert_eq!(json, r#"{"from":"running","to":"completed"}"#);
        let back: FieldChange = serde_json::from_str(&json).unwrap();
        assert_eq!(back, change);
    }
}
