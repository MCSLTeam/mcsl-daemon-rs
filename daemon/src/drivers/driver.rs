use super::Drivers;

#[async_trait::async_trait]
pub trait Driver: Send + Sync {
    async fn run(&self) -> ();

    fn get_driver_type(&self) -> Drivers;
}
