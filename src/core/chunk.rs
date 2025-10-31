// Copyright (c) 2025 Fabstir
// SPDX-License-Identifier: BUSL-1.1

/// Chunk types for chunked vector storage with lazy loading
use crate::core::types::VectorId;
use crate::core::schema::MetadataSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ChunkError {
    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Deserialization error: {0}")]
    Deserialization(String),

    #[error("Invalid version: expected {expected}, found {found}")]
    InvalidVersion { expected: u32, found: u32 },

    #[error("Chunk overlap detected: {0}")]
    ChunkOverlap(String),

    #[error("Invalid chunk range: start={start}, end={end}")]
    InvalidRange { start: usize, end: usize },
}

/// Current manifest version
pub const MANIFEST_VERSION: u32 = 3;

// ============================================================================
// VectorChunk - Storage unit for vectors
// ============================================================================

/// A chunk of vectors (typically 10K vectors)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorChunk {
    pub chunk_id: String,
    pub start_idx: usize,
    pub end_idx: usize,
    pub vectors: HashMap<VectorId, Vec<f32>>,
}

impl VectorChunk {
    /// Create a new empty vector chunk
    pub fn new(chunk_id: String, start_idx: usize, end_idx: usize) -> Self {
        Self {
            chunk_id,
            start_idx,
            end_idx,
            vectors: HashMap::new(),
        }
    }

    /// Add a vector to the chunk
    pub fn add_vector(&mut self, id: VectorId, vector: Vec<f32>) {
        self.vectors.insert(id, vector);
    }

    /// Get a vector from the chunk
    pub fn get_vector(&self, id: &VectorId) -> Option<&Vec<f32>> {
        self.vectors.get(id)
    }

    /// Check if this chunk overlaps with another
    pub fn overlaps_with(&self, other: &VectorChunk) -> bool {
        // Check if ranges overlap
        let self_range = self.start_idx..=self.end_idx;
        let other_range = other.start_idx..=other.end_idx;

        self_range.contains(&other.start_idx)
            || self_range.contains(&other.end_idx)
            || other_range.contains(&self.start_idx)
            || other_range.contains(&self.end_idx)
    }

    /// Serialize to CBOR
    pub fn to_cbor(&self) -> Result<Vec<u8>, ChunkError> {
        serde_cbor::to_vec(self).map_err(|e| ChunkError::Serialization(e.to_string()))
    }

    /// Deserialize from CBOR
    pub fn from_cbor(data: &[u8]) -> Result<Self, ChunkError> {
        serde_cbor::from_slice(data).map_err(|e| ChunkError::Deserialization(e.to_string()))
    }

    /// Get the number of vectors in this chunk
    pub fn len(&self) -> usize {
        self.vectors.len()
    }

    /// Check if the chunk is empty
    pub fn is_empty(&self) -> bool {
        self.vectors.is_empty()
    }
}

// ============================================================================
// ChunkMetadata - Metadata about a chunk
// ============================================================================

/// Metadata for a vector chunk
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkMetadata {
    pub chunk_id: String,
    pub cid: Option<String>, // S5 CID after upload
    pub vector_count: usize,
    pub byte_size: usize,
    pub vector_id_range: (VectorId, VectorId), // (start, end)
}

impl ChunkMetadata {
    /// Create new chunk metadata
    pub fn new(
        chunk_id: String,
        vector_count: usize,
        byte_size: usize,
        start_id: VectorId,
        end_id: VectorId,
    ) -> Self {
        Self {
            chunk_id,
            cid: None,
            vector_count,
            byte_size,
            vector_id_range: (start_id, end_id),
        }
    }

    /// Set the S5 CID after upload
    pub fn set_cid(&mut self, cid: String) {
        self.cid = Some(cid);
    }

    /// Serialize to CBOR
    pub fn to_cbor(&self) -> Result<Vec<u8>, ChunkError> {
        serde_cbor::to_vec(self).map_err(|e| ChunkError::Serialization(e.to_string()))
    }

    /// Deserialize from CBOR
    pub fn from_cbor(data: &[u8]) -> Result<Self, ChunkError> {
        serde_cbor::from_slice(data).map_err(|e| ChunkError::Deserialization(e.to_string()))
    }
}

// ============================================================================
// HNSW Manifest - Graph structure without vectors
// ============================================================================

/// Metadata for a layer in the HNSW graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerMetadata {
    pub layer_id: usize,
    pub node_count: usize,
}

/// HNSW graph structure in the manifest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HNSWManifest {
    pub entry_point: VectorId,
    pub layers: Vec<LayerMetadata>,
    pub node_chunk_map: HashMap<String, String>, // node_id_string -> chunk_id
}

impl HNSWManifest {
    /// Create a new HNSW manifest
    pub fn new(entry_point: VectorId) -> Self {
        Self {
            entry_point,
            layers: Vec::new(),
            node_chunk_map: HashMap::new(),
        }
    }

    /// Add a layer
    pub fn add_layer(&mut self, layer_id: usize, node_count: usize) {
        self.layers.push(LayerMetadata {
            layer_id,
            node_count,
        });
    }

    /// Map a node to its chunk
    pub fn add_node_chunk_mapping(&mut self, node_id: VectorId, chunk_id: String) {
        self.node_chunk_map.insert(node_id.to_string(), chunk_id);
    }

    /// Get the chunk ID for a node
    pub fn get_chunk_for_node(&self, node_id: &VectorId) -> Option<&String> {
        self.node_chunk_map.get(&node_id.to_string())
    }
}

