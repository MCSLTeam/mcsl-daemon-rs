use serde::{Deserialize, Serialize};

use super::UniDriverConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapnprotoDriverConfig {
    #[serde(flatten)]
    pub uni_config: UniDriverConfig,
}
impl Default for CapnprotoDriverConfig {
    fn default() -> Self {
        Self {
            uni_config: UniDriverConfig {
                port: 11453,
                ..Default::default()
            },
        }
    }
}

pub struct CapnprotoDriver {}
