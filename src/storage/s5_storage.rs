// Copyright (c) 2025 Fabstir
// SPDX-License-Identifier: BUSL-1.1

use async_trait::async_trait;
use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use chrono::Utc;

use crate::core::storage::{S5Storage as Storage, StorageError};
use crate::storage::s5_client::S5Client;

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
    client: S5Client,
    cid_map: Arc<RwLock<HashMap<String, String>>>, // Keep for key->CID mapping
    metadata_map: Arc<RwLock<HashMap<String, StorageMetadata>>>,
}

impl S5Storage {
    pub fn new(config: S5Config) -> Self {
        let client = S5Client::new(config.clone());
        Self {
            config,
            client,
            cid_map: Arc::new(RwLock::new(HashMap::new())),
            metadata_map: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn is_connected(&self) -> bool {
        // Check if we can connect to the S5 node
        self.client.health_check().await.is_ok()
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

        // Download data from S5 using CID
        let data = self.client.download_data(cid).await?;
        
        // Check if data is compressed by looking up metadata
        let cid_map = self.cid_map.read().await;
        if let Some((key, _)) = cid_map.iter().find(|(_, v)| v.as_str() == cid) {
            let metadata_map = self.metadata_map.read().await;
            if let Some(metadata) = metadata_map.get(key) {
                if metadata.compressed {
                    // Decompress
                    let decompressed = zstd::decode_all(&data[..])
                        .map_err(|e| anyhow::anyhow!("Decompression failed: {}", e))?;
                    return Ok(decompressed);
                }
            }
        }
        
        Ok(data)
    }

    pub async fn put_compressed(&self, key: &str, value: Vec<u8>) -> Result<()> {
        // Compress the data
        let compressed = zstd::encode_all(&value[..], 3)?;
        
        // Upload to S5
        let cid = self.client.upload_data(compressed.clone()).await?;
        
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
        // Get CID for the key
        let cid_map = self.cid_map.read().await;
        let cid = match cid_map.get(key) {
            Some(cid) => cid.clone(),
            None => return Ok(None), // Key doesn't exist
        };

        // Download data from S5
        let data = self.client.download_data(&cid).await
            .map_err(|e| {
                if e.to_string().contains("404") || e.to_string().contains("not found") {
                    StorageError::NetworkError(format!("Key not found: {}", key))
                } else {
                    StorageError::NetworkError(format!("Failed to download: {}", e))
                }
            })?;

        // Check if data was compressed
        let metadata_map = self.metadata_map.read().await;
        if let Some(metadata) = metadata_map.get(key) {
            if metadata.compressed {
                // Decompress
                let decompressed = zstd::decode_all(&data[..])
                    .map_err(|e| StorageError::SerializationError(format!("Decompression failed: {}", e)))?;
                return Ok(Some(decompressed));
            }
        }
        
        Ok(Some(data))
    }

    async fn put(&self, key: &str, value: Vec<u8>) -> Result<(), StorageError> {
        // Upload data to S5
        let cid = self.client.upload_data(value.clone()).await
            .map_err(|e| StorageError::NetworkError(format!("Upload failed: {}", e)))?;
        
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
        // Note: S5 is immutable, so we can't actually delete data
        // We just remove from our local maps
        let mut cid_map = self.cid_map.write().await;
        cid_map.remove(key);
        
        let mut metadata_map = self.metadata_map.write().await;
        metadata_map.remove(key);
        
        Ok(())
    }

    async fn list(&self, prefix: &str) -> Result<Vec<String>, StorageError> {
        // List from our local CID map since S5 doesn't have direct key listing
        let cid_map = self.cid_map.read().await;
        Ok(cid_map.keys()
            .filter(|k| k.starts_with(prefix))
            .cloned()
            .collect())
    }
}

// Helper functions for tests
impl S5Storage {
    pub async fn exists(&self, key: &str) -> Result<bool> {
        let cid_map = self.cid_map.read().await;
        Ok(cid_map.contains_key(key))
    }
    
    pub async fn list_keys(&self, prefix: &str) -> Result<Vec<String>> {
        self.list(prefix).await
            .map_err(|e| anyhow::anyhow!("Failed to list keys: {}", e))
    }
}

