mod config;
mod protocol;
pub mod v1;
use serde::{Deserialize, Serialize};

pub use config::ProtocolConfig;
pub use protocol::Protocol;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Protocols {
    V1,
    V2,
    Set(u8),
}

impl Protocols {
    pub fn is_enabled(&self, protocol: Protocols) -> bool {
        self.to_bitflag() & protocol.to_bitflag() != 0
    }

    pub fn to_bitflag(self) -> u8 {
        match self {
            Protocols::V1 => 0b00000001,
            Protocols::V2 => 0b00000010,
            Protocols::Set(bitflag) => bitflag,
        }
    }

    pub fn combine(protocols: &[Protocols]) -> Protocols {
        let bit = protocols.iter().fold(0, |a, b| a | b.to_bitflag());
        Protocols::Set(bit)
    }
}
