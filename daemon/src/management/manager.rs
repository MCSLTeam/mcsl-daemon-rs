use crate::management::config::InstanceConfigExt;
use crate::management::factory::InstanceFactoryManager;
use crate::management::instance::{Instance, INST_CFG_FILE_NAME};
use crate::management::strategy::strategies::{
    MinecraftInstanceStrategy, UniversalInstanceStrategy,
};
use crate::storage::files::INSTANCES_ROOT;
use anyhow::{anyhow, Context};
use log::{debug, warn};
use mcsl_protocol::management::instance::{InstanceConfig, InstanceFactorySetting, InstanceReport};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use uuid::Uuid;

pub trait InstManagerTrait {
    async fn add(&self, setting: InstanceFactorySetting) -> anyhow::Result<InstanceConfig>;
    async fn remove(&self, inst_id: Uuid) -> anyhow::Result<()>;
    async fn start(&self, inst_id: Uuid) -> anyhow::Result<Arc<Instance>>;
    async fn stop(&self, inst_id: Uuid) -> anyhow::Result<()>;
    fn send(&self, inst_id: Uuid, message: String) -> anyhow::Result<()>;
    fn kill(&self, inst_id: Uuid);
    async fn get_report(&self, inst_id: Uuid) -> anyhow::Result<InstanceReport>;
    async fn get_total_report(&self) -> HashMap<Uuid, InstanceReport>;
}

pub struct InstManager {
    instances: scc::HashMap<Uuid, Arc<Instance>, ahash::RandomState>,
    factory_manager: InstanceFactoryManager,
}

impl InstManager {
    fn get_instance(&self, uuid: Uuid) -> anyhow::Result<Arc<Instance>> {
        self.instances
            .read(&uuid, |_, v| Arc::clone(v))
            .ok_or(anyhow!("Instance not found"))
    }
    fn remove_instance(&self, uuid: Uuid) -> anyhow::Result<Arc<Instance>> {
        self.instances
            .remove(&uuid)
            .map(|entry| entry.1)
            .ok_or(anyhow!("Could not remove instance"))
    }
}

impl InstManagerTrait for InstManager {
    async fn add(&self, setting: InstanceFactorySetting) -> anyhow::Result<InstanceConfig> {
        todo!()
    }

    async fn remove(&self, inst_id: Uuid) -> anyhow::Result<()> {
        self.remove_instance(inst_id)?;
        fs::remove_dir_all(Path::new(INSTANCES_ROOT).join(inst_id.to_string()))
            .context("Could not remove instance from disk")?;
        Ok(())
    }

    async fn start(&self, inst_id: Uuid) -> anyhow::Result<Arc<Instance>> {
        let instance = self.get_instance(inst_id)?;
        instance.start().await?;
        Ok(instance)
    }

    async fn stop(&self, inst_id: Uuid) -> anyhow::Result<()> {
        self.get_instance(inst_id)?.stop().await
    }

    fn send(&self, inst_id: Uuid, message: String) -> anyhow::Result<()> {
        if let Err(err) = self.get_instance(inst_id)?.send(message) {
            Err(anyhow!("could not send message: {}", err.0))
        } else {
            Ok(())
        }
    }

    fn kill(&self, inst_id: Uuid) {
        if let Ok(instance) = self.get_instance(inst_id) {
            instance.kill()
        }
    }

    async fn get_report(&self, inst_id: Uuid) -> anyhow::Result<InstanceReport> {
        Ok(self.get_instance(inst_id)?.get_report().await)
    }

    async fn get_total_report(&self) -> HashMap<Uuid, InstanceReport> {
        let mut entry = self.instances.first_entry_async().await;
        let mut reports = HashMap::new();
        while let Some(e) = entry {
            reports.insert(*e.key(), e.get_report().await);
            entry = e.next_async().await;
        }
        reports
    }
}

impl Default for InstManager {
    fn default() -> Self {
        Self::new()
    }
}

impl InstManager {
    pub fn new() -> Self {
        let mut manager = Self {
            instances: scc::HashMap::default(),
            factory_manager: InstanceFactoryManager::new(),
        };
        manager
            .init()
            .context("failed to initialize instance manager")
            .unwrap();
        manager
    }

    fn init(&mut self) -> anyhow::Result<()> {
        for entry in fs::read_dir(Path::new(INSTANCES_ROOT))? {
            let entry = match entry {
                Ok(entry) => entry,
                Err(_) => continue,
            };

            if entry.file_type()?.is_file() {
                continue;
            }

            let uuid = match Uuid::parse_str(entry.file_name().to_str().unwrap()) {
                Ok(uuid) => uuid,
                Err(_) => continue,
            };

            let cfg_path = entry.path().join(INST_CFG_FILE_NAME);

            let config = match fs::read_to_string(cfg_path)
                .ok()
                .and_then(|content| serde_json::from_str::<InstanceConfig>(&content).ok())
            {
                Some(cfg) => cfg,
                None => continue,
            };

            if config.uuid != uuid {
                warn!(
                    "instance(name={}) with inconsistent uuid found, ignored",
                    config.name
                );
                continue;
            }

            let instance = if config.is_mc_server() {
                Instance::new::<MinecraftInstanceStrategy>(config)
            } else {
                Instance::new::<UniversalInstanceStrategy>(config)
            };

            self.instances
                .insert(uuid, Arc::new(instance))
                .map_err(|(k, _)| {
                    anyhow::anyhow!("could not create instance(uuid={}) with conflicted uuid", k)
                })?;
            debug!("instance(uuid={}) added", uuid);
        }
        Ok(())
    }
}
