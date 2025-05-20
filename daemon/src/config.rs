use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

use crate::auth::AuthConfig;
use crate::storage::file::{Config, FileIoWithBackup};
use crate::{drivers::DriversConfig, protocols::ProtocolConfig};

#[derive(Debug, Clone, Serialize, Deserialize)]
/// immutable through full lifetime of app, unless restart app.
#[derive(Default)]
pub struct AppConfig {
    pub drivers: DriversConfig,
    pub protocols: ProtocolConfig,
    pub auth: AuthConfig,
}

impl FileIoWithBackup for AppConfig {}

impl Config for AppConfig {
    type ConfigType = AppConfig;
}

impl AppConfig {
    fn load() -> AppConfig {
        Self::load_config_or_default("config.json", Self::default).unwrap()
    }
}

static APP_CONFIG: LazyLock<AppConfig> = LazyLock::new(AppConfig::load);

impl AppConfig {
    pub fn get() -> &'static AppConfig {
        &APP_CONFIG
    }
}
