use crate::status::DaemonReport;
use crate::v1::event::meta::EventMeta;
use serde::Serialize;

pub trait EventData {}

#[derive(Debug, Serialize, PartialEq)]
pub struct InstanceLogEventData {
    log: String,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct DaemonReportEventData {
    report: DaemonReport,
}

impl EventData for DaemonReportEventData {}
impl EventMeta for InstanceLogEventData {}
