use crate::status::DaemonReport;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub enum EventData {
    InstanceLog { log: String },
    DaemonReport { report: DaemonReport },
}
