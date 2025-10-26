use napi::Result;
use napi_derive::napi;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use vector_db::{
    core::types::VectorId,
    hybrid::{HybridIndex, HybridConfig},
    storage::{EnhancedS5Storage, S5StorageConfig, StorageMode},
};

use crate::{
    error::VectorDBError,
    types::{LoadOptions, SearchOptions, SearchResult, SessionStats, VectorDBConfig, VectorInput},
    utils,
};

struct SessionState {
    session_id: String,
    index: Arc<RwLock<HybridIndex>>,
    metadata: Arc<RwLock<HashMap<String, serde_json::Value>>>, // vector_id -> metadata
    storage: Arc<EnhancedS5Storage>,
    config: VectorDBConfig,
    vector_dimension: Option<usize>,
}

#[napi]
pub struct VectorDBSession {
    state: Option<SessionState>,
}

#[napi]
impl VectorDBSession {
    /// Create a new vector DB session
    #[napi(factory)]
    pub async fn create(config: VectorDBConfig) -> Result<Self> {
        // Validate config
        if config.session_id.is_empty() {
            return Err(VectorDBError::invalid_config("session_id is required").into());
        }
        if config.s5_portal.is_empty() {
            return Err(VectorDBError::invalid_config("s5_portal is required").into());
        }
        if config.user_seed_phrase.is_empty() {
            return Err(VectorDBError::invalid_config("user_seed_phrase is required").into());
        }

        // Initialize S5 storage
        let s5_config = S5StorageConfig {
            mode: StorageMode::Real,
            portal_url: Some(config.s5_portal.clone()),
            seed_phrase: Some(config.user_seed_phrase.clone()),
            mock_server_url: None,
            connection_timeout: Some(30000), // 30 seconds
            retry_attempts: Some(3),
        };

        let storage = EnhancedS5Storage::new(s5_config)
            .map_err(|e| VectorDBError::storage_error(format!("Failed to initialize S5 storage: {}", e)))?;

        // Create hybrid index with default configuration
        let hybrid_config = HybridConfig::default();
        let index = HybridIndex::new(hybrid_config);

        let state = SessionState {
            session_id: config.session_id.clone(),
            index: Arc::new(RwLock::new(index)),
            metadata: Arc::new(RwLock::new(HashMap::new())),
            storage: Arc::new(storage),
            config,
            vector_dimension: None,
        };

        Ok(Self { state: Some(state) })
    }

    /// Load user's vectors from S5
    #[napi]
    pub async unsafe fn load_user_vectors(
        &mut self,
        _cid: String,
        _options: Option<LoadOptions>,
    ) -> Result<()> {
        // TODO: Phase 3 - Implement S5 loading when serialization is available
        // This requires HybridIndex::load_from_bytes() method which doesn't exist yet
        Err(VectorDBError::session_error(
            "load_user_vectors not yet implemented - requires index serialization support"
        ).into())
    }

    /// Search for similar vectors
    #[napi]
    pub async fn search(
        &self,
        query_vector: Vec<f64>,
        k: u32,
        options: Option<SearchOptions>,
    ) -> Result<Vec<SearchResult>> {
        let state = self.state.as_ref()
            .ok_or_else(|| VectorDBError::session_error("Session already destroyed"))?;

        // Convert f64 to f32 for Rust
        let query_f32 = utils::js_array_to_vec_f32(query_vector);

        let threshold = options.as_ref()
            .and_then(|o| o.threshold)
            .unwrap_or(0.7) as f32; // Convert to f32 for comparison

        // Perform search using HybridIndex
        let index = state.index.read().await;
        let results = index.search(&query_f32, k as usize)
            .await
            .map_err(|e| VectorDBError::index_error(format!("Search failed: {}", e)))?;
        drop(index); // Release read lock

        // Get metadata map
        let metadata_map = state.metadata.read().await;

        // Convert results to SearchResult format
        let search_results: Vec<SearchResult> = results
            .into_iter()
            .filter(|r| {
                // Convert distance to similarity score (1 - distance) and filter by threshold
                let score = 1.0 - r.distance;
                score >= threshold
            })
            .map(|r| {
                let vector_id_str = r.vector_id.to_string();
                // Retrieve metadata or use empty JSON object
                let metadata = metadata_map
                    .get(&vector_id_str)
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({}));

                SearchResult {
                    id: vector_id_str,
                    score: (1.0 - r.distance) as f64, // Convert distance to similarity score
                    metadata,
                    vector: None, // TODO: Add vector inclusion based on options
                }
            })
            .collect();

