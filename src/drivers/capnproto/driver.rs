use crate::drivers::{driver::StopToken, Driver, Drivers};

pub struct CapnprotoDriver {}

#[async_trait::async_trait]
impl Driver for CapnprotoDriver {
    async fn run(&self) -> () {
        todo!()
    }

    fn stop_token(&self) -> StopToken {
        todo!()
    }

    fn get_driver_type(&self) -> Drivers {
        Drivers::Capnproto
    }
}
