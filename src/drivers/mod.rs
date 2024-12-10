pub mod capnproto;
mod config;
mod driver;
mod graceful_shutdown;
pub mod websocket;
use crate::app::AppResources;
use crate::drivers::websocket::WsDriver;
pub use driver::Driver;
pub use graceful_shutdown::GracefulShutdown;
use serde::{Deserialize, Serialize};

pub use config::{DriversConfig, UniDriverConfig};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Drivers {
    Websocket,
    Capnproto,
}

impl Drivers {
    pub fn new_driver(&self, resources: AppResources) -> impl Driver {
        match self {
            Drivers::Websocket => WsDriver::new(resources),
            Drivers::Capnproto => unimplemented!(),
        }
    }
}
