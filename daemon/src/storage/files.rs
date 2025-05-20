use crate::protocols::ProtocolConfig;
use std::fs;
use std::io::Read;

use crate::storage::file::{FileDownloadInfo, FileUploadInfo};
use anyhow::{anyhow, bail, Context};
use log::debug;
use sha1::{Digest, Sha1};
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt, SeekFrom};

use scc::HashMap;
use uuid::Uuid;

pub const ROOT: &str = "daemon";
pub const DOWNLOAD_ROOT: &str = "daemon/downloads";
pub const INSTANCES_ROOT: &str = "daemon/instances";

pub struct Files {
    protocol_config: ProtocolConfig,
    // use ahash to speed up ops
    upload_sessions: HashMap<Uuid, FileUploadInfo, ahash::RandomState>,
    // use ahash to speed up ops
    download_sessions: HashMap<Uuid, FileDownloadInfo, ahash::RandomState>,
}

// files utils
impl Files {
    pub fn new(protocol_config: ProtocolConfig) -> Self {
        Self::init_dirs()
            .context("failed to initialize directories")
            .unwrap();
        Self {
            protocol_config,
            upload_sessions: HashMap::default(),
            download_sessions: HashMap::default(),
        }
    }

    fn init_dirs() -> std::io::Result<()> {
        fs::create_dir_all(ROOT)?;
        fs::create_dir_all(DOWNLOAD_ROOT)?;
        fs::create_dir_all(INSTANCES_ROOT)?;
        Ok(())
    }

    // 算法层面，判断path是否在root下
    fn validate_path(path: &str, root: &str) -> bool {
        let normalized_path = Self::normalize_path(path);
        let normalized_root = Self::normalize_path(root);
        normalized_path.starts_with(&normalized_root)
    }

    // 从算法层面，将包含..和.的相对路径，转化为绝对路径
    fn normalize_path(path: &str) -> String {
        let parts = path
            .split(['\\', '/'])
            .filter(|s| !s.is_empty())
            .collect::<Vec<&str>>();

        let mut stack = vec![];
        parts.into_iter().for_each(|part| match part {
            "." => {}
            ".." => {
                let _ = stack.pop();
                stack.push(part);
            }
            _ => stack.push(part),
        });

        stack.iter().fold(String::new(), |mut path, part| {
            path.push_str(part);
            path.push('/');
            path
        })
    }

    pub async fn get_sha1(path: &str) -> anyhow::Result<String> {
        let path = path.to_string();
        tokio::task::spawn_blocking(|| -> anyhow::Result<String> {
            let mut hasher = Sha1::new();
            let mut file = std::fs::File::options().read(true).open(path)?;
            let mut buffer = [0; 32768];
            loop {
                let read = file.read(&mut buffer);
                if read.is_err() {
                    break;
                }
                hasher.update(&buffer[..read.unwrap()]);
            }
            Ok(format!("{:x}", hasher.finalize()))
        })
        .await
        .unwrap() // unwarp is safe: won't cancel and panic
    }

    /// encode bytes to utf16 string
    fn bytes_to_string_data(mut bytes: Vec<u8>) -> String {
        if bytes.len() % 2 != 0 {
            bytes.push(0)
        }

        String::from_utf16(
            &bytes
                .chunks(2)
                .map(|c| c[0] as u16 | (c[1] as u16) << 8)
                .collect::<Vec<u16>>(),
        )
        .unwrap()
    }
}

// upload operations
impl Files {
    pub async fn upload_request(
        &self,
        path: Option<&str>,
        size: u64,
        chunk_size: u64,
        sha1: Option<&str>,
    ) -> anyhow::Result<Uuid> {
        if path.is_some_and(|p| Self::validate_path(p, ROOT)) {
            bail!("invalid path");
        }
        let path = path.unwrap_or(DOWNLOAD_ROOT);

        // check if uploading, prevent extra io operation
        if self
            .upload_sessions
            .any_async(|_, v| v.base.path == path)
            .await
        {
            bail!("file is uploading");
        }

        let tmp_file = path.to_string() + ".tmp";

        let file = File::options()
            .create(true)
            .truncate(true)
            .write(true)
            .open(tmp_file)
            .await?;
        file.set_len(size).await?;

        let uuid = Uuid::new_v4();
        let info = FileUploadInfo::new(
            size,
            path.to_string(),
            file,
            sha1.map(|v| v.to_string()),
            chunk_size,
        );
        if self.upload_sessions.insert_async(uuid, info).await.is_err() {
            bail!("file is uploading");
        }
        debug!("uploading file: {}", path);

        Ok(uuid)
    }

