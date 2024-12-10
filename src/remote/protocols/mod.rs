mod protocol;
pub mod v1;
pub const V1: u8 = 0b00000001;
pub const V2: u8 = 0b00000010;

pub use protocol::Protocol;
