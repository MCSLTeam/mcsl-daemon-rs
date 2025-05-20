use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashMap;
use std::sync::LazyLock;

#[allow(clippy::upper_case_acronyms)]
#[derive(Clone, Debug, PartialEq, Eq, Hash, Default)]
pub enum Encoding {
    ASCII,
    #[default]
    UTF8,
    UTF16LE,
    UTF16BE,
    GBK,
    GB18030,
    HZ,
    BIG5_2003,
}

fn map_encoding(encoding: &Encoding) -> encoding::EncodingRef {
    match encoding {
        Encoding::ASCII => encoding::all::ASCII,
        Encoding::UTF8 => encoding::all::UTF_8,
        Encoding::UTF16LE => encoding::all::UTF_16LE,
        Encoding::UTF16BE => encoding::all::UTF_16BE,
        Encoding::GBK => encoding::all::GBK,
        Encoding::GB18030 => encoding::all::GB18030,
        Encoding::HZ => encoding::all::HZ,
        Encoding::BIG5_2003 => encoding::all::BIG5_2003,
    }
}

static STR2ENCODING_MAP: LazyLock<HashMap<&'static str, Encoding>> = LazyLock::new(|| {
    let mut map = HashMap::new();
    map.insert("ascii", Encoding::ASCII);
    map.insert("utf-8", Encoding::UTF8);
    map.insert("utf-16le", Encoding::UTF16LE);
    map.insert("utf-16be", Encoding::UTF16BE);
    map.insert("gbk", Encoding::GBK);
    map.insert("gb18030", Encoding::GB18030);
    map.insert("hz", Encoding::HZ);
    map.insert("big5-2003", Encoding::BIG5_2003);
    map
});

impl Encoding {
    pub fn get(&self) -> encoding::EncodingRef {
        map_encoding(self)
    }
}

// 自定义序列化
impl Serialize for Encoding {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let encoding_name = self.get().name();
        serializer.serialize_str(encoding_name)
    }
}

// 自定义反序列化
impl<'de> Deserialize<'de> for Encoding {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let encoding_name = String::deserialize(deserializer)?;
        STR2ENCODING_MAP
            .get(encoding_name.as_str())
            .cloned()
            .ok_or_else(|| serde::de::Error::custom(format!("Unknown encoding: {}", encoding_name)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn get_encodings() -> Vec<Encoding> {
        Vec::from_iter(STR2ENCODING_MAP.values().cloned())
    }

    #[test]
    fn str_to_encoding_map_test() {
        for encoding in get_encodings() {
            let encoding_name = encoding.get().name();
            assert_eq!(&encoding, STR2ENCODING_MAP.get(encoding_name).unwrap());
        }
    }

    #[test]
    fn encoding_serialize_test() {
        for encoding in get_encodings() {
            let encoding_name = encoding.get().name();
            let serialized = serde_json::to_string(&encoding).unwrap();
            assert_eq!(serialized, format!("\"{}\"", encoding_name));
        }
    }

    #[test]
    fn encoding_deserialize_test() {
        for encoding in get_encodings() {
            let encoding_name = encoding.get().name();
            let serialized = serde_json::to_string(&encoding).unwrap();
            let deserialized: Encoding =
                serde_json::from_str(format!("\"{}\"", encoding_name).as_str()).unwrap();
            assert_eq!(deserialized, encoding);

            let deserialized: Encoding = serde_json::from_str(&serialized).unwrap();
            assert_eq!(deserialized, encoding);
        }
    }
}
