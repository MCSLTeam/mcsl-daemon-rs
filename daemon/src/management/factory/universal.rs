use crate::management::factory::factory::{
    ArchiveInstanceFactory, CoreInstanceFactory, InstanceFactoryConstructor, ScriptInstanceFactory,
};
use crate::management::factory::setting::InstanceFactorySettingExt;
use mcsl_protocol::management::instance::{InstanceConfig, InstanceFactorySetting, TargetType};

pub struct UniversalInstanceFactory;

impl InstanceFactoryConstructor for UniversalInstanceFactory {
    fn new() -> Self
    where
        Self: Sized,
    {
        Self {}
    }
}

#[async_trait::async_trait]
impl CoreInstanceFactory for UniversalInstanceFactory {
    async fn install(&mut self, setting: InstanceFactorySetting) -> anyhow::Result<InstanceConfig> {
        setting.copy_and_rename_target().await?;
        setting.fix_eula().await?;

        Ok(if !matches!(setting.config.target_type, TargetType::Jar) {
            let mut config = setting.config;
            config.target_type = TargetType::Jar;
            config
        } else {
            setting.config
        })
    }
}

#[async_trait::async_trait]
impl ArchiveInstanceFactory for UniversalInstanceFactory {
    async fn install(&mut self, setting: InstanceFactorySetting) -> anyhow::Result<InstanceConfig> {
        todo!()
    }
}

#[async_trait::async_trait]
impl ScriptInstanceFactory for UniversalInstanceFactory {
    async fn install(&mut self, setting: InstanceFactorySetting) -> anyhow::Result<InstanceConfig> {
        todo!()
    }
}