    pub async fn upload_chunk(
        &self,
        file_id: Uuid,
        offset: u64,
        data: &str,
    ) -> anyhow::Result<(bool, u64)> {
        // parse string data to bytes ()
        let data: Vec<u16> = data.encode_utf16().collect();
        // convert vec<u16> to big endian bytes
        let data: Vec<u8> = data.iter().flat_map(|&v| v.to_be_bytes()).collect();

        if !self.upload_sessions.contains_async(&file_id).await {
            bail!("file is not uploading: upload session not found");
        }
        self.upload_sessions
            .read_async(&file_id, |_, v| {
                if offset >= v.base.size {
                    bail!("offset out of range");
                }
                Ok(())
            })
            .await
            .unwrap()?;

        {
            // file write chunk
            let session_info = self.upload_sessions.get_async(&file_id).await;
            if session_info.is_none() {
                bail!("file is not uploading: upload session not found");
            }
            let mut session_info = session_info.unwrap();
            let chunk_size = session_info.chunk_size as usize;
            let file = &mut session_info.base.file;
            file.seek(SeekFrom::Start(offset)).await?;
            file.write_all(&data[..std::cmp::min(chunk_size, data.len())])
                .await?;

            // update info
            session_info
                .base
                .remain
                .reduce(offset, offset + data.len() as u64);

            let remain = session_info.base.remain.get_remain();

            if remain > 0 {
                // partial upload
                return Ok((false, session_info.base.size - remain));
            }
        }

        let session_info = self.upload_sessions.remove_async(&file_id).await;
        if session_info.is_none() {
            bail!("file is not uploading: done but upload session not found");
        }
        let mut session_info = session_info.unwrap().1;
        // complete upload
        let path = session_info.base.path.clone();
        let sha1 = session_info.base.sha1.take();
        session_info.base.file.sync_all().await?;
        // move file
        tokio::fs::rename(path.clone() + ".tmp", &path).await?;
        drop(session_info); //close file

        debug!("upload finished: {}", &path);
        if let Some(sha1) = sha1 {
            let calculated_sha1 = Self::get_sha1(&path).await?;

            if sha1 != calculated_sha1 {
                bail!("sha1 mismatch");
            }
        }
        Ok((true, 0))
    }

    pub async fn upload_cancel(&self, file_id: Uuid) -> bool {
        if let Some(session_info) = self
            .upload_sessions
            .remove_async(&file_id)
            .await
            .map(|e| e.1)
        {
            drop(session_info.base.file); // close file
                                          // delete tmp file
            let _ = tokio::fs::remove_file(session_info.base.path.clone() + ".tmp").await;
            debug!("upload file cancelled: {}", session_info.base.path);
            true
        } else {
            false
        }
    }
}

// download operations
impl Files {
    pub async fn download_request(&self, path: &str) -> anyhow::Result<(Uuid, u64, String)> {
        if !Self::validate_path(path, ROOT) {
            bail!("invalid path");
        }

        if tokio::fs::try_exists(path).await? {
            bail!("file not found");
        }

        let mut file_sessions = 0u8;
        // use sync version
        self.download_sessions.scan(|_, v| {
            if v.base.path == path {
                file_sessions += 1;
            }
        });
        if file_sessions > self.protocol_config.v1.file_download_sessions {
            bail!("max download sessions of file '{}' reached", path);
        }

        let sha1 = Self::get_sha1(path).await?;
        let file = File::options().read(true).open(path).await?;
        let size = file.metadata().await.map(|m| m.len())?;
        let id = Uuid::new_v4();
        let session_info = FileDownloadInfo::new(size, path.to_string(), file, Some(sha1.clone()));
        if self
            .download_sessions
            .insert_async(id, session_info)
            .await
            .is_err()
        {
            bail!("could not open download session")
        }

        Ok((id, size, sha1))
    }

    pub async fn download_range(&self, id: Uuid, from: u64, to: u64) -> anyhow::Result<String> {
        if !self
            .download_sessions
            .read_async(&id, |_, v| to <= v.base.size && from < to)
            .await
            .unwrap_or(false)
        {
            bail!("invalid download file id or invalid range");
        }

        let mut entry = self
            .download_sessions
            .get_async(&id)
            .await
            .ok_or(anyhow!("download id not found"))?;

        entry
            .get_mut()
            .base
            .file
            .seek(SeekFrom::Start(from))
            .await?;
        let mut buf = vec![0; (to - from) as usize];
        entry.get_mut().base.file.read_buf(&mut buf).await?;
        Ok(Self::bytes_to_string_data(buf))
    }

    pub async fn download_close(&self, id: Uuid) -> anyhow::Result<()> {
        if self.download_sessions.remove_async(&id).await.is_none() {
            bail!("download id not found")
        }
        Ok(())
    }
}
