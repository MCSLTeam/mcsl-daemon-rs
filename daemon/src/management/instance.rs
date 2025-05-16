use mcsl_protocol::management::instance::{InstanceConfig, InstanceReport, InstanceStatus};
use std::any::Any;
use std::marker;

// Trait for instances
pub trait TInstance {}

// Example instance types
pub struct Universal;
pub struct Minecraft;

impl TInstance for Universal {}
impl TInstance for Minecraft {}

// Instance struct as provided
pub struct Instance<TInst>
where
    TInst: TInstance,
{
    config: InstanceConfig,
    _marker: marker::PhantomData<TInst>,
}

impl<TInst: TInstance> Instance<TInst> {
    pub fn new(config: InstanceConfig) -> Self {
        Instance {
            config,
            _marker: marker::PhantomData,
        }
    }

    // Example async method
    pub async fn do_work(&self) {
        // Simulate time-consuming operation
        tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
        println!("Work completed for {:?}", std::any::type_name::<TInst>());
    }
}

// Ensure Instance is Sync and Send
unsafe impl<TInst: TInstance> Sync for Instance<TInst> {}
unsafe impl<TInst: TInstance> Send for Instance<TInst> {}

// Trait to enable dynamic dispatch for instances
#[async_trait::async_trait]
pub trait UniversalInstance: Any + Send + Sync {
    fn as_any(&self) -> &dyn Any;

    fn get_config(&self) -> &InstanceConfig;

    fn get_status(&self) -> InstanceStatus;

    async fn get_report(&self) -> InstanceReport;

    async fn start(&self) -> anyhow::Result<()>;

    fn stop(&self) -> anyhow::Result<()>;
}

#[async_trait::async_trait]
impl<TInst: TInstance + 'static> UniversalInstance for Instance<TInst> {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn get_config(&self) -> &InstanceConfig {
        todo!()
    }

    fn get_status(&self) -> InstanceStatus {
        todo!()
    }

    async fn get_report(&self) -> InstanceReport {
        todo!()
    }

    async fn start(&self) -> anyhow::Result<()> {
        todo!()
    }

    fn stop(&self) -> anyhow::Result<()> {
        todo!()
    }
}
