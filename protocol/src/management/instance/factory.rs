use crate::management::instance::config::InstanceConfig;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum InstanceFactoryMirror {
    #[default]
    None,
    BmclApi,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
pub struct InstanceFactorySetting {
    pub source: String,
    pub source_type: SourceType,
    #[serde(default = "InstanceFactoryMirror::default")]
    pub mirror: InstanceFactoryMirror,

    #[serde(flatten)]
    pub config: InstanceConfig,
}
