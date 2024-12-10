use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolV1Config {
    pub max_parallel_requests: u16,
    pub file_download_sessions: u8,
}

impl Default for ProtocolV1Config {
    fn default() -> Self {
        Self {
            max_parallel_requests: 256,
            file_download_sessions: 3,
        }
    }
}
