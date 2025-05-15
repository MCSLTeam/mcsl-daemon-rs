use crate::utils::encoding::Encoding;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
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

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceType {
    /// <summary>
    ///     仅初始化, 非法值
    /// </summary>
    None,

    /// <summary>
    ///     压缩包(zip)(解压后直接创建)
    /// </summary>
    Archive,

    /// <summary>
    ///     核心文件(按照配置进行目标文件安装)
    /// </summary>
    Core,

    /// <summary>
    ///     脚本文件(运行脚本安装)
    /// </summary>
    Script,
}

#[derive(Debug, Serialize, Deserialize)]
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
#[derive(Debug, Serialize, Deserialize)]
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
