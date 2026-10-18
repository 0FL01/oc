//! Pure binary-field encoding for bounded native DTO transport, never logging.
use base64::Engine as _;
use serde::{Deserialize, Deserializer, Serializer};

pub(crate) fn serialize<S: Serializer>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&base64::engine::general_purpose::STANDARD.encode(bytes))
}

pub(crate) fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
    let encoded = String::deserialize(deserializer)?;
    base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(serde::de::Error::custom)
}

pub(crate) mod png {
    use super::*;
    use std::sync::Arc;

    pub(crate) fn serialize<S: Serializer>(
        bytes: &Arc<[u8]>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        super::serialize(bytes, serializer)
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Arc<[u8]>, D::Error> {
        let encoded = String::deserialize(deserializer)?;
        if encoded.len() > crate::prompt::IMAGE_BYTES.div_ceil(3) * 4 {
            return Err(serde::de::Error::custom("image byte limit"));
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(serde::de::Error::custom)?;
        if bytes.is_empty() || bytes.len() > crate::prompt::IMAGE_BYTES {
            return Err(serde::de::Error::custom("image byte limit"));
        }
        Ok(bytes.into())
    }
}
