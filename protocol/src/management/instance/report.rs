use crate::management::instance::config::InstanceConfig;
use crate::management::instance::performance::InstancePerformanceCounter;
use crate::management::instance::status::InstanceStatus;
use crate::management::minecraft::Player;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize)]
pub struct InstanceReport {
    status: InstanceStatus,
    config: InstanceConfig,
    properties: HashMap<String, String>,
    player: Vec<Player>,
    performance_counter: InstancePerformanceCounter,
}
