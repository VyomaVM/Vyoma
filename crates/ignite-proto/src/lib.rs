pub mod vm_service;
pub mod server;

pub use vm_service::*;
pub use server::IgniteGrpcServer;

pub const DEFAULT_GRPC_PORT: u16 = 50051;
