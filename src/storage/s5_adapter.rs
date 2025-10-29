// Copyright (c) 2025 Fabstir
// SPDX-License-Identifier: BUSL-1.1

use async_trait::async_trait;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::error::Error;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StorageMode {
    Mock,
    Real,
}

#[derive(Debug, Clone, PartialEq)]
pub struct S5StorageConfig {
    pub mode: StorageMode,
    pub mock_server_url: Option<String>,
    pub portal_url: Option<String>,
    pub seed_phrase: Option<String>,
    pub connection_timeout: Option<u64>,
    pub retry_attempts: Option<u32>,
    /// Enable encryption at rest (default: true)
    /// When enabled, adds X-S5-Encryption header for xchacha20-poly1305 encryption
    pub encrypt_at_rest: Option<bool>,
}

#[async_trait]
pub trait S5StorageAdapter: Send + Sync {
    async fn put_raw(&self, key: &str, data: Vec<u8>) -> Result<(), Box<dyn Error + Send + Sync>>;
    async fn get_raw(&self, key: &str) -> Result<Vec<u8>, Box<dyn Error + Send + Sync>>;
    async fn delete(&self, key: &str) -> Result<(), Box<dyn Error + Send + Sync>>;
    async fn exists(&self, key: &str) -> Result<bool, Box<dyn Error + Send + Sync>>;
    async fn list(&self, prefix: &str) -> Result<Vec<String>, Box<dyn Error + Send + Sync>>;
    fn get_mode(&self) -> StorageMode;
    async fn is_connected(&self) -> bool;
    async fn get_stats(&self) -> Result<serde_json::Value, Box<dyn Error + Send + Sync>>;
    
    // Convenience methods with default implementations
    async fn put<T: Serialize + Send + Sync>(&self, key: &str, value: &T) -> Result<(), Box<dyn Error + Send + Sync>> {
        let data = serde_cbor::to_vec(value)?;
        self.put_raw(key, data).await
    }
    
    async fn get<T: DeserializeOwned>(&self, key: &str) -> Result<T, Box<dyn Error + Send + Sync>> {
        let data = self.get_raw(key).await?;
        Ok(serde_cbor::from_slice(&data)?)
    }
}

// Higher-level Storage trait that the tests expect
#[async_trait]
pub trait Storage: Send + Sync {
    async fn put<T: Serialize + Send + Sync>(&self, key: &str, value: &T) -> Result<(), Box<dyn Error + Send + Sync>>;
    async fn get<T: DeserializeOwned>(&self, key: &str) -> Result<T, Box<dyn Error + Send + Sync>>;
    async fn delete(&self, key: &str) -> Result<(), Box<dyn Error + Send + Sync>>;
    async fn exists(&self, key: &str) -> Result<bool, Box<dyn Error + Send + Sync>>;
    async fn list(&self, prefix: &str) -> Result<Vec<String>, Box<dyn Error + Send + Sync>>;
}

#[derive(Debug)]
pub struct StorageConfigError {
    message: String,
}

impl StorageConfigError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for StorageConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Storage configuration error: {}", self.message)
    }
}

impl Error for StorageConfigError {}