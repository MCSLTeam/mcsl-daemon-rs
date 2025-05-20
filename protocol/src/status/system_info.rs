use serde::{Deserialize, Serialize};
use std::io;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SystemInfoError {
    #[error("IO error: {0}")]
    IoError(#[from] io::Error),
    #[error("Invalid operation: {0}")]
    InvalidOperation(String),
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct OsInfo {
    pub name: String,
    pub arch: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct CpuInfo {
    pub vendor: String,
    pub name: String,
    pub count: u32,
    pub usage: f32,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct MemInfo {
    pub total: u64,
    pub free: u64,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct DriveInfo {
    pub drive_format: String,
    pub total: u64,
    pub free: u64,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct SysInfo {
    pub os: OsInfo,
    pub cpu: CpuInfo,
    pub mem: MemInfo,
    pub drive: DriveInfo,
}
