use std::path::PathBuf;

use crate::utils::Encoding;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InstType {
    Vanilla,
    Forge,
    Fabric,
    Spigot,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TargetType {
    Jar,
    Script,
}

const FILE_NAME: &'static str = "daemon_instance.json";

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct InstConfig {
    pub uuid: Uuid,
    pub input_encoding: Encoding,
    pub working_directory: PathBuf,
    pub java_args: Vec<String>,
    pub java_path: PathBuf,
    pub name: String,
    pub output_encoding: Encoding,
    pub instance_type: InstType,
    pub target: PathBuf,
    pub target_type: TargetType,
}

pub struct InstConfigBuilder {
    uuid: Option<Uuid>,
    input_encoding: Option<Encoding>,
    working_directory: Option<PathBuf>,
    java_args: Option<Vec<String>>,
    java_path: Option<PathBuf>,
    name: Option<String>,
    output_encoding: Option<Encoding>,
    instance_type: Option<InstType>,
    target: Option<PathBuf>,
    target_type: Option<TargetType>,
}

#[allow(dead_code)]
impl InstConfigBuilder {
    pub fn new() -> Self {
        Self {
            uuid: None,
            input_encoding: None,
            working_directory: None,
            java_args: None,
            java_path: None,
            name: None,
            output_encoding: None,
            instance_type: None,
            target: None,
            target_type: None,
        }
    }

    pub fn uuid(mut self, uuid: Uuid) -> Self {
        self.uuid = Some(uuid);
        self
    }

    pub fn input_encoding(mut self, input_encoding: Encoding) -> Self {
        self.input_encoding = Some(input_encoding);
        self
    }

    pub fn working_directory<P: Into<PathBuf>>(mut self, working_directory: P) -> Self {
        self.working_directory = Some(working_directory.into());
        self
    }

    pub fn java_args(mut self, java_args: Vec<String>) -> Self {
        self.java_args = Some(java_args);
        self
    }

    pub fn java_path<P: Into<PathBuf>>(mut self, java_path: P) -> Self {
        self.java_path = Some(java_path.into());
        self
    }

    pub fn name<S: Into<String>>(mut self, name: S) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn output_encoding(mut self, output_encoding: Encoding) -> Self {
        self.output_encoding = Some(output_encoding);
        self
    }

    pub fn instance_type(mut self, instance_type: InstType) -> Self {
        self.instance_type = Some(instance_type);
        self
    }

    pub fn target<P: Into<PathBuf>>(mut self, target: P) -> Self {
        self.target = Some(target.into());
        self
    }

    pub fn target_type(mut self, target_type: TargetType) -> Self {
        self.target_type = Some(target_type);
        self
    }

    pub fn build(self) -> anyhow::Result<InstConfig> {
        let uuid = self.uuid.unwrap_or_else(Uuid::new_v4);
        Ok(InstConfig {
            uuid,
            input_encoding: self.input_encoding.unwrap_or(Encoding::UTF8),
            working_directory: self
                .working_directory
                .unwrap_or_else(|| format!("./daemon1/instances/{}", uuid).into()),
            java_args: self.java_args.unwrap_or_default(),
            java_path: self.java_path.unwrap_or_else(|| "java".into()),
            name: self.name.ok_or(anyhow::anyhow!("name not set"))?,
            output_encoding: self.output_encoding.unwrap_or(Encoding::UTF8),
            instance_type: self
                .instance_type
                .ok_or(anyhow::anyhow!("instance_type not set"))?,
            target: self.target.ok_or(anyhow::anyhow!("target not set"))?,
            target_type: self
                .target_type
                .ok_or(anyhow::anyhow!("target_type not set"))?,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;
    use std::sync::LazyLock;

    use serde_json::Value;

    use super::*;

    // static INST_CONFIG: LazyLock<InstConfig> = LazyLock::new(|| InstConfig {
    //     uuid: Uuid::from_str("2a42f6ab-8bd9-450c-a391-5ee3bffffb64").unwrap(),
    //     input_encoding: Encoding::UTF8,
    //     working_directory: "./instances/2a42f6ab-8bd9-450c-a391-5ee3bffffb64".into(),
    //     java_args: vec!["-Xmx1G".to_string()],
    //     java_path: "/usr/bin/java".into(),
    //     name: "test".to_string(),
    //     output_encoding: Encoding::UTF8,
    //     instance_type: InstType::Vanilla,
    //     target: "server.jar".into(),
    //     target_type: TargetType::Jar,
    // });
    static INST_CONFIG: LazyLock<InstConfig> = LazyLock::new(|| {
        InstConfigBuilder::new()
            .uuid(Uuid::from_str("2a42f6ab-8bd9-450c-a391-5ee3bffffb64").unwrap())
            .input_encoding(Encoding::UTF8)
            .working_directory("./instances/2a42f6ab-8bd9-450c-a391-5ee3bffffb64")
            .java_args(vec!["-Xmx1G".to_string()])
            .java_path("/usr/bin/java")
            .name("test")
            .output_encoding(Encoding::UTF8)
            .instance_type(InstType::Vanilla)
            .target("server.jar")
            .target_type(TargetType::Jar)
            .build()
            .unwrap()
    });

    const INST_CONFIG_TEXT: &str = r#"{
        "uuid": "2a42f6ab-8bd9-450c-a391-5ee3bffffb64",
        "input_encoding": "utf-8",
        "working_directory": "./instances/2a42f6ab-8bd9-450c-a391-5ee3bffffb64",
        "java_args": [
            "-Xmx1G"
        ],
        "java_path": "/usr/bin/java",
        "name": "test",
        "output_encoding": "utf-8",
        "instance_type": "vanilla",
        "target": "server.jar",
        "target_type": "jar"
    }"#;

    #[test]
    fn inst_config_deserialize_test() {
        let deserialized: InstConfig = serde_json::from_str(INST_CONFIG_TEXT).unwrap();
        assert_eq!(*INST_CONFIG, deserialized);
    }

    #[test]
    fn inst_config_serialize_test() {
        let serialized = serde_json::to_string_pretty(&*INST_CONFIG).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(serialized.as_str()).unwrap(),
            serde_json::from_str::<Value>(INST_CONFIG_TEXT).unwrap()
        );
    }
}
