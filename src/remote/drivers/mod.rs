mod capnproto_driver;
mod driver;
mod graceful_shutdown;
mod ws_driver;
pub use driver::Driver;
pub use graceful_shutdown::GracefulShutdown;
pub use ws_driver::WsDriverBuilder;

use capnproto_driver::CapnprotoDriverConfig;
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr};
use ws_driver::WsDriverConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriversConfig {
    pub enabled: Vec<Drivers>,

    pub websocket_driver_config: WsDriverConfig,
    pub capnproto_driver_config: CapnprotoDriverConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UniDriverConfig {
    pub port: u16,
    pub host: IpAddr,
    pub max_parallel_requests: u16,
    pub file_download_sessions: u8,
}

impl Default for UniDriverConfig {
    fn default() -> Self {
        Self {
            host: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            port: 11452,
            max_parallel_requests: 256,
            file_download_sessions: 3,
        }
    }
}

impl Default for DriversConfig {
    fn default() -> Self {
        Self {
            enabled: vec![Drivers::Websocket],

            websocket_driver_config: WsDriverConfig::default(),
            capnproto_driver_config: CapnprotoDriverConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Drivers {
    Websocket,
    Capnproto,
}
