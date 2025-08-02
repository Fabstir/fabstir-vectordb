use async_trait::async_trait;
use serde::{Serialize, de::DeserializeOwned};
use std::error::Error;
use std::sync::Arc;
use tokio::sync::RwLock;
use std::collections::HashMap;
use reqwest::{Client, StatusCode};
use std::time::Duration;

use crate::storage::s5_adapter::{S5StorageAdapter, Storage, StorageMode, S5StorageConfig, StorageConfigError};
use crate::core::storage::{S5Storage as CoreS5Storage, StorageError as CoreStorageError};

#[derive(Clone)]
pub struct EnhancedS5Storage {
    config: S5StorageConfig,
    client: Client,
    base_url: String,
    cache: Arc<RwLock<HashMap<String, Vec<u8>>>>,
}

impl std::fmt::Debug for EnhancedS5Storage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EnhancedS5Storage")
            .field("config", &self.config)
            .field("base_url", &self.base_url)
            .field("cache_size", &self.cache.try_read().map(|c| c.len()).unwrap_or(0))
            .finish()
    }
}

impl EnhancedS5Storage {
    pub fn new(config: S5StorageConfig) -> Result<Self, Box<dyn Error + Send + Sync>> {
        // Validate configuration
        match config.mode {
            StorageMode::Mock => {
                if config.mock_server_url.is_none() {
                    return Err(Box::new(StorageConfigError::new(
                        "mock_server_url is required for Mock mode"
                    )));
                }
            }
            StorageMode::Real => {
                if config.portal_url.is_none() {
                    return Err(Box::new(StorageConfigError::new(
                        "portal_url is required for Real mode"
                    )));
                }
            }
        }

        let timeout = config.connection_timeout.unwrap_or(5000);
        let client = Client::builder()
            .timeout(Duration::from_millis(timeout))
            .build()?;

        let base_url = match config.mode {
            StorageMode::Mock => {
                let url = config.mock_server_url.as_ref().unwrap();
                // Handle Docker networking - check if we're inside a container
                if url.contains("localhost") {
                    // Check if running inside Docker by looking for .dockerenv or checking cgroup
                    let in_docker = std::path::Path::new("/.dockerenv").exists() ||
                        std::fs::read_to_string("/proc/1/cgroup")
                            .unwrap_or_default()
                            .contains("docker");
                    
                    if in_docker {
                        url.replace("localhost", "host.docker.internal")
                    } else {
                        url.clone()
                    }
                } else {
                    url.clone()
                }
            }
            StorageMode::Real => config.portal_url.as_ref().unwrap().clone(),
        };

        Ok(Self {
            config,
            client,
            base_url,
            cache: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    async fn retry_operation<F, Fut, T>(&self, operation: F) -> Result<T, Box<dyn Error + Send + Sync>>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<T, Box<dyn Error + Send + Sync>>>,
    {
        let max_retries = self.config.retry_attempts.unwrap_or(3);
        let mut last_error = None;

        for attempt in 0..max_retries {
            match operation().await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    last_error = Some(e);
                    if attempt < max_retries - 1 {
                        tokio::time::sleep(Duration::from_millis(100 * (attempt as u64 + 1))).await;
                    }
                }
            }
        }

        Err(last_error.unwrap())
    }

    fn get_storage_path(&self, key: &str) -> String {
        match self.config.mode {
            StorageMode::Mock => format!("/s5/fs/{}", key),
            StorageMode::Real => format!("/storage/{}", key),
        }
    }
}

#[async_trait]
impl S5StorageAdapter for EnhancedS5Storage {
    async fn put_raw(&self, key: &str, data: Vec<u8>) -> Result<(), Box<dyn Error + Send + Sync>> {
        let path = self.get_storage_path(key);
        let url = format!("{}{}", self.base_url, path);

        self.retry_operation(|| {
            let client = self.client.clone();
            let url = url.clone();
            let data = data.clone();
            async move {
                let response = client
                    .put(&url)
                    .body(data.clone())
                    .header("Content-Type", "application/cbor")
                    .send()
                    .await?;

                if response.status().is_success() {
                    Ok(())
                } else {
                    Err(Box::new(std::io::Error::new(
                        std::io::ErrorKind::Other, 
                        format!("PUT failed with status: {}", response.status())
                    )) as Box<dyn Error + Send + Sync>)
                }
            }
        }).await?;

        // Update cache
        let mut cache = self.cache.write().await;
        cache.insert(key.to_string(), data);

        Ok(())
    }

