use crate::management::instance::config::{InstanceConfig, SourceType};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum InstanceFactoryMirror {
    #[default]
    None,
    BmclApi,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InstanceFactorySetting {
    source: String,
    source_type: SourceType,
    #[serde(default = "InstanceFactoryMirror::default")]
    mirror: InstanceFactoryMirror,
    
    #[serde(flatten)]
    config: InstanceConfig,
}
