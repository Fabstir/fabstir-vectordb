use crate::core::storage::S5Storage;
use crate::core::types::VectorId;
use crate::ivf::core::{Centroid, ClusterId, IVFConfig, IVFIndex, InvertedList};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

const CURRENT_VERSION: u32 = 1;
const DEFAULT_CHUNK_SIZE: usize = 1000;

#[derive(Debug, Error)]
pub enum PersistenceError {
    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Incompatible version: expected <= {expected}, found {found}")]
    IncompatibleVersion { expected: u32, found: u32 },

    #[error("Invalid data: {0}")]
    InvalidData(String),

    #[error("Incomplete save: expected {expected} vectors, found {found}")]
    IncompleteSave { expected: usize, found: usize },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IVFMetadata {
    pub version: u32,
    pub config: IVFConfig,
    pub dimension: usize,
    pub n_vectors: usize,
    pub centroids_count: usize,
    pub timestamp: DateTime<Utc>,
}

impl IVFMetadata {
    pub fn from_index(index: &IVFIndex) -> Self {
        Self {
            version: CURRENT_VERSION,
            config: index.config().clone(),
            dimension: index.dimension().unwrap_or(0),
            n_vectors: index.total_vectors(),
            centroids_count: index.get_centroids().len(),
            timestamp: Utc::now(),
        }
    }

    pub fn to_cbor(&self) -> Result<Vec<u8>, PersistenceError> {
        serde_cbor::to_vec(self).map_err(|e| PersistenceError::Serialization(e.to_string()))
    }

