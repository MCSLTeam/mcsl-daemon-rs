use crate::management::instance::config::InstanceConfig;
use crate::management::instance::performance::InstanceProcessMetrics;
use crate::management::instance::status::InstanceStatus;
use crate::management::minecraft::PlayerSample;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize)]
pub struct InstanceReport {
    pub status: InstanceStatus,
    pub config: InstanceConfig,
    pub properties: HashMap<String, String>,
    pub player: Vec<PlayerSample>,
    pub performance_counter: InstanceProcessMetrics,
}
