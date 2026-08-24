//! Base64 serde adapters for binary blob fields, keeping JSON output
//! compact and byte-exact (raw records, OLE streams, embedded models).

use std::collections::BTreeMap;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serializer};

pub(crate) fn encode(data: &[u8]) -> String {
    STANDARD.encode(data)
}

pub(crate) fn decode(s: &str) -> Option<Vec<u8>> {
    STANDARD.decode(s).ok()
}

pub mod b64 {
    use super::*;

    pub fn serialize<S: Serializer>(v: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&encode(v))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let s = String::deserialize(d)?;
        decode(&s).ok_or_else(|| D::Error::custom("invalid base64"))
    }
}

pub mod b64_opt {
    use super::*;

    pub fn serialize<S: Serializer>(v: &Option<Vec<u8>>, s: S) -> Result<S::Ok, S::Error> {
        match v {
            Some(data) => s.serialize_some(&encode(data)),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Vec<u8>>, D::Error> {
        let s = Option::<String>::deserialize(d)?;
        s.map(|s| decode(&s).ok_or_else(|| D::Error::custom("invalid base64")))
            .transpose()
    }
}

pub mod b64_map {
    use super::*;
    use serde::ser::SerializeMap;

    pub fn serialize<S: Serializer>(
        v: &BTreeMap<String, Vec<u8>>,
        s: S,
    ) -> Result<S::Ok, S::Error> {
        let mut map = s.serialize_map(Some(v.len()))?;
        for (k, data) in v {
            map.serialize_entry(k, &encode(data))?;
        }
        map.end()
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        d: D,
    ) -> Result<BTreeMap<String, Vec<u8>>, D::Error> {
        let raw = BTreeMap::<String, String>::deserialize(d)?;
        raw.into_iter()
            .map(|(k, s)| {
                decode(&s)
                    .map(|v| (k, v))
                    .ok_or_else(|| D::Error::custom("invalid base64"))
            })
            .collect()
    }
}

pub mod b64_list {
    use super::*;

    pub fn serialize<S: Serializer>(v: &[Vec<u8>], s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(v.iter().map(|d| encode(d)))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<Vec<u8>>, D::Error> {
        let raw = Vec::<String>::deserialize(d)?;
        raw.into_iter()
            .map(|s| decode(&s).ok_or_else(|| D::Error::custom("invalid base64")))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode_smoke() {
        let data: Vec<u8> = (0..255u8).collect();
        assert_eq!(decode(&encode(&data)).as_deref(), Some(data.as_slice()));
        assert!(decode("not base64!!").is_none());
    }
}
