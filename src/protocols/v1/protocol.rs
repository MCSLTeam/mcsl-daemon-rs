use super::super::Protocol;
use super::action::{
    ActionRequests, ActionResponses, Request, Response, ResponseStatus, RANGE_REGEX,
};
use crate::storage::{java::JavaInfo, Files};
use crate::utils::AsyncTimedCache;
use anyhow::{bail, Context};
use std::time::Duration;
use uuid::Uuid;

pub struct ProtocolV1 {
    java_scan_cache: AsyncTimedCache<Vec<JavaInfo>>,
    files: Files,
}

impl Protocol for ProtocolV1 {
    async fn process_text(&self, raw: &str) -> Option<String> {
        Some(serde_json::to_string_pretty(&self.process(raw).await).unwrap())
    }

    async fn process_binary(&self, _: &[u8]) -> Option<Vec<u8>> {
        None
    }
}

impl ProtocolV1 {
    #[inline]
    async fn process(&self, raw: &str) -> Response {
        let parsed = match serde_json::from_str::<Request>(raw) {
            Ok(parsed) => parsed,
            Err(err) => {
                log::error!("action error: {}", err);
                return Self::err(err.to_string(), Self::get_echo(raw));
            }
        };

        let response = match parsed.request {
            ActionRequests::Ping {} => Self::ping_handler().await,
            ActionRequests::GetJavaList {} => self.get_java_list_handler().await,
            ActionRequests::FileUploadRequest {
                path,
                sha1,
                chunk_size,
                size,
            } => {
                self.file_upload_request_handler(path, sha1, chunk_size, size)
                    .await
            }
            ActionRequests::FileUploadChunk {
                file_id,
                offset,
                data,
            } => self.file_upload_chunk_handler(file_id, offset, data).await,
            ActionRequests::FileUploadCancel { file_id } => {
                self.file_upload_cancel_handler(file_id).await
            }
            ActionRequests::FileDownloadRequest { path } => {
                self.file_download_request_handler(path).await
            }
            ActionRequests::FileDownloadRange { file_id, range } => {
                self.file_download_range_handler(file_id, range).await
            }
            ActionRequests::FileDownloadClose { file_id } => {
                self.file_download_close_handler(file_id).await
            }
        };

        let response = match response {
            Ok(response) => response,
            Err(err) => {
                log::error!("action error: {}", err);
                return Self::err(err.to_string(), Self::get_echo(raw));
            }
        };
        Self::ok(response, parsed.echo)
    }

    fn err(msg: String, echo: Option<String>) -> Response {
        Response {
            status: ResponseStatus::Error,
            data: ActionResponses::ActionError { error_message: msg },
            echo,
        }
    }
    fn ok(data: ActionResponses, echo: Option<String>) -> Response {
        Response {
            status: ResponseStatus::Ok,
            data,
            echo,
        }
    }

    fn get_echo(raw: &str) -> Option<String> {
        let parsed: serde_json::Value = serde_json::from_str(raw).ok()?;
        parsed
            .get("echo")
            .and_then(|echo| echo.as_str())
            .map(|echo| echo.to_string())
    }
}

impl ProtocolV1 {
    #[inline]
    async fn ping_handler() -> anyhow::Result<ActionResponses> {
        Ok(ActionResponses::Ping {
            time: chrono::Utc::now().timestamp() as u64,
        })
    }

    #[inline]
    async fn get_java_list_handler(&self) -> anyhow::Result<ActionResponses> {
        Ok(ActionResponses::GetJavaList {
            java_list: self.java_scan_cache.get().await,
        })
    }

    #[inline]
    async fn file_upload_request_handler(
        &self,
        path: Option<String>,
        sha1: Option<String>,
        chunk_size: u64,
        size: u64,
    ) -> anyhow::Result<ActionResponses> {
        let file_id = self
            .files
            .upload_request(path.as_deref(), size, chunk_size, sha1.as_deref())
            .await?;
        Ok(ActionResponses::FileUploadRequest { file_id })
    }

    #[inline]
    async fn file_upload_chunk_handler(
        &self,
        file_id: Uuid,
        offset: u64,
        data: String,
    ) -> anyhow::Result<ActionResponses> {
        let (done, received) = self.files.upload_chunk(file_id, offset, data).await?;
        Ok(ActionResponses::FileUploadChunk { done, received })
    }

    #[inline]
    async fn file_upload_cancel_handler(&self, file_id: Uuid) -> anyhow::Result<ActionResponses> {
        if self.files.upload_cancel(file_id).await {
            Ok(ActionResponses::FileUploadCancel {})
        } else {
            bail!("session not found")
        }
    }

    #[inline]
    async fn file_download_request_handler(&self, path: String) -> anyhow::Result<ActionResponses> {
        let (file_id, size, sha1) = self.files.download_request(&path).await?;
        Ok(ActionResponses::FileDownloadRequest {
            file_id,
            size,
            sha1,
        })
    }

