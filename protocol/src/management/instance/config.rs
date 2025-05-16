use crate::utils::encoding::Encoding;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstanceType {
    None, // 非MC服务器实例类型, (默认值)
    Universal,
    Fabric,
    Forge,
    NeoForge,
    Cleanroom,
    Quilt,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetType {
    /// <summary>
    ///     目标文件为Java Jar文件
    /// </summary>
    Jar,

    /// <summary>
    ///     目标文件为脚本文件(bat, ps1, sh, ...)
    /// </summary>
    Script,

    /// <summary>
    ///     目标文件为可执行文件
    /// </summary>
    Executable,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceConfig {
    #[serde(default = "uuid::Uuid::new_v4")]
    pub uuid: Uuid,

    pub name: String,
    pub instance_type: InstanceType,
    pub target: String,
    pub target_type: TargetType,

    #[serde(default)]
    pub mc_version: String,
    #[serde(default)]
    pub input_encoding: Encoding,
    #[serde(default)]
    pub output_encoding: Encoding,
    #[serde(default = "default_java_path")]
    pub java_path: String,
    #[serde(default)]
    pub arguments: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
}

fn default_java_path() -> String {
    "java".to_owned()
}
