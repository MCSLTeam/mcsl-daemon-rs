use super::Drivers;
use serde::{Deserialize, Serialize};

use super::capnproto::CapnprotoDriverConfig;
use super::websocket::WsDriverConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriversConfig {
    pub enabled: Vec<Drivers>,

    pub websocket_driver_config: WsDriverConfig,
    pub capnproto_driver_config: CapnprotoDriverConfig,
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

use std::net::{IpAddr, Ipv4Addr};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UniDriverConfig {
    pub port: u16,
    pub host: IpAddr,
}

impl Default for UniDriverConfig {
    fn default() -> Self {
        Self {
            host: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            port: 11452,
        }
    }
}
