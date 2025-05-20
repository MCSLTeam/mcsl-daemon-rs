use serde::{Deserialize, Serialize};
use std::fs::{self, DirEntry, Metadata};
use std::io;
use std::path::Path;
use thiserror::Error;

// 自定义错误类型
#[derive(Error, Debug)]
pub enum FileSystemError {
    #[error("IO error: {0}")]
    IoError(#[from] io::Error),
    #[error("Invalid path: {0}")]
    InvalidPath(String),
    #[cfg(windows)]
    #[error("Command error: {0}")]
    CommandError(String),
}

// DirectoryMeta 结构体
#[derive(Debug, Serialize, Deserialize)]
pub struct DirectoryMeta {
    creation_time: u64,
    last_access_time: u64,
    last_write_time: u64,
    hidden: bool,
}

impl DirectoryMeta {
    /// 从文件系统元数据和 DirEntry 初始化 DirectoryMeta
    pub fn from_metadata_and_entry(
        metadata: &Metadata,
        entry: &DirEntry,
    ) -> Result<Self, FileSystemError> {
        Ok(DirectoryMeta {
            creation_time: metadata
                .created()
                .map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap().as_secs())
                .unwrap_or(0),
            last_access_time: metadata
                .accessed()
                .map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap().as_secs())
                .unwrap_or(0),
            last_write_time: metadata
                .modified()
                .map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap().as_secs())
                .unwrap_or(0),
            hidden: is_hidden(metadata, entry),
        })
    }
}

// FileMeta 结构体
#[derive(Debug, Serialize, Deserialize)]
pub struct FileMeta {
    creation_time: u64,
    last_access_time: u64,
    last_write_time: u64,
    hidden: bool,
    read_only: bool,
    size: u64,
}

impl FileMeta {
    /// 从文件系统元数据和 DirEntry 初始化 FileMeta
    pub fn from_metadata_and_entry(
        metadata: &Metadata,
        entry: &DirEntry,
    ) -> Result<Self, FileSystemError> {
        Ok(FileMeta {
            creation_time: metadata
                .created()
                .map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap().as_secs())
                .unwrap_or(0),
            last_access_time: metadata
                .accessed()
                .map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap().as_secs())
                .unwrap_or(0),
            last_write_time: metadata
                .modified()
                .map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap().as_secs())
                .unwrap_or(0),
            hidden: is_hidden(metadata, entry),
            read_only: metadata.permissions().readonly(),
            size: metadata.len(),
        })
    }
}

// DirectoryInfo 结构体
#[derive(Debug, Serialize, Deserialize)]
pub struct DirectoryInfo {
    name: String,
    #[serde(flatten)]
    meta: DirectoryMeta,
}

impl DirectoryInfo {
    /// 从 DirEntry 初始化 DirectoryInfo
    pub fn from_dir_entry(entry: &DirEntry) -> Result<Self, FileSystemError> {
        let metadata = entry.metadata()?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| FileSystemError::InvalidPath("Invalid file name".to_string()))?;
        let meta = DirectoryMeta::from_metadata_and_entry(&metadata, entry)?;
        Ok(DirectoryInfo { name, meta })
    }
}

// FileInfo 结构体
#[derive(Debug, Serialize, Deserialize)]
pub struct FileInfo {
    name: String,
    #[serde(flatten)]
    meta: FileMeta,
}

impl FileInfo {
    /// 从 DirEntry 初始化 FileInfo
    pub fn from_dir_entry(entry: &DirEntry) -> Result<Self, FileSystemError> {
        let metadata = entry.metadata()?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| FileSystemError::InvalidPath("Invalid file name".to_string()))?;
        let meta = FileMeta::from_metadata_and_entry(&metadata, entry)?;
        Ok(FileInfo { name, meta })
    }
}

// DirectoryEntry 结构体
#[derive(Debug, Serialize, Deserialize)]
pub struct DirectoryEntry {
    parent: Option<String>,
    files: Vec<FileInfo>,
    directories: Vec<DirectoryInfo>,
}

impl DirectoryEntry {
    /// 从路径和根路径初始化 DirectoryEntry
    pub fn new<P: AsRef<Path>>(path: P, root: P) -> Result<Self, FileSystemError> {
        let path = path.as_ref();
        let root = root.as_ref();
        let metadata = fs::metadata(path)?;
        if !metadata.is_dir() {
            return Err(FileSystemError::InvalidPath(format!(
                "{} is not a directory",
                path.display()
            )));
        }

        let mut files = Vec::new();
        let mut directories = Vec::new();

        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let metadata = entry.metadata()?;
            if metadata.is_file() {
                files.push(FileInfo::from_dir_entry(&entry)?);
            } else if metadata.is_dir() {
                directories.push(DirectoryInfo::from_dir_entry(&entry)?);
            }
        }

        let parent = get_relative_path(root, path)?;

        Ok(DirectoryEntry {
            parent,
            files,
            directories,
        })
    }
}

