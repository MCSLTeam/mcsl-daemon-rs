use super::config::InstConfig;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InstProcessStatus {
    Starting,
    Running,
    Stopping,
    Stopped,
    Crashed,
}

pub struct InstStatus<'a> {
    status: InstProcessStatus,
    config: InstConfig,
    properties: &'a [&'a str],
    players: &'a [&'a str],
}
