use crate::v1::event::data::EventData;
use crate::v1::event::meta::EventMeta;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct EventPacket {
    #[serde(flatten)]
    pub meta: EventMeta,
    pub data: EventData,
    pub time: u64,
}