    #[inline]
    async fn file_download_range_handler(
        &self,
        file_id: Uuid,
        range: String,
    ) -> anyhow::Result<ActionResponses> {
        let range_match = RANGE_REGEX.captures(&range);
        if range_match.is_none() {
            bail!("invalid range");
        }
        let range_match = range_match.unwrap();
        let from: u64 = range_match
            .get(1)
            .unwrap()
            .as_str()
            .parse()
            .context("invalid range")?;
        let to: u64 = range_match
            .get(2)
            .unwrap()
            .as_str()
            .parse()
            .context("invalid range")?;

        let content = self.files.download_range(file_id, from, to).await?;
        Ok(ActionResponses::FileDownloadRange { content })
    }

    #[inline]
    async fn file_download_close_handler(&self, file_id: Uuid) -> anyhow::Result<ActionResponses> {
        self.files.download_close(file_id).await?;
        Ok(ActionResponses::FileDownloadClose {})
    }
}

impl ProtocolV1 {
    pub fn new(files: Files) -> Self {
        Self {
            java_scan_cache: AsyncTimedCache::new(Duration::from_secs(60)),
            files,
        }
    }
}

/// test action request deserialize
#[cfg(test)]
mod test_request_deserialize {
    use super::*;

    #[test]
    fn serialize_action() {
        let raw = r#"{
                "action": "file_download_request",
                "params": {
                    "path": "daemon/downloads/sample.jar"
                }
            }"#;
        let expected = Request {
            request: ActionRequests::FileDownloadRequest {
                path: "daemon/downloads/sample.jar".to_string(),
            },
            echo: None,
        };
        assert_eq!(serde_json::from_str::<Request>(raw).unwrap(), expected);
    }

    #[test]
    fn serialize_action_with_echo() {
        let raw = r#"{
                "action": "file_download_request",
                "params": {
                    "path": "daemon/downloads/sample.jar"
                },
                "echo": "114514"
            }"#;
        let expected = Request {
            request: ActionRequests::FileDownloadRequest {
                path: "daemon/downloads/sample.jar".to_string(),
            },
            echo: Some("114514".to_string()),
        };
        assert_eq!(serde_json::from_str::<Request>(raw).unwrap(), expected);
    }

    #[test]
    fn serialize_empty_action() {
        let raw = r#"{
                "action": "ping",
                "params": {},
                "echo": "114514"
            }"#;
        let expected = Request {
            request: ActionRequests::Ping {},
            echo: Some("114514".to_string()),
        };
        assert_eq!(serde_json::from_str::<Request>(raw).unwrap(), expected);
    }
}

/// test action response serialize
#[cfg(test)]
mod test_response_serialize {
    use super::*;

    #[test]
    fn deserialize_action_response() {
        let raw = r#"{
  "status": "ok",
  "data": {
    "file_id": "e7a0c2a1-d0e8-4b0a-a2e5-c0d4e6f7b8c9",
    "size": 1024,
    "sha1": "balabala"
  },
  "echo": "114514"
}"#;
        let expected = Response {
            data: ActionResponses::FileDownloadRequest {
                file_id: Uuid::parse_str("e7a0c2a1-d0e8-4b0a-a2e5-c0d4e6f7b8c9").unwrap(),
                size: 1024,
                sha1: "balabala".to_string(),
            },
            status: ResponseStatus::Ok,
            echo: Some("114514".to_string()),
        };
        assert_eq!(serde_json::to_string_pretty(&expected).unwrap(), raw);
    }

    #[test]
    fn deserialize_action_response_with_no_echo() {
        let raw = r#"{
  "status": "ok",
  "data": {
    "file_id": "e7a0c2a1-d0e8-4b0a-a2e5-c0d4e6f7b8c9",
    "size": 1024,
    "sha1": "balabala"
  }
}"#;
        let expected = Response {
            data: ActionResponses::FileDownloadRequest {
                file_id: Uuid::parse_str("e7a0c2a1-d0e8-4b0a-a2e5-c0d4e6f7b8c9").unwrap(),
                size: 1024,
                sha1: "balabala".to_string(),
            },
            status: ResponseStatus::Ok,
            echo: None,
        };
        assert_eq!(serde_json::to_string_pretty(&expected).unwrap(), raw);
    }

    #[test]
    fn deserialize_action_response_error() {
        let raw = r#"{
  "status": "error",
  "data": {
    "error_message": "error message"
  },
  "echo": "114514"
}"#;
        let expected = Response {
            data: ActionResponses::ActionError {
                error_message: "error message".to_string(),
            },
            status: ResponseStatus::Error,
            echo: Some("114514".to_string()),
        };
        assert_eq!(serde_json::to_string_pretty(&expected).unwrap(), raw);
    }
}
