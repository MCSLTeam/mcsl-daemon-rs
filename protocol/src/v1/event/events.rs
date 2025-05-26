use crate::v1::event::data::{DaemonReportEventData, InstanceLogEventData};
use crate::v1::event::meta::InstanceLogEventMeta;
use serde::Serialize;

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

impl EventPacket {
    pub fn new(event: Events, time: u64) -> Self {
        Self { event, time }
    }
}
