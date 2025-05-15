use super::Drivers;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

use super::websocket::WsDriverConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriversConfig {
    pub enabled: Cow<'static, [Drivers]>,

    pub websocket_driver_config: WsDriverConfig,
}
impl Default for DriversConfig {
    fn default() -> Self {
        Self {
            enabled: Cow::Borrowed(&[Drivers::Websocket]),

            websocket_driver_config: WsDriverConfig::default(),
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
            host: IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)),
            port: 11452,
        }
    }
}
