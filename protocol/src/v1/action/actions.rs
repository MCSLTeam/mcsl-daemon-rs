use crate::files::java_info::JavaInfo;
use crate::v1::action::retcode::Retcode;
use crate::v1::action::status::ActionStatus;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Deserialize, PartialEq, Clone)]
#[serde(tag = "action", content = "params", rename_all = "snake_case")]
#[serde(bound(deserialize = "'de: 'req"))]
pub enum ActionParameters<'req> {
    // event subsystem
    SubscribeEvent {},
    UnsubscribeEvent {},

    // misc
    Ping {},
    GetSystemInfo {},
    GetPermissions {},
    GetJavaList {},
    GetDirectoryInfo {
        path: &'req str,
    },
    GetFileInfo {
        path: &'req str,
    },

    // file down/up-load
    FileUploadRequest {
        path: Option<&'req str>,
        sha1: Option<&'req str>,
        chunk_size: u64,
        size: u64,
    },
    FileUploadChunk {
        file_id: Uuid,
        offset: u64,
        data: &'req str,
    },
    FileUploadChunkRaw {
        file_id: Uuid,
        offset: u64,
        #[serde(skip)]
        data: Option<&'req [u8]>,
    },
    FileUploadCancel {
        file_id: Uuid,
    },
    FileDownloadRequest {
        path: &'req str,
    },
    FileDownloadRange {
        file_id: Uuid,
        range: &'req str,
    },
    FileDownloadClose {
        file_id: Uuid,
    },

    // instance operation
    AddInstance {},
    RemoveInstance {},
    StartInstance {},
    StopInstance {},
    KillInstance {},
    SendToInstance {},
    GetInstanceReport {},
    GetAllReports {},
}

#[derive(Debug, Serialize, PartialEq)]
#[serde(untagged)]
pub enum ActionResults {
    ActionError,

    // event subsystem
    SubscribeEvent {},
    UnsubscribeEvent {},

    // misc
    Ping {
        time: u64,
    },
    GetSystemInfo {},
    GetPermissions {},
    GetJavaList {
        java_list: Vec<JavaInfo>,
    },
    GetDirectoryInfo {},
    GetFileInfo {},

    // file down/up-load
    FileUploadRequest {
        file_id: Uuid,
    },
    FileUploadChunk {
        done: bool,
        received: u64,
    },
    FileUploadCancel {},
    FileDownloadRequest {
        file_id: Uuid,
        size: u64,
        sha1: String,
    },
    FileDownloadRange {
        content: String,
    },
    FileDownloadClose {},

    // instance operation
    AddInstance {},
    RemoveInstance {},
    StartInstance {},
    StopInstance {},
    KillInstance {},
    SendToInstance {},
    GetInstanceReport {},
    GetAllReports {},
}

#[derive(Debug, Deserialize, PartialEq, Clone)]
#[serde(bound(deserialize = "'de: 'req"))]
pub struct ActionRequest<'req> {
    #[serde(flatten)]
    pub parameters: ActionParameters<'req>, // flattened
    pub id: Uuid,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct ActionResponse {
    pub status: ActionStatus,
    pub data: ActionResults,
    #[serde(flatten)]
    pub retcode: Retcode,
    pub id: Uuid,
}

#[cfg(test)]
mod tests {
    use crate::v1::action::actions::{ActionParameters, ActionRequest};
    use pretty_assertions::assert_eq;
    use uuid::Uuid;

    #[test]
    fn serialize_action() {
        let raw = r#"{
                "action": "file_download_request",
                "params": {
                    "path": "daemon1/downloads/sample.jar"
                },
                "id": "a1829c2d-4357-4aef-8a95-544515243faf"
            }"#;

        let path = String::from("daemon1/downloads/sample.jar");
        let expected = ActionRequest {
            parameters: ActionParameters::FileDownloadRequest { path: &path },
            id: Uuid::parse_str("a1829c2d-4357-4aef-8a95-544515243faf").unwrap(),
        };
        assert_eq!(
            serde_json::from_str::<ActionRequest>(raw).unwrap(),
            expected
        );
    }
}
