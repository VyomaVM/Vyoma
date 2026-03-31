pub mod sender;
pub mod receiver;
pub mod protocol;

pub use sender::{MigrationSender, MigrationStats, MigrationSignal, MigrationHeader};
pub use receiver::MigrationReceiver;
pub use protocol::{MigrationMessage, MessageType, MigrationRequest, MigrationResponse};

pub const DEFAULT_MIGRATION_PORT: u16 = 9000;
pub const PAGE_SIZE: u64 = 4096;
pub const DEFAULT_BANDWIDTH_MBPS: u32 = 1000;
