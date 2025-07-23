pub mod s5_storage;
pub mod s5_client;

pub use s5_storage::{S5Config, S5Storage, StorageMetadata};
pub use s5_client::{S5Client, DirectoryEntry, PathResponse, UploadResponse, BatchResult};

// Re-export the Storage trait from core
pub use crate::core::storage::S5Storage as Storage;