use super::{v1::ProtocolV1Config, Protocols};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolConfig {
    pub enabled: Vec<Protocols>,
    pub v1: ProtocolV1Config,
}

impl Default for ProtocolConfig {
    fn default() -> Self {
        Self {
            enabled: vec![Protocols::V1],
            v1: ProtocolV1Config::default(),
        }
    }
}
