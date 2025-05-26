use mcsl_protocol::management::instance::{InstanceConfig, InstanceFactorySetting};

pub trait InstanceFactoryConstructor {
    fn new() -> Self
    where
        Self: Sized;
}

#[async_trait::async_trait]
pub trait CoreInstanceFactory: InstanceFactoryConstructor {
    async fn install(&mut self, setting: InstanceFactorySetting) -> anyhow::Result<InstanceConfig>;
}
#[async_trait::async_trait]
pub trait ArchiveInstanceFactory: InstanceFactoryConstructor {
    async fn install(&mut self, setting: InstanceFactorySetting) -> anyhow::Result<InstanceConfig>;
}
#[async_trait::async_trait]
pub trait ScriptInstanceFactory: InstanceFactoryConstructor {
    async fn install(&mut self, setting: InstanceFactorySetting) -> anyhow::Result<InstanceConfig>;
}