        Ok(search_results)
    }

    /// Add vectors to the index
    #[napi]
    pub async unsafe fn add_vectors(&mut self, vectors: Vec<VectorInput>) -> Result<()> {
        let state = self.state.as_mut()
            .ok_or_else(|| VectorDBError::session_error("Session already destroyed"))?;

        // Check and set vector dimension from first vector
        if !vectors.is_empty() {
            let first_dim = vectors[0].vector.len();

            if let Some(expected_dim) = state.vector_dimension {
                if first_dim != expected_dim {
                    return Err(VectorDBError::index_error(
                        format!("Vector dimension mismatch: expected {}, got {}", expected_dim, first_dim)
                    ).into());
                }
            } else {
                state.vector_dimension = Some(first_dim);
            }
        }

        // Get write lock on index
        let index = state.index.clone();
        let mut index_guard = index.write().await;

        // Initialize index if not already initialized (first time adding vectors)
        if !vectors.is_empty() {
            // Prepare training data from the first batch
            let training_data: Vec<Vec<f32>> = vectors
                .iter()
                .take(10) // Use first 10 vectors for training
                .map(|v| utils::js_array_to_vec_f32(v.vector.clone()))
                .collect();

            if !training_data.is_empty() {
                index_guard.initialize(training_data)
                    .await
                    .map_err(|e| VectorDBError::index_error(format!("Failed to initialize index: {}", e)))?;
            }
        }

        // Insert vectors and store metadata
        let metadata_map = state.metadata.clone();
        let mut metadata_guard = metadata_map.write().await;

        for input in vectors {
            let vector_id = VectorId::from_string(&input.id);
            let vector_f32 = utils::js_array_to_vec_f32(input.vector);

            // Validate dimension
            if let Some(expected_dim) = state.vector_dimension {
                if vector_f32.len() != expected_dim {
                    return Err(VectorDBError::index_error(
                        format!("Vector dimension mismatch: expected {}, got {}", expected_dim, vector_f32.len())
                    ).into());
                }
            }

            // Insert vector into index
            index_guard.insert(vector_id.clone(), vector_f32)
                .await
                .map_err(|e| VectorDBError::index_error(format!("Failed to add vector: {}", e)))?;

            // Store metadata
            metadata_guard.insert(input.id.clone(), input.metadata);
        }

        Ok(())
    }

    /// Save index to S5
    #[napi]
    pub async fn save_to_s5(&self) -> Result<String> {
        // TODO: Phase 3 - Implement S5 save when serialization is available
        // This requires HybridIndex::to_bytes() method which doesn't exist yet
        Err(VectorDBError::session_error(
            "save_to_s5 not yet implemented - requires index serialization support"
        ).into())
    }

    /// Get session statistics
    #[napi]
    pub fn get_stats(&self) -> Result<SessionStats> {
        let state = self.state.as_ref()
            .ok_or_else(|| VectorDBError::session_error("Session already destroyed"))?;

        // Get stats from HybridIndex (synchronous call)
        let index = state.index.try_read()
            .map_err(|_| VectorDBError::session_error("Failed to read index stats"))?;

        let stats = index.get_stats();

        Ok(SessionStats {
            vector_count: stats.total_vectors as u32,
            memory_usage_mb: ((stats.recent_index_memory + stats.historical_index_memory) as f64) / 1_048_576.0,
            index_type: "hybrid".to_string(),
            hnsw_vector_count: Some(stats.recent_vectors as u32),
            ivf_vector_count: Some(stats.historical_vectors as u32),
        })
    }

    /// Destroy session and clear memory
    #[napi]
    pub async unsafe fn destroy(&mut self) -> Result<()> {
        if let Some(state) = self.state.take() {
            // Drop the state which will drop the Arc references
            // When all references are dropped, the HybridIndex will be dropped
            drop(state);
        }

        Ok(())
    }
}

// Ensure cleanup on drop
impl Drop for VectorDBSession {
    fn drop(&mut self) {
        if let Some(state) = &self.state {
            eprintln!("WARNING: VectorDBSession '{}' dropped without calling destroy()", state.session_id);
        }
    }
}
