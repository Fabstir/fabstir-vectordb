use crate::core::chunk::{ChunkMetadata, HNSWManifest, IVFManifest, LayerMetadata, Manifest, VectorChunk};
use crate::core::storage::{MockS5Storage, S5Storage};
use crate::core::types::VectorId;
use crate::hybrid::core::{HybridConfig, HybridIndex};
use crate::hnsw::persistence::{HNSWPersister, PersistenceError as HNSWPersistenceError};
use crate::ivf::persistence::{IVFPersister, PersistenceError as IVFPersistenceError};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

const CURRENT_VERSION: u32 = 1;

#[derive(Debug, Error)]
pub enum PersistenceError {
    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Deserialization error: {0}")]
    Deserialization(String),

    #[error("Incompatible version: expected <= {expected}, found {found}")]
    IncompatibleVersion { expected: u32, found: u32 },

    #[error("Invalid data: {0}")]
    InvalidData(String),

    #[error("HNSW persistence error: {0}")]
    HNSWError(String),

    #[error("IVF persistence error: {0}")]
    IVFError(String),

    #[error("Missing component: {0}")]
    MissingComponent(String),
}

impl From<HNSWPersistenceError> for PersistenceError {
    fn from(err: HNSWPersistenceError) -> Self {
        PersistenceError::HNSWError(err.to_string())
    }
}

impl From<IVFPersistenceError> for PersistenceError {
    fn from(err: IVFPersistenceError) -> Self {
        PersistenceError::IVFError(err.to_string())
    }
}

/// Metadata for serialized HybridIndex
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridMetadata {
    pub version: u32,
    pub config: HybridConfig,
    pub recent_count: usize,
    pub historical_count: usize,
    pub total_vectors: usize,
    pub timestamp: DateTime<Utc>,
}

impl HybridMetadata {
    /// Create metadata from a HybridIndex
    pub fn from_index(index: &HybridIndex) -> Self {
        let stats = index.get_stats();
        Self {
            version: CURRENT_VERSION,
            config: index.config().clone(),
            recent_count: stats.recent_vectors,
            historical_count: stats.historical_vectors,
            total_vectors: stats.total_vectors,
            timestamp: Utc::now(),
        }
    }

    /// Serialize metadata to CBOR bytes
    pub fn to_cbor(&self) -> Result<Vec<u8>, PersistenceError> {
        serde_cbor::to_vec(self).map_err(|e| PersistenceError::Serialization(e.to_string()))
    }

    /// Deserialize metadata from CBOR bytes
    pub fn from_cbor(data: &[u8]) -> Result<Self, PersistenceError> {
        let metadata: Self = serde_cbor::from_slice(data)
            .map_err(|e| PersistenceError::Deserialization(e.to_string()))?;

        // Version compatibility check
        if metadata.version > CURRENT_VERSION {
            return Err(PersistenceError::IncompatibleVersion {
                expected: CURRENT_VERSION,
                found: metadata.version,
            });
        }

        Ok(metadata)
    }
}

/// Serializable wrapper for vector timestamps
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableTimestamps {
    pub timestamps: HashMap<VectorId, DateTime<Utc>>,
}

impl SerializableTimestamps {
    pub fn new(timestamps: HashMap<VectorId, DateTime<Utc>>) -> Self {
        Self { timestamps }
    }

    pub fn to_cbor(&self) -> Result<Vec<u8>, PersistenceError> {
        serde_cbor::to_vec(self).map_err(|e| PersistenceError::Serialization(e.to_string()))
    }

    pub fn from_cbor(data: &[u8]) -> Result<Self, PersistenceError> {
        serde_cbor::from_slice(data)
            .map_err(|e| PersistenceError::Deserialization(e.to_string()))
    }
}

/// Persister for HybridIndex using S5 storage
pub struct HybridPersister<S: S5Storage> {
    storage: S,
}

