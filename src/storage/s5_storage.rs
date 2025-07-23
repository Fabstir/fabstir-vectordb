use async_trait::async_trait;
use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use sha2::{Sha256, Digest};
use chrono::Utc;

use crate::core::storage::{S5Storage as Storage, StorageError};

#[derive(Debug, Clone)]
pub struct S5Config {
    pub node_url: String,
    pub api_key: Option<String>,
    pub enable_compression: bool,
    pub cache_size: usize,
}

impl Default for S5Config {
    fn default() -> Self {
        Self {
            node_url: "http://localhost:5050".to_string(),
            api_key: None,
            enable_compression: false,
            cache_size: 100,
        }
    }
}

#[derive(Debug, Clone)]
pub struct StorageMetadata {
    pub key: String,
    pub cid: String,
    pub size: usize,
    pub created_at: i64,
    pub compressed: bool,
}

pub struct S5Storage {
    config: S5Config,
    // Mock storage - replace with S5Client later
    storage: Arc<RwLock<HashMap<String, Vec<u8>>>>,
    cid_map: Arc<RwLock<HashMap<String, String>>>,
    metadata_map: Arc<RwLock<HashMap<String, StorageMetadata>>>,
}

impl S5Storage {
    pub fn new(config: S5Config) -> Self {
        Self {
            config,
            storage: Arc::new(RwLock::new(HashMap::new())),
            cid_map: Arc::new(RwLock::new(HashMap::new())),
            metadata_map: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn is_connected(&self) -> bool {
        // For now, always return true (mock)
        // Later: check if we're connected to an invalid URL
        !self.config.node_url.contains("invalid-url")
    }

    pub async fn get_cid(&self, key: &str) -> Result<String> {
        let cid_map = self.cid_map.read().await;
        cid_map.get(key)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("CID not found for key: {}", key))
    }

    pub async fn get_by_cid(&self, cid: &str) -> Result<Vec<u8>> {
        // Validate CID format
        if !cid.starts_with("s5://") {
            return Err(anyhow::anyhow!("Invalid CID format: {}", cid));
        }

        // Find the key for this CID
        let cid_map = self.cid_map.read().await;
        let key = cid_map.iter()
            .find(|(_, v)| v.as_str() == cid)
            .map(|(k, _)| k.clone())
            .ok_or_else(|| anyhow::anyhow!("Data not found for CID: {}", cid))?;

        // Get the data
        let storage = self.storage.read().await;
        storage.get(&key)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Data not found for CID: {}", cid))
    }

    pub async fn put_compressed(&self, key: &str, value: Vec<u8>) -> Result<()> {
        // Compress the data
        let compressed = zstd::encode_all(&value[..], 3)?;
        
        // Generate CID
        let cid = generate_mock_cid(&compressed);
        
        // Store the compressed data
        let mut storage = self.storage.write().await;
        storage.insert(key.to_string(), compressed.clone());
        
        // Update CID map
        let mut cid_map = self.cid_map.write().await;
        cid_map.insert(key.to_string(), cid.clone());
        
        // Update metadata
        let mut metadata_map = self.metadata_map.write().await;
        metadata_map.insert(key.to_string(), StorageMetadata {
            key: key.to_string(),
            cid,
            size: compressed.len(),
            created_at: Utc::now().timestamp(),
            compressed: true,
        });
        
        Ok(())
    }

    pub async fn get_metadata(&self, key: &str) -> Result<StorageMetadata> {
        let metadata_map = self.metadata_map.read().await;
        metadata_map.get(key)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Metadata not found for key: {}", key))
    }

    pub async fn batch_put(&self, items: Vec<(String, Vec<u8>)>) -> Result<Vec<Result<()>>> {
        let mut results = Vec::new();
        
        for (key, data) in items {
            let result = self.put(&key, data).await
                .map_err(|e| anyhow::anyhow!("Failed to put {}: {}", key, e));
            results.push(result);
        }
        
        Ok(results)
    }

    pub async fn batch_get(&self, keys: &[String]) -> Result<Vec<Result<Vec<u8>>>> {
        let mut results = Vec::new();
        
        for key in keys {
            let result = self.get(key).await
                .and_then(|opt| opt.ok_or_else(|| StorageError::NetworkError(format!("Key not found: {}", key))))
                .map_err(|e| anyhow::anyhow!("Failed to get {}: {}", key, e));
            results.push(result);
        }
        
        Ok(results)
    }
}

#[async_trait]
impl Storage for S5Storage {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, StorageError> {
        // Check for network error simulation
        if self.config.node_url.contains("invalid-url") {
            return Err(StorageError::NetworkError("Network error: Failed to connect".to_string()));
        }

        let storage = self.storage.read().await;
        
        // Check if data exists
        if let Some(data) = storage.get(key) {
            // Check if it was compressed
            let metadata_map = self.metadata_map.read().await;
            if let Some(metadata) = metadata_map.get(key) {
                if metadata.compressed {
                    // Decompress
                    let decompressed = zstd::decode_all(&data[..])
                        .map_err(|e| StorageError::SerializationError(format!("Decompression failed: {}", e)))?;
                    return Ok(Some(decompressed));
                }
            }
            Ok(Some(data.clone()))
        } else {
            Ok(None)
        }
    }

    async fn put(&self, key: &str, value: Vec<u8>) -> Result<(), StorageError> {
        // Check for network error simulation
        if self.config.node_url.contains("invalid-url") {
            return Err(StorageError::NetworkError("Network error: Failed to connect".to_string()));
        }

        // Generate CID
        let cid = generate_mock_cid(&value);
        
        // Store the data
        let mut storage = self.storage.write().await;
        storage.insert(key.to_string(), value.clone());
        
        // Update CID map
        let mut cid_map = self.cid_map.write().await;
        cid_map.insert(key.to_string(), cid.clone());
        
        // Update metadata
        let mut metadata_map = self.metadata_map.write().await;
        metadata_map.insert(key.to_string(), StorageMetadata {
            key: key.to_string(),
            cid,
            size: value.len(),
            created_at: Utc::now().timestamp(),
            compressed: false,
        });
        
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<(), StorageError> {
        let mut storage = self.storage.write().await;
        storage.remove(key);
        
        let mut cid_map = self.cid_map.write().await;
        cid_map.remove(key);
        
        let mut metadata_map = self.metadata_map.write().await;
        metadata_map.remove(key);
        
        Ok(())
    }

    async fn list(&self, prefix: &str) -> Result<Vec<String>, StorageError> {
        let storage = self.storage.read().await;
        Ok(storage.keys()
            .filter(|k| k.starts_with(prefix))
            .cloned()
            .collect())
    }
}

// Helper functions for tests
impl S5Storage {
    pub async fn exists(&self, key: &str) -> Result<bool> {
        let storage = self.storage.read().await;
        Ok(storage.contains_key(key))
    }
    
    pub async fn list_keys(&self, prefix: &str) -> Result<Vec<String>> {
        self.list(prefix).await
            .map_err(|e| anyhow::anyhow!("Failed to list keys: {}", e))
    }
}

fn generate_mock_cid(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let hash = hasher.finalize();
    format!("s5://mock_{}", hex::encode(&hash[..8]))
}