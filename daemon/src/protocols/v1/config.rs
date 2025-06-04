use serde::{Deserialize, Serialize};
use sysinfo::System;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolV1Config {
    pub max_parallel_requests: u16,
    pub max_pending_requests: u16,
    pub file_download_sessions: u8,
}

impl Default for ProtocolV1Config {
    fn default() -> Self {
        let cpu_count = System::new_all().cpus().len() as u16;
        Self {
            max_parallel_requests: cpu_count,
            max_pending_requests: cpu_count,
            file_download_sessions: 3,
        }
    }
}
