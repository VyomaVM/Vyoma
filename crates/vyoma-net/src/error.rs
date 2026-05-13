use thiserror::Error;
use nix::Error as NixError;
use std::ffi::NulError;

#[derive(Error, Debug)]
pub enum NetworkError {
    #[error("Netlink error: {0}")]
    Netlink(String),
     
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Nix error: {0}")]
    Nix(#[from] NixError),
    
    #[error("Nul error: {0}")]
    Nul(#[from] NulError),
     
    #[error("Not found: {0}")]
    NotFound(String),
     
    #[error("Already exists: {0}")]
    AlreadyExists(String),
     
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
     
    #[error("Invalid input: {0}")]
    InvalidInput(String),
}

pub type Result<T> = std::result::Result<T, NetworkError>;
