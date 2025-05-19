use crate::management::comm::ProcessStartInfo;
use crate::storage::files::INSTANCES_ROOT;
use lazy_static::lazy_static;
use log::warn;
use mcsl_protocol::management::instance::{InstanceConfig, InstanceType, TargetType};
use mcsl_protocol::utils::PlaceHolderRender;
use std::collections::HashMap;
use std::ffi::OsString;
use std::path;
use std::path::{Path, PathBuf};

lazy_static! {
    static ref STRING_ENVS: HashMap<String, String> = {
        std::env::vars_os()
            .into_iter()
            .map(|(var, value)| {
                (
                    var.to_string_lossy().to_string(),
                    value.to_string_lossy().to_string(),
                )
            })
            .collect()
    };
}

pub trait InstanceConfigExt {
    fn is_mc_server(&self) -> bool;
    fn get_working_dir(&self) -> PathBuf;

    fn get_start_info(&self) -> ProcessStartInfo;
    fn get_launch_script(&self) -> (String, Vec<String>);
}

impl InstanceConfigExt for InstanceConfig {
    fn is_mc_server(&self) -> bool {
        self.instance_type != InstanceType::None
    }
    fn get_working_dir(&self) -> PathBuf {
        Path::new(INSTANCES_ROOT).join(self.uuid.to_string())
    }

    fn get_start_info(&self) -> ProcessStartInfo {
        let mut envs = HashMap::new();
        for (k, v) in std::env::vars_os() {
            let utf8_k = k.to_string_lossy();
            if self.env.contains_key(utf8_k.as_ref()) {
                match v.to_string_lossy().to_string().format(&STRING_ENVS) {
                    Ok(rendered) => {
                        envs.insert(k, OsString::from(rendered));
                    }
                    Err(e) => {
                        warn!(
                            "Failed to parse environment variable {}: {}, ignored",
                            utf8_k, e
                        );
                        envs.insert(k, v);
                    }
                }
            } else {
                envs.insert(k, v);
            }
        }
        let (target, args) = self.get_launch_script();
        ProcessStartInfo { target, args, envs }
    }

    fn get_launch_script(&self) -> (String, Vec<String>) {
        let full_path = path::absolute(
            self.get_working_dir()
                .as_path()
                .join(Path::new(&self.target)),
        )
        .unwrap();

        match self.target_type {
            TargetType::Jar => {
                let mut args = vec![];
                args.extend_from_slice(self.arguments.as_slice());
                args.push("-jar".into());
                args.push(self.target.clone());
                if self.is_mc_server() {
                    args.push("nogui".into());
                }

                (self.java_path.clone(), args)
            }
            TargetType::Script => (full_path.to_string_lossy().to_string(), vec![]),
            TargetType::Executable => (
                full_path.to_string_lossy().to_string(),
                self.arguments.clone(),
            ),
        }
    }
}
