use crate::management::factory::factory::{
    ArchiveInstanceFactory, CoreInstanceFactory, InstanceFactoryConstructor, ScriptInstanceFactory,
};
use crate::management::factory::universal::UniversalInstanceFactory;
use crate::management::minecraft::MinecraftVersion;
use crate::management::version::Version;
use anyhow::Context;
use mcsl_protocol::management::instance::{
    InstanceConfig, InstanceFactorySetting, InstanceType, SourceType,
};

#[derive(Debug, Clone)]
struct Conditions {
    pub instance_type: InstanceType,
    pub min_version: Option<Version>,
    pub max_version: Option<Version>,
}

impl Conditions {
    pub fn new(
        instance_type: InstanceType,
        min_version: Option<Version>,
        max_version: Option<Version>,
    ) -> Self {
        Self {
            instance_type,
            min_version,
            max_version,
        }
    }
}

pub struct InstanceFactoryManager {
    core_factories: Vec<(
        Conditions,
        Box<dyn Fn() -> Box<dyn CoreInstanceFactory + Send> + Send + Sync>,
    )>,
    archive_factories: Vec<(
        Conditions,
        Box<dyn Fn() -> Box<dyn ArchiveInstanceFactory + Send> + Send + Sync>,
    )>,
    script_factories: Vec<(
        Conditions,
        Box<dyn Fn() -> Box<dyn ScriptInstanceFactory + Send> + Send + Sync>,
    )>,
}

impl InstanceFactoryManager {
    pub fn new() -> Self {
        let mut manager = Self {
            core_factories: vec![],
            archive_factories: vec![],
            script_factories: vec![],
        };
        manager.init();
        manager
    }

    fn init(&mut self) {
        self.register_all::<UniversalInstanceFactory>(Conditions::new(
            InstanceType::Universal,
            None,
            None,
        ));
    }
}

impl InstanceFactoryManager {
    fn register_core<T>(&mut self, conditions: Conditions)
    where
        T: CoreInstanceFactory + Send + 'static + InstanceFactoryConstructor,
    {
        let ctor = Box::new(|| Box::new(T::new()) as Box<dyn CoreInstanceFactory + Send>);
        self.core_factories.push((conditions, ctor));
    }

    fn register_archive<T>(&mut self, conditions: Conditions)
    where
        T: ArchiveInstanceFactory + Send + 'static + InstanceFactoryConstructor,
    {
        let ctor = Box::new(|| Box::new(T::new()) as Box<dyn ArchiveInstanceFactory + Send>);
        self.archive_factories.push((conditions, ctor));
    }

    fn register_script<T>(&mut self, conditions: Conditions)
    where
        T: ScriptInstanceFactory + Send + 'static + InstanceFactoryConstructor,
    {
        let ctor = Box::new(|| Box::new(T::new()) as Box<dyn ScriptInstanceFactory + Send>);
        self.script_factories.push((conditions, ctor));
    }

    fn register_all<T>(&mut self, conditions: Conditions)
    where
        T: CoreInstanceFactory
            + ArchiveInstanceFactory
            + ScriptInstanceFactory
            + Send
            + 'static
            + InstanceFactoryConstructor,
    {
        self.register_core::<T>(conditions.clone());
        self.register_archive::<T>(conditions.clone());
        self.register_script::<T>(conditions);
    }
}

impl InstanceFactoryManager {
    fn get_ctor<TCtor>(
        instance_type: InstanceType,
        version: MinecraftVersion,
        list: &[(Conditions, TCtor)],
    ) -> Option<&TCtor> {
        for (cond, ctor) in list {
            if instance_type != cond.instance_type {
                continue;
            }

            // 如果版本没有指定, 直接返回
            match version {
                MinecraftVersion::Release(ref version) => {
                    if cond
                        .min_version
                        .as_ref()
                        .map(|min_version| min_version <= version)
                        .unwrap_or(true)
                        && cond
                            .max_version
                            .as_ref()
                            .map(|max_version| version <= max_version)
                            .unwrap_or(true)
                    {
                        return Some(ctor);
                    }
                }
                MinecraftVersion::Snapshot(_) => {
                    if cond.min_version.is_none() && cond.max_version.is_none() {
                        return Some(ctor);
                    }
                }
                MinecraftVersion::None => return Some(ctor),
            }
        }
        None
    }
    pub async fn install(&self, setting: InstanceFactorySetting) -> anyhow::Result<InstanceConfig> {
        let raw_version = setting.config.mc_version.as_str();
        let minecraft_version = MinecraftVersion::try_from(raw_version)
            .context(format!("Could not parse minecraft version {}", raw_version))?;
        match setting.source_type {
            SourceType::None => {
                anyhow::bail!("source_type cound not be SourceType::None")
            }
            SourceType::Archive => {
                let factory_ctor = Self::get_ctor(
                    setting.config.instance_type.clone(),
                    minecraft_version,
                    self.archive_factories.as_slice(),
                )
                .ok_or(anyhow::anyhow!("could not find archive factory"))?;
                let mut factory = factory_ctor();
                factory.install(setting).await
            }
            SourceType::Core => {
                let factory_ctor = Self::get_ctor(
                    setting.config.instance_type.clone(),
                    minecraft_version,
                    self.core_factories.as_slice(),
                )
                .ok_or(anyhow::anyhow!("could not find core factory"))?;
                let mut factory = factory_ctor();
                factory.install(setting).await
            }
            SourceType::Script => {
                let factory_ctor = Self::get_ctor(
                    setting.config.instance_type.clone(),
                    minecraft_version,
                    self.script_factories.as_slice(),
                )
                .ok_or(anyhow::anyhow!("could not find script factory"))?;
                let mut factory = factory_ctor();
                factory.install(setting).await
            }
        }
    }
}
