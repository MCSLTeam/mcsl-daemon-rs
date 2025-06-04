use super::super::Protocol;
use regex::Regex;
use std::sync::LazyLock;

use crate::storage::java::java_scan;
use crate::storage::Files;
use anyhow::{bail, Context};
use mcsl_protocol::v1::action::retcode::Retcode;
use mcsl_protocol::v1::action::status::ActionStatus;
use mcsl_protocol::v1::action::{
    retcode, ActionParameters, ActionRequest, ActionResponse, ActionResults,
};
use uuid::Uuid;

pub static RANGE_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(\d+)..(\d+)$").unwrap());
pub struct ProtocolV1 {
    files: Files,
}

impl Protocol for ProtocolV1 {
    fn process_text_request<'req>(
        &self,
        raw: &'req str,
    ) -> Result<ActionRequest<'req>, ActionResponse> {
        serde_json::from_str::<ActionRequest>(raw).map_err(move |err| {
            log::error!("action error: {}", err);
            Self::err(retcode::BAD_REQUEST.clone(), Uuid::nil())
        })
    }

    fn process_bin_request<'req>(
        &self,
        raw: &'req [u8],
    ) -> Result<ActionRequest<'req>, ActionResponse> {
        todo!()
    }

    async fn process_text(&self, raw: &str) -> Option<String> {
        Some(serde_json::to_string_pretty(&self.process(raw).await).unwrap())
    }

    async fn process_binary(&self, _: &[u8]) -> Option<Vec<u8>> {
        None
    }

    fn handle_text_rate_limit_exceed(&self, raw: &str) -> Option<String> {
        let resp = match self.process_text_request(raw) {
            Ok(req) => Self::err(retcode::RATE_LIMIT_EXCEEDED.clone(), req.id),
            Err(resp) => resp,
        };
        Some(serde_json::to_string_pretty(&resp).unwrap())
    }

    fn handle_bin_rate_limit_exceed(&self, raw: &[u8]) -> Option<Vec<u8>> {
        None
    }
}

impl ProtocolV1 {
    #[inline]
    async fn process(&self, raw: &str) -> ActionResponse {
        let request = match self.process_text_request(raw) {
            Ok(request) => request,
            Err(resp) => return resp,
        };

        let response = match request.parameters {
            ActionParameters::Ping {} => Self::ping_handler().await,
            ActionParameters::GetJavaList {} => self.get_java_list_handler().await,
            ActionParameters::FileUploadRequest {
                path,
                sha1,
                chunk_size,
                size,
            } => {
                self.file_upload_request_handler(path, sha1, chunk_size, size)
                    .await
            }
            ActionParameters::FileUploadChunk {
                file_id,
                offset,
                data,
            } => self.file_upload_chunk_handler(file_id, offset, data).await,
            ActionParameters::FileUploadCancel { file_id } => {
                self.file_upload_cancel_handler(file_id).await
            }
            ActionParameters::FileDownloadRequest { path } => {
                self.file_download_request_handler(path).await
            }
            ActionParameters::FileDownloadRange { file_id, range } => {
                self.file_download_range_handler(file_id, range).await
            }
            ActionParameters::FileDownloadClose { file_id } => {
                self.file_download_close_handler(file_id).await
            }
            _ => {
                todo!()
            }
        };

        match response {
            Ok(response) => Self::ok(response, request.id),
            Err(err) => {
                log::error!("action error: {}", err);
                Self::err(
                    retcode::REQUEST_ERROR.with_message(&err.to_string()),
                    Uuid::nil(),
                )
            }
        }
    }

    pub fn err(retcode: Retcode, id: Uuid) -> ActionResponse {
        ActionResponse {
            status: ActionStatus::Error,
            data: ActionResults::ActionError {},
            retcode,
            id,
        }
    }
    fn ok(data: ActionResults, id: Uuid) -> ActionResponse {
        ActionResponse {
            status: ActionStatus::Ok,
            data,
            retcode: retcode::OK.clone(),
            id,
        }
    }
}

impl ProtocolV1 {
    #[inline]
    async fn ping_handler() -> anyhow::Result<ActionResults> {
        Ok(ActionResults::Ping {
            time: chrono::Utc::now().timestamp() as u64,
        })
    }

    #[inline]
    async fn get_java_list_handler(&self) -> anyhow::Result<ActionResults> {
        Ok(ActionResults::GetJavaList {
            java_list: java_scan().await,
        })
    }

    #[inline]
    async fn file_upload_request_handler(
        &self,
        path: Option<&str>,
        sha1: Option<&str>,
        chunk_size: u64,
        size: u64,
    ) -> anyhow::Result<ActionResults> {
        let file_id = self
            .files
            .upload_request(path, size, chunk_size, sha1)
            .await?;
        Ok(ActionResults::FileUploadRequest { file_id })
    }

    #[inline]
    async fn file_upload_chunk_handler(
        &self,
        file_id: Uuid,
        offset: u64,
        data: &str,
    ) -> anyhow::Result<ActionResults> {
        let (done, received) = self.files.upload_chunk(file_id, offset, data).await?;
        Ok(ActionResults::FileUploadChunk { done, received })
    }

    #[inline]
    async fn file_upload_cancel_handler(&self, file_id: Uuid) -> anyhow::Result<ActionResults> {
        if self.files.upload_cancel(file_id).await {
            Ok(ActionResults::FileUploadCancel {})
        } else {
            bail!("session not found")
        }
    }

    #[inline]
    async fn file_download_request_handler(&self, path: &str) -> anyhow::Result<ActionResults> {
        let (file_id, size, sha1) = self.files.download_request(path).await?;
        Ok(ActionResults::FileDownloadRequest {
            file_id,
            size,
            sha1,
        })
    }

    #[inline]
    async fn file_download_range_handler(
        &self,
        file_id: Uuid,
        range: &str,
    ) -> anyhow::Result<ActionResults> {
        let range_match = RANGE_REGEX.captures(range);
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
        Ok(ActionResults::FileDownloadRange { content })
    }

    #[inline]
    async fn file_download_close_handler(&self, file_id: Uuid) -> anyhow::Result<ActionResults> {
        self.files.download_close(file_id).await?;
        Ok(ActionResults::FileDownloadClose {})
    }
}

impl ProtocolV1 {
    pub fn new(files: Files) -> Self {
        Self { files }
    }
}
