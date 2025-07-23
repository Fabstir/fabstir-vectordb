pub mod s5_storage;

pub use s5_storage::{S5Config, S5Storage, StorageMetadata};

// Re-export the Storage trait from core
pub use crate::core::storage::S5Storage as Storage;