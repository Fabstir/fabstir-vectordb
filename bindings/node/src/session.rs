use napi::Result;
use napi_derive::napi;

use crate::{
    error::VectorDBError,
    types::{LoadOptions, SearchOptions, SearchResult, SessionStats, VectorDBConfig, VectorInput},
};

#[napi]
pub struct VectorDBSession {
    session_id: String,
    // TODO: Add actual state fields in Phase 2
    // state: Option<SessionState>,
}

#[napi]
impl VectorDBSession {
    /// Create a new vector DB session
    #[napi(factory)]
    pub async fn create(config: VectorDBConfig) -> Result<Self> {
        // Validate config
        if config.s5_portal.is_empty() {
            return Err(VectorDBError::invalid_config("s5_portal is required").into());
        }
        if config.user_seed_phrase.is_empty() {
            return Err(VectorDBError::invalid_config("user_seed_phrase is required").into());
        }
        if config.session_id.is_empty() {
            return Err(VectorDBError::invalid_config("session_id is required").into());
        }

        // TODO: Phase 2 - Create S5 storage and hybrid index
        // let storage = create_s5_storage(&config).await?;
        // let index = create_hybrid_index().await?;

        Ok(Self {
            session_id: config.session_id,
        })
    }

    /// Load user's vectors from S5
    #[napi]
    pub async unsafe fn load_user_vectors(
        &mut self,
        _cid: String,
        _options: Option<LoadOptions>,
    ) -> Result<()> {
        // TODO: Phase 2 - Implement S5 loading
        // let lazy_load = options.as_ref()
        //     .and_then(|o| o.lazy_load)
        //     .unwrap_or(true);
        //
        // let index_data = self.storage.load(&cid).await?;
        // self.index.load_from_bytes(&index_data, lazy_load)?;

        Ok(())
    }

    /// Search for similar vectors
    #[napi]
    pub async fn search(
        &self,
        _query_vector: Vec<f64>,
        _k: u32,
        _options: Option<SearchOptions>,
    ) -> Result<Vec<SearchResult>> {
        // TODO: Phase 2 - Implement search
        // let threshold = options.as_ref()
        //     .and_then(|o| o.threshold)
        //     .unwrap_or(0.7);
        //
        // let results = self.index.search(&query_vector, k)?;
        // Filter and map results

        Ok(vec![])
    }

    /// Add vectors to the index
    #[napi]
    pub async unsafe fn add_vectors(&mut self, _vectors: Vec<VectorInput>) -> Result<()> {
        // TODO: Phase 2 - Implement add vectors
        // for input in vectors {
        //     self.index.add(input.into())?;
        // }

        Ok(())
    }

    /// Save index to S5
    #[napi]
    pub async fn save_to_s5(&self) -> Result<String> {
        // TODO: Phase 2 - Implement S5 save
        // let index_bytes = self.index.to_bytes()?;
        // let cid = self.storage.store(&index_bytes).await?;

        Ok("placeholder_cid".to_string())
    }

    /// Get session statistics
    #[napi]
    pub fn get_stats(&self) -> Result<SessionStats> {
        // TODO: Phase 2 - Get real stats from index
        Ok(SessionStats {
            vector_count: 0,
            memory_usage_mb: 0.0,
            index_type: "hybrid".to_string(),
            hnsw_vector_count: Some(0),
            ivf_vector_count: Some(0),
        })
    }

    /// Destroy session and clear memory
    #[napi]
    pub async unsafe fn destroy(&mut self) -> Result<()> {
        // TODO: Phase 2 - Clear index and storage
        // if let Some(state) = self.state.take() {
        //     state.index.clear()?;
        // }

        Ok(())
    }
}

// Ensure cleanup on drop
impl Drop for VectorDBSession {
    fn drop(&mut self) {
        // TODO: Phase 2 - Add cleanup warning if state exists
        eprintln!("VectorDBSession '{}' dropped", self.session_id);
    }
}
