use crate::utils::U64Remain;
use std::path::Path;

use serde::{Deserialize, Serialize};

pub trait FileIoWithBackup {
    /// Writes the given content to a file and creates a backup of the file before writing.
    fn write_with_backup<P: AsRef<Path>>(path: P, content: &str) -> Result<(), std::io::Error> {
        let path = path.as_ref();

        if path.exists() {
            let backup_path = path.with_extension("bak");

            // Create a backup of the file
            std::fs::copy(path, backup_path)?;
        }

        // Write the content to the file
        std::fs::write(path, content)?;

        Ok(())
    }
}

/// Trait for configuration handling.
pub trait Config: FileIoWithBackup {
    type ConfigType: Serialize + for<'de> Deserialize<'de>;

    fn load_config<P: AsRef<Path>>(path: P) -> anyhow::Result<Self::ConfigType> {
        let path = path.as_ref();
        let content = std::fs::read_to_string(path)?;
        let config: Self::ConfigType = serde_json::from_str(&content)?;
        Ok(config)
    }

    fn save_config<P: AsRef<Path>>(path: P, config: &Self::ConfigType) -> anyhow::Result<()> {
        let path = path.as_ref();
        let content = serde_json::to_string_pretty(config)?;
        Self::write_with_backup(path, &content)?;
        Ok(())
    }

    fn load_config_or_default<P: AsRef<Path>, F: FnOnce() -> Self::ConfigType>(
        path: P,
        default: F,
    ) -> anyhow::Result<Self::ConfigType> {
        match std::fs::metadata(path.as_ref()) {
            Ok(metadata) if metadata.is_file() => Self::load_config(path),
            _ => {
                let config = default();
                Self::save_config(path, &config)?;
                Ok(config)
            }
        }
    }
}

// FileLoadInfo 类似父类
pub struct FileLoadInfo {
    pub size: u64,
    pub file: tokio::fs::File,
    pub sha1: Option<String>,
    pub path: String,
    pub remain: U64Remain,
}

impl FileLoadInfo {
    pub fn new(size: u64, path: String, file: tokio::fs::File, sha1: Option<String>) -> Self {
        Self {
            size,
            file,
            sha1: sha1.map(|v| v.to_lowercase()),
            path,
            remain: U64Remain::new(0, size),
        }
    }
}

pub struct FileUploadInfo {
    pub base: FileLoadInfo,
    pub chunk_size: u64,
}

impl FileUploadInfo {
    pub fn new(
        size: u64,
        path: String,
        file: tokio::fs::File,
        sha1: Option<String>,
        chunk_size: u64,
    ) -> Self {
        Self {
            base: FileLoadInfo::new(size, path, file, sha1),
            chunk_size,
        }
    }
}

pub struct FileDownloadInfo {
    pub base: FileLoadInfo,
}

impl FileDownloadInfo {
    pub fn new(size: u64, path: String, file: tokio::fs::File, sha1: Option<String>) -> Self {
        Self {
            base: FileLoadInfo::new(size, path, file, sha1),
        }
    }
}
