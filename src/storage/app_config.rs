use serde::{Deserialize, Serialize};

use super::file::{Config, FileIoWithBackup};

#[derive(Debug, Clone, Serialize, Deserialize)]
/// immutable through full lifetime of app, unless restart app.
pub struct AppConfig {
    pub port: u16,
    pub file_download_sessions: u8,
}

impl FileIoWithBackup for AppConfig {}

impl Config for AppConfig {
    type ConfigType = AppConfig;
}

impl AppConfig {
    pub fn new() -> AppConfig {
        Self::load_config_or_default("config.json", Self::default).unwrap()
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        AppConfig {
            port: 11452,
            file_download_sessions: 3,
        }
    }
}