    async fn get_raw(&self, key: &str) -> Result<Vec<u8>, Box<dyn Error + Send + Sync>> {
        // Check cache first
        {
            let cache = self.cache.read().await;
            if let Some(data) = cache.get(key) {
                return Ok(data.clone());
            }
        }

        let path = self.get_storage_path(key);
        let url = format!("{}{}", self.base_url, path);

        let data = self.retry_operation(|| {
            let client = self.client.clone();
            let url = url.clone();
            async move {
                let response = client.get(&url).send().await?;

                if response.status() == StatusCode::NOT_FOUND {
                    return Err(Box::new(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        "Key not found"
                    )) as Box<dyn Error + Send + Sync>);
                }

                if !response.status().is_success() {
                    return Err(Box::new(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("GET failed with status: {}", response.status())
                    )) as Box<dyn Error + Send + Sync>);
                }

                Ok(response.bytes().await?.to_vec())
            }
        }).await?;

        // Update cache
        {
            let mut cache = self.cache.write().await;
            cache.insert(key.to_string(), data.clone());
        }

        Ok(data)
    }

    async fn delete(&self, key: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
        let path = self.get_storage_path(key);
        let url = format!("{}{}", self.base_url, path);

        self.retry_operation(|| {
            let client = self.client.clone();
            let url = url.clone();
            async move {
                let response = client.delete(&url).send().await?;

                if response.status().is_success() || response.status() == StatusCode::NOT_FOUND {
                    Ok(())
                } else {
                    Err(Box::new(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("DELETE failed with status: {}", response.status())
                    )) as Box<dyn Error + Send + Sync>)
                }
            }
        }).await?;

        // Remove from cache
        let mut cache = self.cache.write().await;
        cache.remove(key);

        Ok(())
    }

    async fn exists(&self, key: &str) -> Result<bool, Box<dyn Error + Send + Sync>> {
        // Check cache first
        {
            let cache = self.cache.read().await;
            if cache.contains_key(key) {
                return Ok(true);
            }
        }

        let path = self.get_storage_path(key);
        let url = format!("{}{}", self.base_url, path);

        let exists = self.retry_operation(|| {
            let client = self.client.clone();
            let url = url.clone();
            async move {
                let response = client.head(&url).send().await?;
                Ok(response.status().is_success())
            }
        }).await?;

        Ok(exists)
    }

    async fn list(&self, prefix: &str) -> Result<Vec<String>, Box<dyn Error + Send + Sync>> {
        let path = self.get_storage_path(prefix);
        let url = format!("{}{}", self.base_url, path);

        self.retry_operation(|| {
            let client = self.client.clone();
            let url = url.clone();
            async move {
                let response = client.get(&url).send().await?;

                if !response.status().is_success() {
                    return Err(Box::new(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("LIST failed with status: {}", response.status())
                    )) as Box<dyn Error + Send + Sync>);
                }

                let body = response.text().await?;
                // Parse the response - assuming it returns a JSON array of file names
                let files: Vec<String> = serde_json::from_str(&body)?;
                Ok(files)
            }
        }).await
    }

    fn get_mode(&self) -> StorageMode {
        self.config.mode
    }

    async fn is_connected(&self) -> bool {
        let test_url = match self.config.mode {
            StorageMode::Mock => format!("{}/health", self.base_url),
            StorageMode::Real => format!("{}/api/health", self.base_url),
        };

        match self.client.get(&test_url).send().await {
            Ok(response) => response.status().is_success(),
            Err(_) => false,
        }
    }

    async fn get_stats(&self) -> Result<serde_json::Value, Box<dyn Error + Send + Sync>> {
        let cache = self.cache.read().await;
        Ok(serde_json::json!({
            "mode": format!("{:?}", self.config.mode),
            "cache_entries": cache.len(),
            "base_url": self.base_url,
            "connected": self.is_connected().await,
        }))
    }
}

