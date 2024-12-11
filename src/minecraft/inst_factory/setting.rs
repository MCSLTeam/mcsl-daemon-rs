use super::super::inst_config::InstConfig;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct InstFactorySetting {
    pub source: String,
    pub source_type: SourceType,
    pub use_post_process: bool,

    #[serde(flatten)]
    pub inner: InstConfig,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceType {
    Archive,
    Core,
    Script,
}