// 辅助函数：检查文件是否隐藏
fn is_hidden(metadata: &Metadata, entry: &DirEntry) -> bool {
    #[cfg(windows)]
    {
        // Windows: 检查 FILE_ATTRIBUTE_HIDDEN
        const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
        metadata.file_attributes() & FILE_ATTRIBUTE_HIDDEN != 0
    }

    #[cfg(unix)]
    {
        // Unix: 检查文件名是否以 . 开头
        entry
            .file_name()
            .to_str()
            .map(|name| name.starts_with('.'))
            .unwrap_or(false)
    }

    #[cfg(not(any(windows, unix)))]
    {
        // 其他平台：默认返回 false
        false
    }
}

// 辅助函数：计算相对路径
fn get_relative_path<P: AsRef<Path>>(root: P, path: P) -> Result<Option<String>, FileSystemError> {
    let root = root.as_ref();
    let path = path.as_ref();
    let root = fs::canonicalize(root)?;
    let path = fs::canonicalize(path)?;
    let relative = path
        .strip_prefix(&root)
        .map_err(|_| FileSystemError::InvalidPath("Path is not under root".to_string()))?;
    if relative == Path::new("") {
        Ok(None)
    } else {
        Ok(Some(
            relative
                .to_str()
                .ok_or_else(|| FileSystemError::InvalidPath("Invalid path string".to_string()))?
                .to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::io::Write;

    use tempfile::TempDir;

    fn create_test_dir() -> Result<TempDir, FileSystemError> {
        let temp_dir = tempfile::tempdir()?;
        let dir_path = temp_dir.path();

        // 创建普通文件
        let file_path = dir_path.join("test.txt");
        File::create(&file_path)?.write_all(b"test")?;

        // 创建隐藏文件
        #[cfg(unix)]
        let hidden_file_path = dir_path.join(".hidden.txt");
        #[cfg(windows)]
        let hidden_file_path = dir_path.join("hidden.txt");

        File::create(&hidden_file_path)?.write_all(b"hidden")?;

        #[cfg(windows)]
        {
            // 设置 Windows 隐藏属性（文件）
            use std::process::Command;
            let output = Command::new("attrib")
                .arg("+H")
                .arg(&hidden_file_path)
                .output()
                .map_err(|e| {
                    FileSystemError::CommandError(format!("Failed to run attrib: {}", e))
                })?;
            if !output.status.success() {
                return Err(FileSystemError::CommandError(format!(
                    "attrib failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                )));
            }
        }

        // 创建子目录
        let subdir_path = dir_path.join("subdir");
        fs::create_dir(&subdir_path)?;

        // 创建隐藏子目录
        #[cfg(unix)]
        let hidden_subdir_path = dir_path.join(".hidden_dir");
        #[cfg(windows)]
        let hidden_subdir_path = dir_path.join("hidden_dir");

        fs::create_dir(&hidden_subdir_path)?;

        #[cfg(windows)]
        {
            // 设置 Windows 隐藏属性（目录）
            use std::process::Command;
            let output = Command::new("attrib")
                .arg("+H")
                .arg(&hidden_subdir_path)
                .output()
                .map_err(|e| {
                    FileSystemError::CommandError(format!("Failed to run attrib: {}", e))
                })?;
            if !output.status.success() {
                return Err(FileSystemError::CommandError(format!(
                    "attrib failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                )));
            }
        }

        Ok(temp_dir)
    }

    #[test]
    fn test_directory_entry() {
        let temp_dir = create_test_dir().unwrap();
        let dir_path = temp_dir.path();

        let entry = DirectoryEntry::new(dir_path, dir_path).unwrap();

        assert_eq!(entry.parent, None);
        assert_eq!(entry.files.len(), 2); // test.txt 和 hidden.txt
        assert_eq!(entry.directories.len(), 2); // subdir 和 hidden_dir

        // 验证普通文件
        let test_file = entry.files.iter().find(|f| f.name == "test.txt").unwrap();
        assert!(!test_file.meta.hidden);

        // 验证隐藏文件
        #[cfg(unix)]
        let hidden_file_name = ".hidden.txt";
        #[cfg(windows)]
        let hidden_file_name = "hidden.txt";

        let hidden_file = entry
            .files
            .iter()
            .find(|f| f.name == hidden_file_name)
            .unwrap();
        assert!(hidden_file.meta.hidden);

        // 验证普通目录
        let subdir = entry
            .directories
            .iter()
            .find(|d| d.name == "subdir")
            .unwrap();
        assert!(!subdir.meta.hidden);

        // 验证隐藏目录
        #[cfg(unix)]
        let hidden_dir_name = ".hidden_dir";
        #[cfg(windows)]
        let hidden_dir_name = "hidden_dir";

        let hidden_dir = entry
            .directories
            .iter()
            .find(|d| d.name == hidden_dir_name)
            .unwrap();
        assert!(hidden_dir.meta.hidden);
    }

    #[test]
    fn test_invalid_path() {
        let result = DirectoryEntry::new("/non/existent/path", "/non/existent/path");
        assert!(result.is_err());
    }
}