// Implement the high-level Storage trait
#[async_trait]
impl Storage for EnhancedS5Storage {
    async fn put<T: Serialize + Send + Sync>(&self, key: &str, value: &T) -> Result<(), Box<dyn Error + Send + Sync>> {
        <Self as S5StorageAdapter>::put(self, key, value).await
    }

    async fn get<T: DeserializeOwned>(&self, key: &str) -> Result<T, Box<dyn Error + Send + Sync>> {
        <Self as S5StorageAdapter>::get(self, key).await
    }

    async fn delete(&self, key: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
        <Self as S5StorageAdapter>::delete(self, key).await
    }

    async fn exists(&self, key: &str) -> Result<bool, Box<dyn Error + Send + Sync>> {
        <Self as S5StorageAdapter>::exists(self, key).await
    }

    async fn list(&self, prefix: &str) -> Result<Vec<String>, Box<dyn Error + Send + Sync>> {
        <Self as S5StorageAdapter>::list(self, prefix).await
    }
}

// Implement the core S5Storage trait for backward compatibility
#[async_trait]
impl CoreS5Storage for EnhancedS5Storage {
    async fn get(&self, path: &str) -> Result<Option<Vec<u8>>, CoreStorageError> {
        match S5StorageAdapter::exists(self, path).await {
            Ok(false) => Ok(None),
            Ok(true) => {
                // Get raw bytes
                let storage_path = self.get_storage_path(path);
                let url = format!("{}{}", self.base_url, storage_path);
                
                match self.client.get(&url).send().await {
                    Ok(response) if response.status().is_success() => {
                        match response.bytes().await {
                            Ok(bytes) => Ok(Some(bytes.to_vec())),
                            Err(e) => Err(CoreStorageError::NetworkError(e.to_string())),
                        }
                    }
                    Ok(response) if response.status() == StatusCode::NOT_FOUND => Ok(None),
                    Ok(response) => Err(CoreStorageError::NetworkError(
                        format!("GET failed with status: {}", response.status())
                    )),
                    Err(e) => Err(CoreStorageError::NetworkError(e.to_string())),
                }
            }
            Err(e) => Err(CoreStorageError::NetworkError(e.to_string())),
        }
    }

    async fn put(&self, path: &str, data: Vec<u8>) -> Result<(), CoreStorageError> {
        let storage_path = self.get_storage_path(path);
        let url = format!("{}{}", self.base_url, storage_path);

        match self.client
            .put(&url)
            .body(data)
            .send()
            .await
        {
            Ok(response) if response.status().is_success() => Ok(()),
            Ok(response) => Err(CoreStorageError::NetworkError(
                format!("PUT failed with status: {}", response.status())
            )),
            Err(e) => Err(CoreStorageError::NetworkError(e.to_string())),
        }
    }

    async fn delete(&self, path: &str) -> Result<(), CoreStorageError> {
        match S5StorageAdapter::delete(self, path).await {
            Ok(()) => Ok(()),
            Err(e) => Err(CoreStorageError::NetworkError(e.to_string())),
        }
    }

    async fn list(&self, prefix: &str) -> Result<Vec<String>, CoreStorageError> {
        match S5StorageAdapter::list(self, prefix).await {
            Ok(files) => Ok(files),
            Err(e) => Err(CoreStorageError::NetworkError(e.to_string())),
        }
    }
}