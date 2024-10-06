use serde::{Deserialize, Serialize};

use super::file::{Config, FileIoWithBackup};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub port: u16,
}

impl FileIoWithBackup for AppConfig {}

impl Config for AppConfig {
    type ConfigType = AppConfig;
}

impl AppConfig {
    pub fn new() -> AppConfig {
        Self::load_config_or_default("config.json", || AppConfig { port: 11452 }).unwrap()
    }
}