    pub fn from_cbor(data: &[u8]) -> Result<Self, PersistenceError> {
        serde_cbor::from_slice(data).map_err(|e| PersistenceError::Serialization(e.to_string()))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableInvertedList {
    pub cluster_id: ClusterId,
    pub vectors: HashMap<VectorId, Vec<f32>>,
}

impl SerializableInvertedList {
    pub fn from_inverted_list(cluster_id: ClusterId, list: &InvertedList) -> Self {
        Self {
            cluster_id,
            vectors: list.vectors.clone(),
        }
    }

    pub fn to_inverted_list(self) -> InvertedList {
        InvertedList {
            vectors: self.vectors,
        }
    }

    pub fn cluster_id(&self) -> ClusterId {
        self.cluster_id
    }

    pub fn size(&self) -> usize {
        self.vectors.len()
    }

    pub fn to_cbor(&self) -> Result<Vec<u8>, PersistenceError> {
        serde_cbor::to_vec(self).map_err(|e| PersistenceError::Serialization(e.to_string()))
    }

    pub fn from_cbor(data: &[u8]) -> Result<Self, PersistenceError> {
        serde_cbor::from_slice(data).map_err(|e| PersistenceError::Serialization(e.to_string()))
    }

    pub fn to_cbor_compressed(&self) -> Result<Vec<u8>, PersistenceError> {
        let cbor = self.to_cbor()?;

        // Use zstd compression
        let compressed = zstd::encode_all(&cbor[..], 3)
            .map_err(|e| PersistenceError::Serialization(format!("Compression failed: {}", e)))?;

        Ok(compressed)
    }

    pub fn from_cbor_compressed(data: &[u8]) -> Result<Self, PersistenceError> {
        // Decompress first
        let decompressed = zstd::decode_all(data)
            .map_err(|e| PersistenceError::Serialization(format!("Decompression failed: {}", e)))?;

        Self::from_cbor(&decompressed)
    }
}

// Extension methods for InvertedList
impl InvertedList {
    pub fn add(&mut self, id: VectorId, vector: Vec<f32>) {
        self.vectors.insert(id, vector);
    }

    pub fn get(&self, id: &VectorId) -> Option<&Vec<f32>> {
        self.vectors.get(id)
    }

    pub fn size(&self) -> usize {
        self.vectors.len()
    }
}

pub struct IVFPersister<S: S5Storage> {
    storage: S,
    chunk_size: usize,
    use_compression: bool,
}

impl<S: S5Storage> IVFPersister<S> {
    pub fn new(storage: S) -> Self {
        Self {
            storage,
            chunk_size: DEFAULT_CHUNK_SIZE,
            use_compression: false,
        }
    }

    pub fn with_chunk_size(storage: S, chunk_size: usize) -> Self {
        Self {
            storage,
            chunk_size,
            use_compression: false,
        }
    }

    pub fn with_compression(storage: S, use_compression: bool) -> Self {
        Self {
            storage,
            chunk_size: DEFAULT_CHUNK_SIZE,
            use_compression,
        }
    }

    pub fn storage(&self) -> &S {
        &self.storage
    }

    pub async fn save_index(&self, index: &IVFIndex, path: &str) -> Result<(), PersistenceError> {
        // Save metadata
        let metadata = IVFMetadata::from_index(index);
        let metadata_path = format!("{}/metadata.cbor", path);
        self.storage
            .put(&metadata_path, metadata.to_cbor()?)
            .await
            .map_err(|e| PersistenceError::Storage(e.to_string()))?;

        // Save centroids
        let centroids_path = format!("{}/centroids.cbor", path);
        let centroids_data = serialize_centroids(index.get_centroids())?;
        self.storage
            .put(&centroids_path, centroids_data)
            .await
            .map_err(|e| PersistenceError::Storage(e.to_string()))?;

        // Save inverted lists
        let inverted_lists = index.get_all_inverted_lists();

        // Convert to serializable format
        let serializable_lists: HashMap<ClusterId, SerializableInvertedList> = inverted_lists
            .iter()
            .map(|(cluster_id, list)| {
                (
                    *cluster_id,
                    SerializableInvertedList::from_inverted_list(*cluster_id, list),
                )
            })
            .collect();

        self.save_inverted_lists(path, &serializable_lists).await?;

        Ok(())
    }

    pub async fn load_index(&self, path: &str) -> Result<IVFIndex, PersistenceError> {
        // Load metadata
        let metadata_path = format!("{}/metadata.cbor", path);
        let metadata_data = self
            .storage
            .get(&metadata_path)
            .await
            .map_err(|e| PersistenceError::Storage(e.to_string()))?;
        let metadata_data = metadata_data
            .ok_or_else(|| PersistenceError::Storage("Metadata file not found".to_string()))?;
        let metadata = IVFMetadata::from_cbor(&metadata_data)?;

        // Check version
        if metadata.version > CURRENT_VERSION {
            return Err(PersistenceError::IncompatibleVersion {
                expected: CURRENT_VERSION,
                found: metadata.version,
            });
        }

        // Load centroids
        let centroids_path = format!("{}/centroids.cbor", path);
        let centroids_data = self
            .storage
            .get(&centroids_path)
            .await
            .map_err(|e| PersistenceError::Storage(e.to_string()))?;
        let centroids_data = centroids_data
            .ok_or_else(|| PersistenceError::Storage("Centroids file not found".to_string()))?;
        let centroids = deserialize_centroids(&centroids_data)?;

        // Create index
        let mut index = IVFIndex::new(metadata.config.clone());
        index.set_trained(centroids, metadata.dimension);

        // Load inverted lists
        let serializable_lists = self
            .load_inverted_lists(path, metadata.centroids_count)
            .await?;

        // Convert back to InvertedList format
        let mut inverted_lists = HashMap::new();
        let mut total_vectors = 0;

        for (cluster_id, ser_list) in serializable_lists {
            total_vectors += ser_list.size();
            inverted_lists.insert(cluster_id, ser_list.to_inverted_list());
        }

        if total_vectors != metadata.n_vectors {
            return Err(PersistenceError::IncompleteSave {
                expected: metadata.n_vectors,
                found: total_vectors,
            });
        }

        index.set_inverted_lists(inverted_lists);

        Ok(index)
    }

    pub async fn save_incremental(
        &self,
        index: &IVFIndex,
        path: &str,
        modified_clusters: &HashMap<ClusterId, SerializableInvertedList>,
    ) -> Result<(), PersistenceError> {
        // Update metadata
        let metadata = IVFMetadata::from_index(index);
        let metadata_path = format!("{}/metadata.cbor", path);
        self.storage
            .put(&metadata_path, metadata.to_cbor()?)
            .await
            .map_err(|e| PersistenceError::Storage(e.to_string()))?;

        // Save only modified inverted lists
        for (cluster_id, list) in modified_clusters {
            let list_path = self.get_inverted_list_path(path, *cluster_id);
            let data = if self.use_compression {
                list.to_cbor_compressed()?
            } else {
                list.to_cbor()?
            };

            self.storage
                .put(&list_path, data)
                .await
                .map_err(|e| PersistenceError::Storage(e.to_string()))?;
        }

        Ok(())
    }

    pub async fn check_integrity(
        &self,
        path: &str,
    ) -> Result<IntegrityCheckResult, PersistenceError> {
        // Load metadata
        let metadata_path = format!("{}/metadata.cbor", path);
        let metadata_data = self
            .storage
            .get(&metadata_path)
            .await
            .map_err(|e| PersistenceError::Storage(e.to_string()))?;
        let metadata_data = metadata_data
            .ok_or_else(|| PersistenceError::Storage("Metadata file not found".to_string()))?;
        let metadata = IVFMetadata::from_cbor(&metadata_data)?;

        // Check centroids
        let centroids_path = format!("{}/centroids.cbor", path);
        let has_centroids = self.storage.get(&centroids_path).await.is_ok();

        // Count vectors in inverted lists
        let mut found_vectors = 0;
        let mut missing_clusters = Vec::new();

        for cluster_idx in 0..metadata.centroids_count {
            let cluster_id = ClusterId(cluster_idx);
            let list_path = self.get_inverted_list_path(path, cluster_id);

            match self.storage.get(&list_path).await {
                Ok(Some(data)) => {
                    let list = if self.use_compression {
                        SerializableInvertedList::from_cbor_compressed(&data)?
                    } else {
                        SerializableInvertedList::from_cbor(&data)?
                    };
                    found_vectors += list.size();
                }
                Ok(None) | Err(_) => {
                    missing_clusters.push(cluster_id);
                }
            }
        }

        Ok(IntegrityCheckResult {
            expected_vectors: metadata.n_vectors,
            found_vectors,
            has_metadata: true,
            has_centroids,
            is_complete: found_vectors == metadata.n_vectors && missing_clusters.is_empty(),
            missing_clusters,
        })
    }

    pub async fn migrate_index(
        &self,
        source_path: &str,
        target_path: &str,
        new_config: IVFConfig,
    ) -> Result<MigrationResult, PersistenceError> {
        // Load existing index
        let old_index = self.load_index(source_path).await?;
        let old_clusters = old_index.config().n_clusters;

        // Collect all vectors
        let mut all_vectors = Vec::new();
        let mut all_ids = Vec::new();

        for list in old_index.get_all_inverted_lists().values() {
            for (id, vector) in &list.vectors {
                all_ids.push(id.clone());
                all_vectors.push(vector.clone());
            }
        }

        // Create new index
        let mut new_index = IVFIndex::new(new_config.clone());

        // Train on existing vectors
        new_index
            .train(&all_vectors)
            .map_err(|e| PersistenceError::InvalidData(e.to_string()))?;

        // Insert all vectors
        for (id, vector) in all_ids.iter().zip(all_vectors.iter()) {
            new_index
                .insert(id.clone(), vector.clone())
                .map_err(|e| PersistenceError::InvalidData(e.to_string()))?;
        }

        // Save new index
        self.save_index(&new_index, target_path).await?;

        Ok(MigrationResult {
            vectors_migrated: all_ids.len(),
            old_clusters,
            new_clusters: new_config.n_clusters,
        })
    }

    async fn save_inverted_lists(
        &self,
        path: &str,
        inverted_lists: &HashMap<ClusterId, SerializableInvertedList>,
    ) -> Result<(), PersistenceError> {
        // Group lists into chunks
        let mut chunks: Vec<Vec<(ClusterId, &SerializableInvertedList)>> = Vec::new();
        let mut current_chunk = Vec::new();

        for (cluster_id, list) in inverted_lists {
            current_chunk.push((*cluster_id, list));

            if current_chunk.len() >= self.chunk_size {
                chunks.push(current_chunk);
                current_chunk = Vec::new();
            }
        }

        if !current_chunk.is_empty() {
            chunks.push(current_chunk);
        }

        // Save chunks
        for (chunk_idx, chunk) in chunks.iter().enumerate() {
            if self.chunk_size == DEFAULT_CHUNK_SIZE {
                // Save individual files
                for (cluster_id, list) in chunk {
                    let list_path = self.get_inverted_list_path(path, *cluster_id);
                    let data = if self.use_compression {
                        list.to_cbor_compressed()?
                    } else {
                        list.to_cbor()?
                    };

                    self.storage
                        .put(&list_path, data)
                        .await
                        .map_err(|e| PersistenceError::Storage(e.to_string()))?;
                }
            } else {
                // Save as chunks
                let chunk_path = format!("{}/inverted_lists/chunk_{:04}.cbor", path, chunk_idx);
                let chunk_data = serialize_chunk(chunk)?;

                self.storage
                    .put(&chunk_path, chunk_data)
                    .await
                    .map_err(|e| PersistenceError::Storage(e.to_string()))?;
            }
        }

        Ok(())
    }

    async fn load_inverted_lists(
        &self,
        path: &str,
        n_clusters: usize,
    ) -> Result<HashMap<ClusterId, SerializableInvertedList>, PersistenceError> {
        let mut inverted_lists = HashMap::new();

        // Try to load chunks first
        let chunk_pattern = format!("{}/inverted_lists/", path);
        let files = self
            .storage
            .list(&chunk_pattern)
            .await
            .map_err(|e| PersistenceError::Storage(e.to_string()))?;

        let chunk_files: Vec<_> = files.iter().filter(|f| f.contains("chunk_")).collect();

        if !chunk_files.is_empty() {
            // Load from chunks
            for chunk_file in chunk_files {
                if let Ok(Some(chunk_data)) = self.storage.get(chunk_file).await {
                    let chunk_lists = deserialize_chunk(&chunk_data)?;
                    for list in chunk_lists {
                        inverted_lists.insert(list.cluster_id(), list);
                    }
                }
            }
        } else {
            // Load individual files
            for cluster_idx in 0..n_clusters {
                let cluster_id = ClusterId(cluster_idx);
                let list_path = self.get_inverted_list_path(path, cluster_id);

                if let Ok(Some(data)) = self.storage.get(&list_path).await {
                    let list = if self.use_compression {
                        SerializableInvertedList::from_cbor_compressed(&data)?
                    } else {
                        SerializableInvertedList::from_cbor(&data)?
                    };
                    inverted_lists.insert(cluster_id, list);
                }
            }
        }

        Ok(inverted_lists)
    }

    fn get_inverted_list_path(&self, base_path: &str, cluster_id: ClusterId) -> String {
        format!(
            "{}/inverted_lists/cluster_{:06}.cbor",
            base_path, cluster_id.0
        )
    }
}

#[derive(Debug)]
pub struct IntegrityCheckResult {
    pub expected_vectors: usize,
    pub found_vectors: usize,
    pub has_metadata: bool,
    pub has_centroids: bool,
    pub missing_clusters: Vec<ClusterId>,
    pub is_complete: bool,
}

#[derive(Debug)]
pub struct MigrationResult {
    pub vectors_migrated: usize,
    pub old_clusters: usize,
    pub new_clusters: usize,
}

// Helper functions for Centroid serialization
impl Centroid {
    pub fn to_cbor(&self) -> Result<Vec<u8>, PersistenceError> {
        serde_cbor::to_vec(self).map_err(|e| PersistenceError::Serialization(e.to_string()))
    }

    pub fn from_cbor(data: &[u8]) -> Result<Self, PersistenceError> {
        serde_cbor::from_slice(data).map_err(|e| PersistenceError::Serialization(e.to_string()))
    }
}

pub fn serialize_centroids(centroids: &[Centroid]) -> Result<Vec<u8>, PersistenceError> {
    serde_cbor::to_vec(&centroids).map_err(|e| PersistenceError::Serialization(e.to_string()))
}

fn deserialize_centroids(data: &[u8]) -> Result<Vec<Centroid>, PersistenceError> {
    serde_cbor::from_slice(data).map_err(|e| PersistenceError::Serialization(e.to_string()))
}

fn serialize_chunk(
    chunk: &[(ClusterId, &SerializableInvertedList)],
) -> Result<Vec<u8>, PersistenceError> {
    let lists: Vec<&SerializableInvertedList> = chunk.iter().map(|(_, list)| *list).collect();
    serde_cbor::to_vec(&lists).map_err(|e| PersistenceError::Serialization(e.to_string()))
}

fn deserialize_chunk(data: &[u8]) -> Result<Vec<SerializableInvertedList>, PersistenceError> {
    serde_cbor::from_slice(data).map_err(|e| PersistenceError::Serialization(e.to_string()))
}

pub async fn calculate_total_size<S: S5Storage>(storage: &S, files: &[String]) -> usize {
    let mut total_size = 0;

    for file in files {
        if let Ok(Some(data)) = storage.get(file).await {
            total_size += data.len();
        }
    }

    total_size
}
