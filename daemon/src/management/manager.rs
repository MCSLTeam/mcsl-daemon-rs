use crate::management::config::InstanceConfigExt;
use crate::management::instance::{
    Instance, Minecraft, TInstance, Universal, UniversalInstance, INST_CFG_FILE_NAME,
};
use crate::storage::files::INSTANCES_ROOT;
use anyhow::{anyhow, Context};
use futures::SinkExt;
use log::{debug, warn};
use mcsl_protocol::management::instance::{InstanceConfig, InstanceFactorySetting, InstanceReport};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use uuid::Uuid;

// Container for storing Uuid: Instance pairs using scc::HashMap
pub struct InstanceContainer {
    instances: scc::HashMap<Uuid, Arc<dyn UniversalInstance + Send + Sync>>,
}

impl InstanceContainer {
    pub fn new() -> Self {
        InstanceContainer {
            instances: scc::HashMap::new(),
        }
    }

    // Insert an instance with a Uuid key
    pub fn insert<TInst: TInstance + 'static>(
        &self,
        uuid: Uuid,
        instance: Instance<TInst>,
    ) -> Option<(Uuid, Arc<dyn UniversalInstance + Send + Sync>)> {
        self.instances.insert(uuid, Arc::new(instance)).err()
    }

    // Retrieve an instance by Uuid and cast to the correct type
    pub fn get<TInst: TInstance + 'static>(&self, uuid: Uuid) -> Option<Arc<Instance<TInst>>> {
        self.instances.read(&uuid, |_, v| {
            if v.as_any().is::<Instance<TInst>>() {
                // Safety: We've verified the type is Instance<TInst>
                Some(unsafe {
                    Arc::from_raw(Arc::into_raw(Arc::clone(v)) as *const Instance<TInst>)
                })
            } else {
                None
            }
        })?
    }

    pub fn get_raw(&self, uuid: Uuid) -> Option<Arc<dyn UniversalInstance + Send + Sync>> {
        self.instances.read(&uuid, |_, v| Arc::clone(v))
    }

    // Remove an instance by Uuid
    pub fn remove(&self, uuid: Uuid) -> Option<Arc<dyn UniversalInstance + Send + Sync>> {
        self.instances.remove(&uuid).map(|entry| entry.1)
    }

    // Check if an instance exists for a Uuid
    pub fn contains(&self, uuid: Uuid) -> bool {
        self.contains(uuid)
    }
}

pub trait InstManagerTrait {
    async fn add(&self, setting: InstanceFactorySetting) -> anyhow::Result<InstanceConfig>;
    async fn remove(&self, inst_id: Uuid) -> bool;
    async fn start(
        &self,
        inst_id: Uuid,
    ) -> anyhow::Result<Arc<dyn UniversalInstance + Send + Sync>>;
    async fn stop(&self, inst_id: Uuid) -> bool;
    fn send(&self, inst_id: Uuid, message: &str) -> anyhow::Result<()>;
    fn kill(&self, inst_id: Uuid);
    async fn get_report(&self, inst_id: Uuid) -> anyhow::Result<InstanceReport>;
    async fn get_total_report(&self) -> anyhow::Result<HashMap<Uuid, InstanceReport>>;
}

pub struct InstManager {
    instances: InstanceContainer,
}

impl InstManagerTrait for InstManager {
    async fn add(&self, setting: InstanceFactorySetting) -> anyhow::Result<InstanceConfig> {
        todo!()
    }

    async fn remove(&self, inst_id: Uuid) -> bool {
        todo!()
    }

    async fn start(
        &self,
        inst_id: Uuid,
    ) -> anyhow::Result<Arc<dyn UniversalInstance + Send + Sync>> {
        let instance = self
            .instances
            .get_raw(inst_id)
            .ok_or(anyhow!("Instance not found"))?;
        instance.start().await?;

        Ok(instance)
    }

    async fn stop(&self, inst_id: Uuid) -> bool {
        let instance = self.instances.get_raw(inst_id);
        if let Some(instance) = instance {
            match instance.stop() {
                Ok(_) => true,
                Err(reason) => {
                    warn!("Error occurred while stopping instance: {}", reason);
                    false
                }
            }
        } else {
            false
        }
    }

    fn send(&self, inst_id: Uuid, message: &str) -> anyhow::Result<()> {
        if let Err(msg) = self
            .instances
            .get_raw(inst_id)
            .ok_or(anyhow!("Instance not found"))?
            .send(message)
        {
            Err(anyhow!("could not send message: {}", message))
        } else {
            Ok(())
        }
    }

    fn kill(&self, inst_id: Uuid) {
        todo!()
    }

    async fn get_report(&self, inst_id: Uuid) -> anyhow::Result<InstanceReport> {
        todo!()
    }

    async fn get_total_report(&self) -> anyhow::Result<HashMap<Uuid, InstanceReport>> {
        todo!()
    }
}

impl InstManager {
    pub fn new() -> Self {
        let mut manager = Self {
            instances: InstanceContainer::new(),
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
                .map(|content| serde_json::from_str::<InstanceConfig>(&content).ok())
                .flatten()
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

            if config.is_mc_server() {
                self.instances
                    .insert(uuid, Instance::<Minecraft>::new(config));
            } else {
                self.instances
                    .insert(uuid, Instance::<Universal>::new(config));
            }
            debug!("instance(uuid={}) added", uuid);
        }
        Ok(())
    }
}

// Example usage with async operations
#[cfg(test)]
mod tests {
    use super::*;
    use crate::management::instance::{Minecraft, Universal};
    use mcsl_protocol::management::instance::{InstanceType, TargetType};
    use tokio::task;

    #[tokio::test]
    async fn test_concurrent_access() {
        let container = Arc::new(InstanceContainer::new());
        let uuid1 = Uuid::new_v4();
        let uuid2 = Uuid::new_v4();

        // Create instances
        let config = InstanceConfig {
            uuid: Default::default(),
            name: "".to_string(),
            instance_type: InstanceType::None,
            target: "".to_string(),
            target_type: TargetType::Jar,
            mc_version: "".to_string(),
            input_encoding: Default::default(),
            output_encoding: Default::default(),
            java_path: "".to_string(),
            arguments: vec![],
            env: Default::default(),
        };
        let universal_instance = Instance::<Universal>::new(config.clone());
        let minecraft_instance = Instance::<Minecraft>::new(config);

        // Insert instances
        container.insert(uuid1, universal_instance);
        container.insert(uuid2, minecraft_instance);

        // Spawn tasks to call do_work concurrently
        let container_clone1 = Arc::clone(&container);
        let container_clone2 = Arc::clone(&container);
        let handle1 = task::spawn(async move {
            if let Some(instance) = container_clone1.get::<Universal>(uuid1) {
                instance.do_work().await;
            }
        });
        let handle2 = task::spawn(async move {
            if let Some(instance) = container_clone2.get::<Minecraft>(uuid2) {
                instance.do_work().await;
            }
        });

        // Spawn a task to remove an instance while do_work is running
        let container_clone3 = Arc::clone(&container);
        let handle3 = task::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            container_clone3.remove(uuid1);
        });

        // Wait for all tasks to complete
        let _ = tokio::try_join!(handle1, handle2, handle3).unwrap();

        // Verify state
        assert!(container.get::<Universal>(uuid1).is_none());
        assert!(container.get::<Minecraft>(uuid2).is_some());
    }
}
