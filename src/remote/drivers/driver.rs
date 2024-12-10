use std::sync::Arc;

use tokio::sync::Notify;

pub type StopToken = Arc<Notify>;

#[async_trait::async_trait]
pub trait Driver: Send + Sync {
    async fn run(&self) -> ();
    fn stop_token(&self) -> Arc<Notify>;

    fn set_protocol_set(&mut self, set: u8);
    fn protocol_set(&self) -> u8;
    fn get_driver_type(&self) -> &'static str;
}
