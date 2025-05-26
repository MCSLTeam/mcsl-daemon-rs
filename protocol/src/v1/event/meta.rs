use serde::Serialize;
use uuid::Uuid;

pub trait EventMeta: PartialEq {}

#[derive(Debug, Serialize, PartialEq)]
pub struct InstanceLogEventMeta {
    instance_id: Uuid,
}
impl EventMeta for InstanceLogEventMeta {}