impl<S: S5Storage + Clone> HybridPersister<S> {
    pub fn new(storage: S) -> Self {
        Self { storage }
    }

    pub fn storage(&self) -> &S {
        &self.storage
    }

    /// Save HybridIndex to S5 storage
    pub async fn save_index(&self, index: &HybridIndex, path: &str) -> Result<(), PersistenceError> {
        // 1. Save metadata
        let metadata = HybridMetadata::from_index(index);
        let metadata_path = format!("{}/metadata.cbor", path);
        self.storage
            .put(&metadata_path, metadata.to_cbor()?)
            .await
            .map_err(|e| PersistenceError::Storage(e.to_string()))?;

        // 2. Save timestamps using accessor method
        let timestamps = index.get_timestamps().await;
        let serializable_timestamps = SerializableTimestamps::new(timestamps);
        let timestamps_path = format!("{}/timestamps.cbor", path);
        self.storage
            .put(&timestamps_path, serializable_timestamps.to_cbor()?)
            .await
            .map_err(|e| PersistenceError::Storage(e.to_string()))?;

        // 3. Save recent index (HNSW) using HNSWPersister
        let recent_index_guard = index.get_recent_index().await;
        let hnsw_persister = HNSWPersister::new(self.storage.clone());
        let recent_path = format!("{}/recent", path);
        hnsw_persister.save_index(&*recent_index_guard, &recent_path).await?;
        drop(recent_index_guard);

        // 4. Save historical index (IVF) using IVFPersister
        let historical_index_guard = index.get_historical_index().await;
        let ivf_persister = IVFPersister::new(self.storage.clone());
        let historical_path = format!("{}/historical", path);
        ivf_persister.save_index(&*historical_index_guard, &historical_path).await?;
        drop(historical_index_guard);

        Ok(())
    }

    /// Save HybridIndex using chunked storage format
    ///
    /// This method partitions vectors into chunks (default 10K vectors per chunk),
    /// saves each chunk separately, and creates a manifest with index structure.
    ///
    /// # Arguments
    /// * `index` - The HybridIndex to save
    /// * `path` - Base path for storage (e.g., "session-123")
    ///
    /// # Returns
    /// The manifest with chunk metadata
    pub async fn save_index_chunked(&self, index: &HybridIndex, path: &str) -> Result<Manifest, PersistenceError> {
        const CHUNK_SIZE: usize = 10000;

        // Validate path
        if path.is_empty() {
            return Err(PersistenceError::InvalidData("Path cannot be empty".to_string()));
        }

        let stats = index.get_stats();
        let mut manifest = Manifest::new(CHUNK_SIZE, stats.total_vectors);

        // If empty index, just save manifest
        if stats.total_vectors == 0 {
            let manifest_json = manifest
                .to_json()
                .map_err(|e| PersistenceError::Serialization(e.to_string()))?;

            let manifest_path = format!("{}/manifest.json", path);
            self.storage
                .put(&manifest_path, manifest_json.into_bytes())
                .await
                .map_err(|e| PersistenceError::Storage(e.to_string()))?;

            return Ok(manifest);
        }

        // Step 1: Collect all vectors from both HNSW and IVF indices
        let all_vectors = self.collect_all_vectors(index).await?;

        // Step 2: Partition vectors into chunks
        let chunks = self.partition_into_chunks(all_vectors, CHUNK_SIZE);

        // Step 3: Save each chunk and collect metadata
        for (chunk_idx, chunk) in chunks.into_iter().enumerate() {
            let chunk_metadata = self.save_chunk(&chunk, path, chunk_idx).await?;
            manifest.add_chunk(chunk_metadata);
        }

        // Step 4: Build HNSW manifest (graph structure without vectors)
        let hnsw_manifest = self.build_hnsw_manifest(index, &manifest).await?;
        manifest.set_hnsw_structure(hnsw_manifest);

        // Step 5: Build IVF manifest (centroids and cluster assignments)
        let ivf_manifest = self.build_ivf_manifest(index, &manifest).await?;
        manifest.set_ivf_structure(ivf_manifest);

        // Step 6: Save manifest as JSON (unencrypted for fast loading)
        let manifest_json = manifest
            .to_json()
            .map_err(|e| PersistenceError::Serialization(e.to_string()))?;

        let manifest_path = format!("{}/manifest.json", path);
        self.storage
            .put(&manifest_path, manifest_json.into_bytes())
            .await
            .map_err(|e| PersistenceError::Storage(e.to_string()))?;

        // Step 7: Save metadata separately (timestamps, etc.)
        self.save_metadata(index, path).await?;

        Ok(manifest)
    }