// ============================================================================
// IVF Manifest - Centroids and cluster mappings
// ============================================================================

/// IVF index structure in the manifest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IVFManifest {
    pub centroids: Vec<Vec<f32>>, // Keep centroids in memory
    pub cluster_assignments: HashMap<usize, Vec<String>>, // cluster_id -> [chunk_ids]
}

impl IVFManifest {
    /// Create a new IVF manifest
    pub fn new(centroids: Vec<Vec<f32>>) -> Self {
        Self {
            centroids,
            cluster_assignments: HashMap::new(),
        }
    }

    /// Assign a cluster to chunks
    pub fn add_cluster_assignment(&mut self, cluster_id: usize, chunk_ids: Vec<String>) {
        self.cluster_assignments.insert(cluster_id, chunk_ids);
    }

    /// Get chunks for a cluster
    pub fn get_chunks_for_cluster(&self, cluster_id: usize) -> Option<&Vec<String>> {
        self.cluster_assignments.get(&cluster_id)
    }

    /// Get the number of centroids
    pub fn num_centroids(&self) -> usize {
        self.centroids.len()
    }
}

// ============================================================================
// Manifest - Top-level index manifest
// ============================================================================

/// Top-level manifest for chunked vector index
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub version: u32,
    pub chunk_size: usize, // Vectors per chunk (default: 10000)
    pub total_vectors: usize,
    pub chunks: Vec<ChunkMetadata>,
    pub hnsw_structure: Option<HNSWManifest>,
    pub ivf_structure: Option<IVFManifest>,

    /// List of soft-deleted vector IDs (v3+)
    /// These vectors are marked as deleted but not physically removed
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deleted_vectors: Option<Vec<String>>,

    /// Optional metadata schema for validation (v3+)
    /// If present, all metadata operations will be validated against this schema
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<MetadataSchema>,
}

impl Manifest {
    /// Create a new manifest
    pub fn new(chunk_size: usize, total_vectors: usize) -> Self {
        Self {
            version: MANIFEST_VERSION,
            chunk_size,
            total_vectors,
            chunks: Vec::new(),
            hnsw_structure: None,
            ivf_structure: None,
            deleted_vectors: None,
            schema: None,
        }
    }

    /// Add a chunk to the manifest
    pub fn add_chunk(&mut self, chunk: ChunkMetadata) {
        self.chunks.push(chunk);
    }

    /// Set the HNSW structure
    pub fn set_hnsw_structure(&mut self, hnsw: HNSWManifest) {
        self.hnsw_structure = Some(hnsw);
    }

    /// Set the IVF structure
    pub fn set_ivf_structure(&mut self, ivf: IVFManifest) {
        self.ivf_structure = Some(ivf);
    }

    /// Serialize to JSON
    pub fn to_json(&self) -> Result<String, ChunkError> {
        serde_json::to_string_pretty(self).map_err(|e| ChunkError::Serialization(e.to_string()))
    }

    /// Deserialize from JSON
    pub fn from_json(json: &str) -> Result<Self, ChunkError> {
        let manifest: Manifest =
            serde_json::from_str(json).map_err(|e| ChunkError::Deserialization(e.to_string()))?;

        // Validate version
        if manifest.version > MANIFEST_VERSION {
            return Err(ChunkError::InvalidVersion {
                expected: MANIFEST_VERSION,
                found: manifest.version,
            });
        }

        Ok(manifest)
    }

    /// Validate the manifest (check for overlaps, etc.)
    pub fn validate(&self) -> Result<(), ChunkError> {
        // Check for chunk overlaps in vector ID ranges
        for i in 0..self.chunks.len() {
            for j in (i + 1)..self.chunks.len() {
                let chunk_i = &self.chunks[i];
                let chunk_j = &self.chunks[j];

                // For now, just check that chunk IDs are unique
                if chunk_i.chunk_id == chunk_j.chunk_id {
                    return Err(ChunkError::ChunkOverlap(format!(
                        "Duplicate chunk ID: {}",
                        chunk_i.chunk_id
                    )));
                }
            }
        }

        Ok(())
    }

    /// Get the number of chunks
    pub fn num_chunks(&self) -> usize {
        self.chunks.len()
    }

    /// Get a chunk by ID
    pub fn get_chunk(&self, chunk_id: &str) -> Option<&ChunkMetadata> {
        self.chunks.iter().find(|c| c.chunk_id == chunk_id)
    }

    /// Get all chunk IDs
    pub fn get_chunk_ids(&self) -> Vec<String> {
        self.chunks.iter().map(|c| c.chunk_id.clone()).collect()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manifest_version() {
        let manifest = Manifest::new(10000, 0);
        assert_eq!(manifest.version, MANIFEST_VERSION);
    }

    #[test]
    fn test_chunk_error_display() {
        let error = ChunkError::InvalidVersion {
            expected: 2,
            found: 3,
        };
        assert!(error.to_string().contains("expected 2"));
    }

    #[test]
    fn test_vector_chunk_basic_operations() {
        let mut chunk = VectorChunk::new("test".to_string(), 0, 999);
        let id = VectorId::from_string("vec1");
        let vector = vec![1.0, 2.0, 3.0];

        chunk.add_vector(id.clone(), vector.clone());

        assert_eq!(chunk.len(), 1);
        assert!(!chunk.is_empty());
        assert_eq!(chunk.get_vector(&id), Some(&vector));
    }
}
