use thiserror::Error;

#[derive(Error, Debug)]
pub enum StorageError {
    #[error("Device mapper error: {0}")]
    DeviceMapper(String),
    
    #[error("Loop device error: {0}")]
    LoopDevice(String),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("Sled error: {0}")]
    Sled(#[from] sled::Error),
    
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    
    #[error("Path error: {0}")]
    Path(String),
    
    #[error("Not found: {0}")]
    NotFound(String),
    
    #[error("Already exists: {0}")]
    AlreadyExists(String),
    
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
}

pub type Result<T> = std::result::Result<T, StorageError>;
