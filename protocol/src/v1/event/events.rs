use crate::status::DaemonReport;
use serde::Serialize;
use uuid::Uuid;

pub trait EventMeta: PartialEq {}

#[derive(Debug, Serialize, PartialEq)]
pub struct InstanceLogEventMeta {
    instance_id: Uuid,
}
impl EventMeta for InstanceLogEventMeta {}

#[derive(Debug, Serialize, PartialEq)]
pub struct InstanceLogEventData {
    log: String,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct DaemonReportEventData {
    report: DaemonReport,
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Events {
    InstanceLog {
        meta: InstanceLogEventMeta,
        data: InstanceLogEventData,
    },
    DaemonReport {
        meta: (),
        data: DaemonReportEventData,
    },
}

#[derive(Debug, Serialize, PartialEq)]
pub struct EventPacket {
    #[serde(flatten)]
    event: Events,
    time: u64,
}