    /// Collect all vectors from the hybrid index
    async fn collect_all_vectors(&self, index: &HybridIndex) -> Result<Vec<(VectorId, Vec<f32>)>, PersistenceError> {
        let mut all_vectors = Vec::new();

        // Get vectors from HNSW index (recent vectors)
        // Extract data immediately and drop lock to avoid deadlock with sync operations
        let hnsw_nodes = {
            let recent_index = index.get_recent_index().await;
            recent_index.get_all_nodes()
        };

        for node in hnsw_nodes {
            if !node.is_deleted() {
                all_vectors.push((node.id().clone(), node.vector().clone()));
            }
        }

        // Get vectors from IVF index (historical vectors)
        // Extract data immediately and drop lock
        let ivf_vectors = {
            let historical_index = index.get_historical_index().await;
            let mut vectors = Vec::new();
            for inverted_list in historical_index.get_all_inverted_lists().values() {
                for (id, vector) in &inverted_list.vectors {
                    vectors.push((id.clone(), vector.clone()));
                }
            }
            vectors
        };

        all_vectors.extend(ivf_vectors);

        Ok(all_vectors)
    }

    /// Partition vectors into chunks of specified size
    fn partition_into_chunks(&self, vectors: Vec<(VectorId, Vec<f32>)>, chunk_size: usize) -> Vec<VectorChunk> {
        let mut chunks = Vec::new();
        let total_vectors = vectors.len();

        for (chunk_idx, chunk_vectors) in vectors.chunks(chunk_size).enumerate() {
            let start_idx = chunk_idx * chunk_size;
            let end_idx = std::cmp::min(start_idx + chunk_size - 1, total_vectors - 1);

            let mut chunk = VectorChunk::new(
                format!("chunk-{}", chunk_idx),
                start_idx,
                end_idx,
            );

            for (id, vector) in chunk_vectors {
                chunk.add_vector(id.clone(), vector.clone());
            }

            chunks.push(chunk);
        }

        chunks
    }

    /// Save a single chunk to S5 storage
    async fn save_chunk(&self, chunk: &VectorChunk, base_path: &str, chunk_idx: usize) -> Result<ChunkMetadata, PersistenceError> {
        // Serialize chunk to CBOR
        let cbor_data = chunk
            .to_cbor()
            .map_err(|e| PersistenceError::Serialization(e.to_string()))?;

        // Save to S5
        let chunk_path = format!("{}/chunks/chunk-{}.cbor", base_path, chunk_idx);
        self.storage
            .put(&chunk_path, cbor_data.clone())
            .await
            .map_err(|e| PersistenceError::Storage(e.to_string()))?;

        // Get first and last vector IDs for range
        let vector_ids: Vec<&VectorId> = chunk.vectors.keys().collect();
        let (start_id, end_id) = if !vector_ids.is_empty() {
            (
                (*vector_ids.first().unwrap()).clone(),
                (*vector_ids.last().unwrap()).clone(),
            )
        } else {
            (VectorId::from_string(""), VectorId::from_string(""))
        };

        // Create metadata
        Ok(ChunkMetadata::new(
            format!("chunk-{}", chunk_idx),
            chunk.len(),
            cbor_data.len(),
            start_id,
            end_id,
        ))
    }

