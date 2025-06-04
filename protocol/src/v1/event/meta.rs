use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "event", content = "meta", rename_all = "snake_case")]
pub enum EventMeta {
    InstanceLog { instance_id: Uuid },
    DaemonReport,
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn event_meta_serialization() {
        let meta = EventMeta::InstanceLog {
            instance_id: Uuid::parse_str("fc4b983a-1238-42b5-a020-cebebb5a6dfe").unwrap(),
        };
        let deserialized: EventMeta = serde_json::from_str(
            r#"{
            "event": "instance_log",
            "meta": {
                "instance_id": "fc4b983a-1238-42b5-a020-cebebb5a6dfe"
            }
        }"#,
        )
        .unwrap();
        assert_eq!(meta, deserialized);
    }

    #[test]
    fn empty_event_meta_serialization() {
        let meta = EventMeta::DaemonReport;
        let deserialized: EventMeta = serde_json::from_str(
            r#"{
            "event": "daemon_report",
            "meta": null
        }"#,
        )
        .unwrap();
        assert_eq!(meta, deserialized);
    }
}
