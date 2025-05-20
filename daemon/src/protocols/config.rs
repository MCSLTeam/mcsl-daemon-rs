use super::{v1::ProtocolV1Config, Protocols};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolConfig {
    pub enabled: Cow<'static, [Protocols]>,
    pub v1: ProtocolV1Config,
}

impl Default for ProtocolConfig {
    fn default() -> Self {
        Self {
            enabled: Cow::Borrowed(&[Protocols::V1]),
            v1: ProtocolV1Config::default(),
        }
    }
}
