// Copyright (c) 2025 Fabstir
// SPDX-License-Identifier: BUSL-1.1

use crate::core::storage::S5Storage;
use crate::core::types::VectorId;
use crate::hnsw::core::{HNSWConfig, HNSWIndex, HNSWNode};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PersistenceError {
    #[error("Storage error: {0}")]
    StorageError(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("Deserialization error: {0}")]
    DeserializationError(String),

    #[error("Incompatible version: found {found}, expected {expected}")]
    IncompatibleVersion { found: u32, expected: u32 },

    #[error("Data integrity error: {0}")]
    IntegrityError(String),

    #[error("Recovery error: {0}")]
    RecoveryError(String),

    #[error("HNSW error: {0}")]
    HNSWError(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HNSWMetadata {
    pub version: u32,
    pub config: HNSWConfig,
    pub entry_point: Option<VectorId>,
    pub node_count: usize,
    pub dimension: Option<usize>,
}

impl HNSWMetadata {
    pub fn from_index(index: &HNSWIndex) -> Self {
        Self {
            version: 1,
            config: index.config().clone(),
            entry_point: index.entry_point(),
            node_count: index.node_count(),
            dimension: index.dimension(),
        }
    }

    pub fn to_cbor(&self) -> Result<Vec<u8>, PersistenceError> {
        serde_cbor::to_vec(self).map_err(|e| PersistenceError::SerializationError(e.to_string()))
    }

    pub fn from_cbor(data: &[u8]) -> Result<Self, PersistenceError> {
        serde_cbor::from_slice(data)
            .map_err(|e| PersistenceError::DeserializationError(e.to_string()))
    }
}

#[derive(Debug)]
pub struct RecoveryInfo {
    pub expected_nodes: usize,
    pub found_nodes: usize,
    pub missing_chunks: Vec<usize>,
}

pub struct HNSWPersister<S: S5Storage> {
    storage: S,
    chunk_size: usize,
}

impl<S: S5Storage> HNSWPersister<S> {
    pub fn new(storage: S) -> Self {
        Self {
            storage,
            chunk_size: 1000,
        }
    }

    pub fn with_chunk_size(storage: S, chunk_size: usize) -> Self {
        Self {
            storage,
            chunk_size,
        }
    }

    pub fn storage(&self) -> &S {
        &self.storage
    }

    pub async fn save_index(&self, index: &HNSWIndex, path: &str) -> Result<(), PersistenceError> {
        // Save metadata
        let metadata = HNSWMetadata::from_index(index);
        let metadata_path = format!("{}/metadata.cbor", path);
        self.storage
            .put(&metadata_path, metadata.to_cbor()?)
            .await
            .map_err(|e| PersistenceError::StorageError(e.to_string()))?;

        // Save nodes in chunks
        let all_nodes = index.get_all_nodes();
        let chunks = chunk_nodes(&all_nodes, self.chunk_size);

        for (chunk_id, chunk_data) in chunks {
            let chunk_path = format!("{}/nodes/chunk_{:04}.cbor", path, chunk_id);
            self.storage
                .put(&chunk_path, chunk_data)
                .await
                .map_err(|e| PersistenceError::StorageError(e.to_string()))?;
        }

        Ok(())
    }

    pub async fn load_index(&self, path: &str) -> Result<HNSWIndex, PersistenceError> {
        // Load metadata
        let metadata_path = format!("{}/metadata.cbor", path);
        let metadata_data = self
            .storage
            .get(&metadata_path)
            .await
            .map_err(|e| PersistenceError::StorageError(e.to_string()))?
            .ok_or_else(|| PersistenceError::StorageError("Metadata not found".to_string()))?;

        let metadata = HNSWMetadata::from_cbor(&metadata_data)?;

        // Check version compatibility
        if metadata.version != 1 {
            return Err(PersistenceError::IncompatibleVersion {
                found: metadata.version,
                expected: 1,
            });
        }

        // Create index with saved config
        let mut index = HNSWIndex::new(metadata.config.clone());

        // Load all node chunks
        let nodes_path = format!("{}/nodes/", path);
        let chunk_files = self
            .storage
            .list(&nodes_path)
            .await
            .map_err(|e| PersistenceError::StorageError(e.to_string()))?;

        let expected_chunks = (metadata.node_count + self.chunk_size - 1) / self.chunk_size;
        if chunk_files.len() < expected_chunks {
            return Err(PersistenceError::IntegrityError(format!(
                "Expected {} chunks, found {}",
                expected_chunks,
                chunk_files.len()
            )));
        }

        // Load each chunk
        for chunk_file in chunk_files {
            let chunk_data = self
                .storage
                .get(&chunk_file)
                .await
                .map_err(|e| PersistenceError::StorageError(e.to_string()))?
                .ok_or_else(|| PersistenceError::StorageError("Chunk not found".to_string()))?;

            let nodes = deserialize_node_chunk(&chunk_data)?;

            // Restore nodes to index
            for node in nodes {
                index
                    .restore_node(node)
                    .map_err(|e| PersistenceError::HNSWError(e.to_string()))?;
            }
        }

        // Restore entry point
        if let Some(entry_point) = metadata.entry_point {
            index.set_entry_point(entry_point);
        }

        Ok(index)
    }

    pub async fn save_incremental(
        &self,
        index: &HNSWIndex,
        path: &str,
        dirty_nodes: &HashMap<VectorId, HNSWNode>,
    ) -> Result<(), PersistenceError> {
        // Update metadata
        let metadata = HNSWMetadata::from_index(index);
        let metadata_path = format!("{}/metadata.cbor", path);
        self.storage
            .put(&metadata_path, metadata.to_cbor()?)
            .await
            .map_err(|e| PersistenceError::StorageError(e.to_string()))?;

        // Group dirty nodes by chunk
        let mut chunks_to_update: HashMap<usize, Vec<HNSWNode>> = HashMap::new();

        for (_, node) in dirty_nodes {
            let node_index = index.get_node_index(node.id()).unwrap_or(0);
            let chunk_id = node_index / self.chunk_size;
            chunks_to_update
                .entry(chunk_id)
                .or_insert_with(Vec::new)
                .push(node.clone());
        }

        // Update affected chunks
        for (chunk_id, nodes) in chunks_to_update {
            // Load existing chunk if it exists
            let chunk_path = format!("{}/nodes/chunk_{:04}.cbor", path, chunk_id);
            let existing_data = self.storage.get(&chunk_path).await.ok().flatten();

            let mut chunk_nodes = if let Some(data) = existing_data {
                deserialize_node_chunk(&data)?
            } else {
                Vec::new()
            };

            // Update or add nodes
            for new_node in nodes {
                chunk_nodes.retain(|n| n.id() != new_node.id());
                chunk_nodes.push(new_node);
            }

            // Save updated chunk
            let chunk_data = serialize_node_chunk(&chunk_nodes)?;
            self.storage
                .put(&chunk_path, chunk_data)
                .await
                .map_err(|e| PersistenceError::StorageError(e.to_string()))?;
        }

        Ok(())
    }

    pub async fn save_with_backup(
        &self,
        index: &HNSWIndex,
        prod_path: &str,
        backup_path: &str,
    ) -> Result<(), PersistenceError> {
        // First save to backup
        self.save_index(index, backup_path).await?;

        // Then save to production
        self.save_index(index, prod_path).await?;

        Ok(())
    }

    pub async fn restore_from_backup(
        &self,
        backup_path: &str,
        prod_path: &str,
    ) -> Result<(), PersistenceError> {
        // List all files in backup
        let metadata_src = format!("{}/metadata.cbor", backup_path);
        let metadata_dst = format!("{}/metadata.cbor", prod_path);

        // Copy metadata
        let metadata_data = self
            .storage
            .get(&metadata_src)
            .await
            .map_err(|e| PersistenceError::StorageError(e.to_string()))?
            .ok_or_else(|| {
                PersistenceError::StorageError("Backup metadata not found".to_string())
            })?;

        self.storage
            .put(&metadata_dst, metadata_data)
            .await
            .map_err(|e| PersistenceError::StorageError(e.to_string()))?;

        // Copy all node chunks
        let backup_nodes_path = format!("{}/nodes/", backup_path);
        let chunk_files = self
            .storage
            .list(&backup_nodes_path)
            .await
            .map_err(|e| PersistenceError::StorageError(e.to_string()))?;

        for chunk_file in chunk_files {
            let chunk_data = self
                .storage
                .get(&chunk_file)
                .await
                .map_err(|e| PersistenceError::StorageError(e.to_string()))?
                .ok_or_else(|| PersistenceError::StorageError("Chunk not found".to_string()))?;

            let dst_path = chunk_file.replace(backup_path, prod_path);
            self.storage
                .put(&dst_path, chunk_data)
                .await
                .map_err(|e| PersistenceError::StorageError(e.to_string()))?;
        }

        Ok(())
    }

    pub async fn check_integrity(&self, path: &str) -> Result<RecoveryInfo, PersistenceError> {
        // Load metadata
        let metadata_path = format!("{}/metadata.cbor", path);
        let metadata_data = self
            .storage
            .get(&metadata_path)
            .await
            .map_err(|e| PersistenceError::StorageError(e.to_string()))?
            .ok_or_else(|| PersistenceError::StorageError("Metadata not found".to_string()))?;

        let metadata = HNSWMetadata::from_cbor(&metadata_data)?;

        // Check chunks
        let nodes_path = format!("{}/nodes/", path);
        let _chunk_files = self
            .storage
            .list(&nodes_path)
            .await
            .map_err(|e| PersistenceError::StorageError(e.to_string()))?;

        let expected_chunks = (metadata.node_count + self.chunk_size - 1) / self.chunk_size;
        let mut found_nodes = 0;
        let mut missing_chunks = Vec::new();

        for chunk_id in 0..expected_chunks {
            let chunk_path = format!("{}/nodes/chunk_{:04}.cbor", path, chunk_id);
            if let Ok(Some(chunk_data)) = self.storage.get(&chunk_path).await {
                if let Ok(nodes) = deserialize_node_chunk(&chunk_data) {
                    found_nodes += nodes.len();
                } else {
                    missing_chunks.push(chunk_id);
                }
            } else {
                missing_chunks.push(chunk_id);
            }
        }

        Ok(RecoveryInfo {
            expected_nodes: metadata.node_count,
            found_nodes,
            missing_chunks,
        })
    }
}

// Helper functions
pub fn chunk_nodes(nodes: &[HNSWNode], chunk_size: usize) -> Vec<(usize, Vec<u8>)> {
    nodes
        .chunks(chunk_size)
        .enumerate()
        .map(|(i, chunk)| {
            let bytes = serialize_node_chunk(chunk).unwrap();
            (i, bytes)
        })
        .collect()
}

pub fn serialize_node_chunk(nodes: &[HNSWNode]) -> Result<Vec<u8>, PersistenceError> {
    serde_cbor::to_vec(&nodes).map_err(|e| PersistenceError::SerializationError(e.to_string()))
}

pub fn deserialize_node_chunk(data: &[u8]) -> Result<Vec<HNSWNode>, PersistenceError> {
    serde_cbor::from_slice(data).map_err(|e| PersistenceError::DeserializationError(e.to_string()))
}
