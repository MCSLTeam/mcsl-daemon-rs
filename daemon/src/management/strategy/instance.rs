use crate::management::instance::Instance;
use mcsl_protocol::management::instance::InstanceReport;

#[async_trait::async_trait]
pub trait InstanceStrategy {
    async fn get_report(&self, this: &Instance) -> InstanceReport;
    async fn stop(&self, this: &Instance) -> anyhow::Result<()>;
}