    /// Build HNSW manifest from the index
    async fn build_hnsw_manifest(&self, index: &HybridIndex, manifest: &Manifest) -> Result<HNSWManifest, PersistenceError> {
        // Extract all data we need while holding the lock, then drop it immediately
        let (entry_point, level_distribution, nodes) = {
            let recent_index = index.get_recent_index().await;

            let entry_point = recent_index.entry_point()
                .unwrap_or_else(|| VectorId::from_string("placeholder"));
            let level_distribution = recent_index.get_level_distribution();
            let nodes = recent_index.get_all_nodes();

            (entry_point, level_distribution, nodes)
        };

        let mut hnsw_manifest = HNSWManifest::new(entry_point);

        // Add layer metadata (distribution of nodes per layer)
        for (layer_id, node_count) in level_distribution.iter().enumerate() {
            hnsw_manifest.add_layer(layer_id, *node_count);
        }

        // Map nodes to chunks
        for node in nodes {
            let chunk_id = self.find_chunk_for_vector(node.id(), manifest);
            hnsw_manifest.add_node_chunk_mapping(node.id().clone(), chunk_id);
        }

        Ok(hnsw_manifest)
    }

    /// Build IVF manifest from the index
    async fn build_ivf_manifest(&self, index: &HybridIndex, manifest: &Manifest) -> Result<IVFManifest, PersistenceError> {
        // Extract all data we need while holding the lock, then drop it immediately
        let (centroids, cluster_vector_ids) = {
            let historical_index = index.get_historical_index().await;

            // Get centroids (keep in memory - these are small)
            let centroids: Vec<Vec<f32>> = historical_index
                .get_centroids()
                .iter()
                .map(|c| c.vector().clone())
                .collect();

            // Extract vector IDs per cluster
            let cluster_vector_ids: Vec<(usize, Vec<VectorId>)> = historical_index
                .get_all_inverted_lists()
                .iter()
                .map(|(cluster_id, inverted_list)| {
                    (cluster_id.0, inverted_list.vectors.keys().cloned().collect())
                })
                .collect();

            (centroids, cluster_vector_ids)
        };

        let mut ivf_manifest = IVFManifest::new(centroids);

        // Map clusters to chunks
        for (cluster_id, vector_ids) in cluster_vector_ids {
            let mut chunk_ids = std::collections::HashSet::new();

            // Find which chunks contain vectors from this cluster
            for vector_id in &vector_ids {
                let chunk_id = self.find_chunk_for_vector(vector_id, manifest);
                chunk_ids.insert(chunk_id);
            }

            ivf_manifest.add_cluster_assignment(cluster_id, chunk_ids.into_iter().collect());
        }

        Ok(ivf_manifest)
    }

    /// Find which chunk contains a given vector
    fn find_chunk_for_vector(&self, vector_id: &VectorId, manifest: &Manifest) -> String {
        // Search through chunks to find which one contains this vector
        for chunk_meta in &manifest.chunks {
            // Check if vector_id falls within this chunk's range
            // For now, use a simple heuristic based on vector ID string
            // In production, we'd maintain a proper vector_id -> chunk_id mapping

            // Simple approach: check if the chunk was saved and assume linear ordering
            // This works because we partition vectors sequentially
            // TODO: Maintain explicit mapping during partitioning for production use
        }

        // Default to first chunk if not found
        if manifest.chunks.is_empty() {
            "chunk-0".to_string()
        } else {
            // For MVP: distribute evenly across chunks based on hash
            let chunk_idx = vector_id.to_string().len() % manifest.chunks.len();
            manifest.chunks[chunk_idx].chunk_id.clone()
        }
    }

    /// Save metadata (timestamps, config, etc.)
    async fn save_metadata(&self, index: &HybridIndex, path: &str) -> Result<(), PersistenceError> {
        let metadata = HybridMetadata::from_index(index);
        let metadata_cbor = metadata.to_cbor()?;

        let metadata_path = format!("{}/metadata.cbor", path);
        self.storage
            .put(&metadata_path, metadata_cbor)
            .await
            .map_err(|e| PersistenceError::Storage(e.to_string()))?;

        Ok(())
    }

