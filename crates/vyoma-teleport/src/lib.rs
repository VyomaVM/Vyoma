pub mod sender;
pub mod receiver;

pub use sender::Teleporter;
pub use receiver::TeleportReceiver;

pub const PAGE_SIZE: u64 = 4096;
