use crate::status::system_info::SysInfo;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct DaemonReport {
    #[serde(flatten)]
    pub sys_info: SysInfo,
    pub start_time_stamp: u64,
}
