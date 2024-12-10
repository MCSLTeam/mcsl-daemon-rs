pub mod capnproto;
mod config;
mod driver;
mod graceful_shutdown;
pub mod websocket;
pub use driver::Driver;
pub use graceful_shutdown::GracefulShutdown;
use serde::{Deserialize, Serialize};

pub use config::{DriversConfig, UniDriverConfig};
pub use websocket::WsDriverBuilder;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Drivers {
    Websocket,
    Capnproto,
}
