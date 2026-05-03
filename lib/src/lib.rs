pub mod channels;
pub mod client;
pub mod codecs;
pub mod messages;

pub const DLIVE_MIXRACK_TCP_PORT: u16 = 51325;
pub const DLIVE_SURFACE_TCP_PORT: u16 = 51328;
/// Non-standard port. Used specifically for the `fake-dlive` crate.
pub const DLIVE_FAKE_TCP_PORT: u16 = 51331;