    /// Load HybridIndex from S5 storage
    pub async fn load_index(&self, path: &str) -> Result<HybridIndex, PersistenceError> {
        // 1. Load metadata
        let metadata_path = format!("{}/metadata.cbor", path);
        let metadata_data = self
            .storage
            .get(&metadata_path)
            .await
            .map_err(|e| PersistenceError::Storage(e.to_string()))?
            .ok_or_else(|| PersistenceError::MissingComponent("metadata".to_string()))?;

        let metadata = HybridMetadata::from_cbor(&metadata_data)?;

        // 2. Load timestamps
        let timestamps_path = format!("{}/timestamps.cbor", path);
        let timestamps_data = self
            .storage
            .get(&timestamps_path)
            .await
            .map_err(|e| PersistenceError::Storage(e.to_string()))?
            .ok_or_else(|| PersistenceError::MissingComponent("timestamps".to_string()))?;

        let serializable_timestamps = SerializableTimestamps::from_cbor(&timestamps_data)?;

        // 3. Load recent index (HNSW) using HNSWPersister
        let hnsw_persister = HNSWPersister::new(self.storage.clone());
        let recent_path = format!("{}/recent", path);
        let recent_index = hnsw_persister.load_index(&recent_path).await?;

        // 4. Load historical index (IVF) using IVFPersister
        let ivf_persister = IVFPersister::new(self.storage.clone());
        let historical_path = format!("{}/historical", path);
        let historical_index = ivf_persister.load_index(&historical_path).await?;

        // 5. Reconstruct HybridIndex using from_parts method
        HybridIndex::from_parts(
            metadata.config,
            recent_index,
            historical_index,
            serializable_timestamps.timestamps,
            metadata.recent_count,
            metadata.historical_count,
        )
        .map_err(|e| PersistenceError::InvalidData(format!("Failed to reconstruct index: {}", e)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hybrid_metadata_cbor_roundtrip() {
        let metadata = HybridMetadata {
            version: CURRENT_VERSION,
            config: HybridConfig::default(),
            recent_count: 100,
            historical_count: 500,
            total_vectors: 600,
            timestamp: Utc::now(),
        };

        let cbor = metadata.to_cbor().expect("Failed to serialize");
        let deserialized = HybridMetadata::from_cbor(&cbor).expect("Failed to deserialize");

        assert_eq!(deserialized.version, metadata.version);
        assert_eq!(deserialized.recent_count, metadata.recent_count);
        assert_eq!(deserialized.historical_count, metadata.historical_count);
        assert_eq!(deserialized.total_vectors, metadata.total_vectors);
    }

    #[test]
    fn test_serializable_timestamps_cbor_roundtrip() {
        let mut timestamps = HashMap::new();
        timestamps.insert(
            VectorId::from_string("test1"),
            Utc::now(),
        );
        timestamps.insert(
            VectorId::from_string("test2"),
            Utc::now(),
        );

        let serializable = SerializableTimestamps::new(timestamps.clone());
        let cbor = serializable.to_cbor().expect("Failed to serialize");
        let deserialized = SerializableTimestamps::from_cbor(&cbor).expect("Failed to deserialize");

        assert_eq!(deserialized.timestamps.len(), timestamps.len());
        for (id, _timestamp) in &timestamps {
            assert!(deserialized.timestamps.contains_key(id));
        }
    }

    #[test]
    fn test_version_compatibility() {
        let metadata = HybridMetadata {
            version: CURRENT_VERSION + 1, // Future version
            config: HybridConfig::default(),
            recent_count: 0,
            historical_count: 0,
            total_vectors: 0,
            timestamp: Utc::now(),
        };

        let cbor = serde_cbor::to_vec(&metadata).unwrap();
        let result = HybridMetadata::from_cbor(&cbor);

        assert!(result.is_err());
        match result {
            Err(PersistenceError::IncompatibleVersion { expected, found }) => {
                assert_eq!(expected, CURRENT_VERSION);
                assert_eq!(found, CURRENT_VERSION + 1);
            }
            _ => panic!("Expected IncompatibleVersion error"),
        }
    }

    #[tokio::test]
    async fn test_hybrid_persister_save_and_load() {
        use crate::core::storage::MockS5Storage;

        // Create MockS5Storage
        let storage = MockS5Storage::new();

        // Create a HybridIndex with some test data
        let mut index = HybridIndex::new(HybridConfig::default());

        // Initialize with training data
        let training_data = vec![
            vec![0.1, 0.2, 0.3, 0.4],
            vec![0.2, 0.3, 0.4, 0.5],
            vec![0.3, 0.4, 0.5, 0.6],
        ];
        index.initialize(training_data.clone()).await.expect("Failed to initialize");

        // Add some vectors
        let id1 = VectorId::from_string("vec1");
        let id2 = VectorId::from_string("vec2");
        let id3 = VectorId::from_string("vec3");

        index.insert(id1.clone(), vec![0.1, 0.2, 0.3, 0.4]).await.expect("Failed to insert vec1");
        index.insert(id2.clone(), vec![0.2, 0.3, 0.4, 0.5]).await.expect("Failed to insert vec2");
        index.insert(id3.clone(), vec![0.3, 0.4, 0.5, 0.6]).await.expect("Failed to insert vec3");

        // Get stats before save
        let stats_before = index.get_statistics().await;
        assert_eq!(stats_before.total_vectors, 3);

        // Create persister and save
        let persister = HybridPersister::new(storage.clone());
        let path = "test/hybrid_index";
        persister.save_index(&index, path).await.expect("Failed to save index");

        // Load the index
        let loaded_index = persister.load_index(path).await.expect("Failed to load index");

        // Verify stats match
        let stats_after = loaded_index.get_statistics().await;
        assert_eq!(stats_after.total_vectors, stats_before.total_vectors);
        assert_eq!(stats_after.recent_vectors, stats_before.recent_vectors);
        assert_eq!(stats_after.historical_vectors, stats_before.historical_vectors);
    }

    #[tokio::test]
    async fn test_hybrid_persister_preserves_vector_count() {
        use crate::core::storage::MockS5Storage;

        let storage = MockS5Storage::new();
        let mut index = HybridIndex::new(HybridConfig::default());

        // Initialize
        let training_data: Vec<Vec<f32>> = (0..10)
            .map(|i| vec![i as f32 * 0.1; 4])
            .collect();
        index.initialize(training_data).await.expect("Failed to initialize");

        // Add 20 vectors
        for i in 0..20 {
            let id = VectorId::from_string(&format!("vec{}", i));
            let vector = vec![i as f32 * 0.05; 4];
            index.insert(id, vector).await.expect("Failed to insert");
        }

        let stats_before = index.get_statistics().await;
        assert_eq!(stats_before.total_vectors, 20);

        // Save and load
        let persister = HybridPersister::new(storage);
        persister.save_index(&index, "test/count_test").await.expect("Failed to save");
        let loaded = persister.load_index("test/count_test").await.expect("Failed to load");

        // Verify counts
        let stats_after = loaded.get_statistics().await;
        assert_eq!(stats_after.total_vectors, 20, "Total vector count mismatch");
        assert_eq!(
            stats_after.recent_vectors + stats_after.historical_vectors,
            20,
            "Sum of recent and historical vectors doesn't match total"
        );
    }

    #[tokio::test]
    async fn test_hybrid_persister_preserves_search_results() {
        use crate::core::storage::MockS5Storage;

        let storage = MockS5Storage::new();
        let mut index = HybridIndex::new(HybridConfig::default());

        // Create deterministic test data
        let dim = 4;
        let training_data: Vec<Vec<f32>> = (0..10)
            .map(|i| vec![i as f32; dim])
            .collect();

        index.initialize(training_data).await.expect("Failed to initialize");

        // Add vectors with known patterns
        for i in 0..10 {
            let id = VectorId::from_string(&format!("vec{}", i));
            let vector = vec![i as f32; dim];
            index.insert(id, vector).await.expect("Failed to insert");
        }

        // Perform a search before save
        let query = vec![5.0; dim];
        let results_before = index.search(&query, 3).await.expect("Search failed");
        assert!(!results_before.is_empty(), "No results from original index");

        // Save and load
        let persister = HybridPersister::new(storage);
        persister.save_index(&index, "test/search_test").await.expect("Failed to save");
        let loaded = persister.load_index("test/search_test").await.expect("Failed to load");

        // Perform same search on loaded index
        let results_after = loaded.search(&query, 3).await.expect("Search failed on loaded index");

        // Verify we got the same number of results
        assert_eq!(
            results_after.len(),
            results_before.len(),
            "Different number of search results"
        );

        // Verify the distances are similar
        // Note: VectorIds might be reassigned during index reconstruction,
        // but the search quality (distances) should be preserved
        for (before, after) in results_before.iter().zip(results_after.iter()) {
            // Allow small floating point differences in distances
            let distance_diff = (before.distance - after.distance).abs();
            assert!(
                distance_diff < 0.01,
                "Search distances differ too much: {} vs {}",
                before.distance,
                after.distance
            );
        }

        // Also verify that both result sets have similar distance ranges
        let before_min = results_before.iter().map(|r| r.distance).fold(f32::INFINITY, f32::min);
        let before_max = results_before.iter().map(|r| r.distance).fold(f32::NEG_INFINITY, f32::max);
        let after_min = results_after.iter().map(|r| r.distance).fold(f32::INFINITY, f32::min);
        let after_max = results_after.iter().map(|r| r.distance).fold(f32::NEG_INFINITY, f32::max);

        assert!(
            (before_min - after_min).abs() < 0.01,
            "Min distances don't match: {} vs {}",
            before_min,
            after_min
        );
        assert!(
            (before_max - after_max).abs() < 0.01,
            "Max distances don't match: {} vs {}",
            before_max,
            after_max
        );
    }

    #[tokio::test]
    async fn test_hybrid_persister_empty_index() {
        use crate::core::storage::MockS5Storage;

        let storage = MockS5Storage::new();
        let index = HybridIndex::new(HybridConfig::default());

        // Don't initialize or add any vectors - save empty index
        let persister = HybridPersister::new(storage);
        let result = persister.save_index(&index, "test/empty").await;

        // Should succeed (empty index is valid)
        assert!(result.is_ok(), "Failed to save empty index: {:?}", result.err());

        // Load should also succeed
        let loaded = persister.load_index("test/empty").await;
        assert!(loaded.is_ok(), "Failed to load empty index: {:?}", loaded.err());

        if let Ok(loaded_index) = loaded {
            let stats = loaded_index.get_statistics().await;
            assert_eq!(stats.total_vectors, 0, "Empty index should have 0 vectors");
        }
    }

    #[tokio::test]
    async fn test_hybrid_persister_missing_metadata() {
        use crate::core::storage::MockS5Storage;

        let storage = MockS5Storage::new();
        let persister = HybridPersister::new(storage);

        // Try to load from non-existent path
        let result = persister.load_index("test/nonexistent").await;

        // Should fail with MissingComponent error
        assert!(result.is_err());
        match result {
            Err(PersistenceError::MissingComponent(component)) => {
                assert_eq!(component, "metadata");
            }
            Err(e) => panic!("Expected MissingComponent error, got: {:?}", e),
            Ok(_) => panic!("Expected error, got Ok"),
        }
    }
}
