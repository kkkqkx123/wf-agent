use crate::error::CheckpointError;
use serde::{de::DeserializeOwned, Serialize};

const BINCODE_MAGIC: u8 = 0xBC;

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
        match codec {
            CheckpointCodec::Bincode => {
                let data = bincode::serialize(value)?;
                let mut result = Vec::with_capacity(data.len() + 1);
                result.push(BINCODE_MAGIC);
                result.extend_from_slice(&data);
                Ok(result)
            }
            CheckpointCodec::Json => Ok(serde_json::to_vec(value)?),
        }
    }

    pub fn deserialize<T: DeserializeOwned>(
        data: &[u8],
        codec: CheckpointCodec,
    ) -> Result<T, CheckpointError> {
        match codec {
            CheckpointCodec::Bincode => {
                let data = if data.first() == Some(&BINCODE_MAGIC) {
                    &data[1..]
                } else {
                    data
                };
                Ok(bincode::deserialize(data)?)
            }
            CheckpointCodec::Json => Ok(serde_json::from_slice(data)?),
        }
    }

    pub fn auto_deserialize<T: DeserializeOwned>(data: &[u8]) -> Result<T, CheckpointError> {
        if data.first() == Some(&BINCODE_MAGIC) {
            Self::deserialize(data, CheckpointCodec::Bincode)
        } else {
            Self::deserialize(data, CheckpointCodec::Json)
        }
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

    #[test]
    fn bincode_roundtrip() {
        let original = TestData {
            id: "test".to_string(),
            value: 42,
        };
        let bytes = CheckpointSerializer::serialize(&original, CheckpointCodec::Bincode).unwrap();
        assert_eq!(bytes[0], BINCODE_MAGIC);

        let restored: TestData =
            CheckpointSerializer::deserialize(&bytes, CheckpointCodec::Bincode).unwrap();
        assert_eq!(original, restored);
    }

    #[test]
    fn json_roundtrip() {
        let original = TestData {
            id: "test".to_string(),
            value: 42,
        };
        let bytes = CheckpointSerializer::serialize(&original, CheckpointCodec::Json).unwrap();
        let restored: TestData =
            CheckpointSerializer::deserialize(&bytes, CheckpointCodec::Json).unwrap();
        assert_eq!(original, restored);
    }

    #[test]
    fn auto_detect_bincode() {
        let original = TestData {
            id: "auto".to_string(),
            value: 100,
        };
        let bytes = CheckpointSerializer::serialize(&original, CheckpointCodec::Bincode).unwrap();
        let restored: TestData = CheckpointSerializer::auto_deserialize(&bytes).unwrap();
        assert_eq!(original, restored);
    }

    #[test]
    fn auto_detect_json() {
        let original = TestData {
            id: "json".to_string(),
            value: 200,
        };
        let bytes = CheckpointSerializer::serialize(&original, CheckpointCodec::Json).unwrap();
        let restored: TestData = CheckpointSerializer::auto_deserialize(&bytes).unwrap();
        assert_eq!(original, restored);
    }

    #[test]
    fn bincode_read_compat_without_magic() {
        let original = TestData {
            id: "compat".to_string(),
            value: 300,
        };
        let bytes = bincode::serialize(&original).unwrap();
        let restored: TestData =
            CheckpointSerializer::deserialize(&bytes, CheckpointCodec::Bincode).unwrap();
        assert_eq!(original, restored);
    }
}
