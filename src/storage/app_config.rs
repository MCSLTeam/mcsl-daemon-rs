use serde::{Deserialize, Serialize};

use crate::remote::drivers::DriversConfig;

use super::file::{Config, FileIoWithBackup};

#[derive(Debug, Clone, Serialize, Deserialize)]
/// immutable through full lifetime of app, unless restart app.
#[derive(Default)]
pub struct AppConfig {
    pub drivers: DriversConfig,
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
