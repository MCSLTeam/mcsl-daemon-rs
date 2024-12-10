use super::super::UniDriverConfig;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WsDriverConfig {
    #[serde(flatten)]
    pub uni_config: UniDriverConfig,
}
