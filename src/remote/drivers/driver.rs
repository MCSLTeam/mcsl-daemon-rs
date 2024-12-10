use std::sync::Arc;

use tokio::sync::Notify;

use super::Drivers;

pub type StopToken = Arc<Notify>;

#[async_trait::async_trait]
pub trait Driver: Send + Sync {
    async fn run(&self) -> ();
    fn stop_token(&self) -> Arc<Notify>;

    fn get_driver_type(&self) -> Drivers;
}
