use super::inst_factory::InstFactorySetting;
use super::inst_status::InstStatus;
use std::collections::HashMap;
use uuid::Uuid;

pub trait InstManager {
    async fn add(&self, setting: InstFactorySetting) -> anyhow::Result<()>;
    async fn remove(&self, inst_id: Uuid) -> anyhow::Result<()>;
    async fn start(&self, inst_id: Uuid) -> anyhow::Result<()>;
    async fn stop(&self, inst_id: Uuid) -> anyhow::Result<()>;
    async fn send(&self, inst_id: Uuid, message: &str) -> anyhow::Result<()>;
    async fn kill(&self, inst_id: Uuid) -> ();
    async fn status(&self, inst_id: Uuid) -> anyhow::Result<InstStatus>;
    async fn all_status(&self) -> anyhow::Result<HashMap<Uuid, InstStatus>>;
}

pub struct InstManagerImpl {}
