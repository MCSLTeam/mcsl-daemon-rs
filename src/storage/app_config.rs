use serde::{Deserialize, Serialize};

use crate::utils;

use super::file::{Config, FileIoWithBackup};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub port: u16,
    pub secret: String,
}

impl FileIoWithBackup for AppConfig {}

impl Config for AppConfig {
    type ConfigType = AppConfig;
}

impl AppConfig {
    pub fn new() -> AppConfig {
        Self::load_config_or_default("config.json", || AppConfig {
            port: 11451,
            secret: utils::get_random_string(32),
        })
        .unwrap()
    }
}
