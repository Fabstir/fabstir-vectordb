// Copyright (c) 2025 Fabstir
// SPDX-License-Identifier: BUSL-1.1

use crate::core::storage::S5Storage;
use crate::core::types::{SearchResult, VectorId};
use crate::hnsw::core::{HNSWConfig, HNSWIndex};
use crate::ivf::core::{ClusterId, IVFConfig, IVFIndex};
use crate::storage::chunk_loader::ChunkLoader;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use thiserror::Error;
use tokio::sync::RwLock;

#[derive(Debug, Error)]
pub enum HybridError {
    #[error("HNSW error: {0}")]
    HNSW(String),

    #[error("IVF error: {0}")]
    IVF(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("Index not initialized")]
    NotInitialized,

    #[error("Dimension mismatch: expected {expected}, got {actual}")]
    DimensionMismatch { expected: usize, actual: usize },

    #[error("Vector with ID {0:?} already exists")]
    DuplicateVector(VectorId),
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct HybridConfig {
    #[serde(with = "duration_serde")]
    pub recent_threshold: Duration,
    pub hnsw_config: HNSWConfig,
    pub ivf_config: IVFConfig,
    pub migration_batch_size: usize,
    pub auto_migrate: bool,
}

// Helper module for std::time::Duration serialization
mod duration_serde {
    use serde::{Deserialize, Deserializer, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(duration.as_secs())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let seconds = u64::deserialize(deserializer)?;
        Ok(Duration::from_secs(seconds))
    }
}

impl Default for HybridConfig {
    fn default() -> Self {
        let mut ivf_config = IVFConfig::default();
        ivf_config.train_size = 9; // Smaller for tests
        ivf_config.n_clusters = 3; // Fewer clusters for tests
        ivf_config.n_probe = 2; // Must be <= n_clusters

        Self {
            recent_threshold: Duration::from_secs(7 * 24 * 3600), // 7 days
            hnsw_config: HNSWConfig::default(),
            ivf_config,
            migration_batch_size: 100,
            auto_migrate: true,
        }
    }
}

impl HybridConfig {
    pub fn is_valid(&self) -> bool {
        self.recent_threshold.as_secs() > 0 && self.migration_batch_size > 0
    }
}

#[derive(Debug, Clone)]
pub struct TimestampedVector {
    pub id: VectorId,
    pub vector: Vec<f32>,
    pub timestamp: SystemTime,
}

impl TimestampedVector {
    pub fn new(id: VectorId, vector: Vec<f32>, timestamp: DateTime<Utc>) -> Self {
        Self {
            id,
            vector,
            timestamp: timestamp.into(),
        }
    }

    pub fn id(&self) -> &VectorId {
        &self.id
    }

    pub fn vector(&self) -> &Vec<f32> {
        &self.vector
    }

    pub fn timestamp(&self) -> SystemTime {
        self.timestamp
    }

    pub fn is_recent(&self, threshold: Duration) -> bool {
        if let Ok(elapsed) = self.timestamp.elapsed() {
            elapsed < threshold
        } else {
            false
        }
    }
}

#[derive(Debug, Clone)]
pub struct HybridStats {
    pub recent_vectors: usize,
    pub historical_vectors: usize,
    pub total_vectors: usize,
    pub avg_vector_age_ms: f64,
    pub recent_index_memory: usize,
    pub historical_index_memory: usize,
    pub avg_query_time_ms: f32,
}

#[derive(Debug, Clone)]
pub struct MigrationResult {
    pub vectors_migrated: usize,
}

#[derive(Debug, Clone)]
pub struct AgeDistribution {
    pub under_1_hour: usize,
    pub under_1_day: usize,
    pub under_1_week: usize,
    pub over_1_week: usize,
    pub total_vectors: usize,
    pub buckets: Vec<(String, usize)>,
    pub newest_timestamp: DateTime<Utc>,
    pub oldest_timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct DeleteStats {
    pub successful: usize,
    pub failed: usize,
    pub errors: Vec<(VectorId, String)>,
}

#[derive(Debug, Clone)]
pub struct VacuumStats {
    pub hnsw_removed: usize,
    pub ivf_removed: usize,
    pub total_removed: usize,
}

#[derive(Debug, Clone)]
pub struct HybridSearchConfig {
    pub search_recent: bool,
    pub search_historical: bool,
    pub recent_k: usize,
    pub historical_k: usize,
    pub recent_threshold_override: Option<Duration>,
    pub k: usize,
    pub hnsw_ef: usize,
    pub ivf_n_probe: usize,
}

impl Default for HybridSearchConfig {
    fn default() -> Self {
        Self {
            search_recent: true,
            search_historical: true,
            recent_k: 0,
            historical_k: 0,
            recent_threshold_override: None,
            k: 10,
            hnsw_ef: 50,
            ivf_n_probe: 10,
        }
    }
}

pub type SearchConfig = HybridSearchConfig;

#[derive(Clone)]
pub struct HybridIndex {
    config: HybridConfig,
    recent_index: Arc<RwLock<HNSWIndex>>,
    historical_index: Arc<RwLock<IVFIndex>>,
    pub timestamps: Arc<RwLock<std::collections::HashMap<VectorId, DateTime<Utc>>>>,
    initialized: bool,
    recent_count: Arc<RwLock<usize>>,
    historical_count: Arc<RwLock<usize>>,
    /// Chunk loader for lazy loading vectors from S5 storage (shared between HNSW and IVF)
    chunk_loader: Option<Arc<ChunkLoader>>,
}

impl HybridIndex {
    pub fn new(config: HybridConfig) -> Self {
        let recent_index = Arc::new(RwLock::new(HNSWIndex::new(config.hnsw_config.clone())));
        let historical_index = Arc::new(RwLock::new(IVFIndex::new(config.ivf_config.clone())));

        Self {
            config,
            recent_index,
            historical_index,
            timestamps: Arc::new(RwLock::new(std::collections::HashMap::new())),
            initialized: false,
            recent_count: Arc::new(RwLock::new(0)),
            historical_count: Arc::new(RwLock::new(0)),
            chunk_loader: None,
        }
    }

    /// Create a new HybridIndex with chunk loader for lazy loading support
    pub fn with_chunk_loader(config: HybridConfig, chunk_loader: Option<Arc<ChunkLoader>>) -> Self {
        // Create indices with chunk loader
        let recent_index = Arc::new(RwLock::new(HNSWIndex::with_chunk_loader(
            config.hnsw_config.clone(),
            chunk_loader.clone(),
        )));
        let historical_index = Arc::new(RwLock::new(IVFIndex::with_chunk_loader(
            config.ivf_config.clone(),
            chunk_loader.clone(),
        )));

        Self {
            config,
            recent_index,
            historical_index,
            timestamps: Arc::new(RwLock::new(std::collections::HashMap::new())),
            initialized: false,
            recent_count: Arc::new(RwLock::new(0)),
            historical_count: Arc::new(RwLock::new(0)),
            chunk_loader,
        }
    }

    pub async fn with_storage(_storage: Arc<dyn S5Storage>) -> Self {
        Self::new(HybridConfig::default())
    }

    pub async fn initialize(&mut self, training_data: Vec<Vec<f32>>) -> Result<(), HybridError> {
        // Train the IVF index
        let mut historical = self.historical_index.write().await;
        historical
            .train(&training_data)
            .map_err(|e| HybridError::IVF(e.to_string()))?;

        // Clear any training data that was inserted
        historical.inverted_lists.clear();
        historical.total_vectors = 0;
        for i in 0..historical.config.n_clusters {
            historical
                .inverted_lists
                .insert(ClusterId(i), crate::ivf::core::InvertedList::new());
        }
        drop(historical);

        self.initialized = true;
        Ok(())
    }

    pub fn config(&self) -> &HybridConfig {
        &self.config
    }

    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    pub async fn insert(&self, id: VectorId, vector: Vec<f32>) -> Result<(), HybridError> {
        self.insert_with_timestamp(id, vector, Utc::now()).await
    }

    /// Insert a vector with chunk reference for lazy loading support
    pub async fn insert_with_chunk(
        &self,
        id: VectorId,
        vector: Vec<f32>,
        timestamp: DateTime<Utc>,
        chunk_id: Option<String>,
    ) -> Result<(), HybridError> {
        if !self.initialized {
            return Err(HybridError::NotInitialized);
        }

        // Check for duplicates
        let timestamps = self.timestamps.read().await;
        if timestamps.contains_key(&id) {
            return Err(HybridError::DuplicateVector(id));
        }
        drop(timestamps);

        // Determine if vector is recent or historical
        let now = Utc::now();
        let age = now
            .signed_duration_since(timestamp)
            .to_std()
            .unwrap_or(Duration::from_secs(0));

        if age < self.config.recent_threshold {
            // Insert into HNSW (recent) with chunk reference
            let mut recent = self.recent_index.write().await;
            recent
                .insert_with_chunk(id.clone(), vector, chunk_id)
                .map_err(|e| HybridError::HNSW(e.to_string()))?;

            let mut count = self.recent_count.write().await;
            *count += 1;
        } else {
            // Insert into IVF (historical) with chunk reference
            let mut historical = self.historical_index.write().await;
            historical
                .insert_with_chunk(id.clone(), vector, chunk_id)
                .map_err(|e| HybridError::IVF(e.to_string()))?;

            let mut count = self.historical_count.write().await;
            *count += 1;
        }

        // Store timestamp
        let mut timestamps = self.timestamps.write().await;
        timestamps.insert(id, timestamp);

        Ok(())
    }

    pub async fn insert_with_timestamp(
        &self,
        id: VectorId,
        vector: Vec<f32>,
        timestamp: DateTime<Utc>,
    ) -> Result<(), HybridError> {
        if !self.initialized {
            return Err(HybridError::NotInitialized);
        }

        // Check for duplicates
        let timestamps = self.timestamps.read().await;
        if timestamps.contains_key(&id) {
            return Err(HybridError::DuplicateVector(id));
        }
        drop(timestamps);

        // Determine if vector is recent or historical
        let now = Utc::now();
        let age = now
            .signed_duration_since(timestamp)
            .to_std()
            .unwrap_or(Duration::from_secs(0));

        if age < self.config.recent_threshold {
            // Insert into HNSW (recent)
            let mut recent = self.recent_index.write().await;
            recent
                .insert(id.clone(), vector)
                .map_err(|e| HybridError::HNSW(e.to_string()))?;

            let mut count = self.recent_count.write().await;
            *count += 1;
        } else {
            // Insert into IVF (historical)
            let mut historical = self.historical_index.write().await;
            historical
                .insert(id.clone(), vector)
                .map_err(|e| HybridError::IVF(e.to_string()))?;

            let mut count = self.historical_count.write().await;
            *count += 1;
        }

        // Store timestamp
        let mut timestamps = self.timestamps.write().await;
        timestamps.insert(id, timestamp);

        Ok(())
    }

    pub async fn search(&self, query: &[f32], k: usize) -> Result<Vec<SearchResult>, HybridError> {
        let mut config = SearchConfig::default();
        config.k = k;
        self.search_with_config(query, config).await
    }

    pub async fn search_with_config(
        &self,
        query: &[f32],
        config: SearchConfig,
    ) -> Result<Vec<SearchResult>, HybridError> {
        let k = config.k;
        if !self.initialized {
            // Return empty results for uninitialized index
            return Ok(Vec::new());
        }

        // Auto-migrate if enabled
        if self.config.auto_migrate {
            self.migrate_old_vectors().await?;
        }

        let mut all_results = Vec::new();

        // Determine k values for each index
        let recent_k = if config.recent_k > 0 {
            config.recent_k
        } else {
            k
        };
        let historical_k = if config.historical_k > 0 {
            config.historical_k
        } else {
            k
        };

        // Search recent vectors
        if config.search_recent {
            let recent = self.recent_index.read().await;
            let ef = config.hnsw_ef;
            if let Ok(recent_results) = recent.search(query, recent_k, ef) {
                all_results.extend(recent_results);
            }
        }

        // Search historical vectors
        if config.search_historical {
            let historical = self.historical_index.read().await;
            // Use custom n_probe if specified
            if config.ivf_n_probe != historical.config().n_probe {
                if let Ok(historical_results) =
                    historical.search_with_config(query, historical_k, config.ivf_n_probe).await
                {
                    all_results.extend(historical_results);
                }
            } else {
                if let Ok(historical_results) = historical.search(query, historical_k).await {
                    all_results.extend(historical_results);
                }
            }
        }

        // Sort by distance and take top k
        all_results.sort_by(|a, b| a.distance.partial_cmp(&b.distance).unwrap());
        all_results.truncate(k);

        Ok(all_results)
    }

    /// Search with metadata filtering
    ///
    /// Implements k-oversampling strategy: retrieves more candidates than k,
    /// filters by metadata, then truncates to k results.
    ///
    /// # Arguments
    /// * `query` - Query vector
    /// * `k` - Number of results to return
    /// * `filter` - Optional metadata filter
    /// * `metadata_map` - HashMap mapping vector IDs to metadata
    ///
    /// # Returns
    /// Filtered search results, sorted by distance, limited to k results
    ///
    /// # Example
    /// ```ignore
    /// use serde_json::json;
    /// use vector_db::core::metadata_filter::MetadataFilter;
    ///
    /// let filter = MetadataFilter::from_json(&json!({
    ///     "category": "technology"
    /// })).unwrap();
    ///
    /// let results = index.search_with_filter(&query, 10, Some(&filter), &metadata_map).await?;
    /// ```
    pub async fn search_with_filter(
        &self,
        query: &[f32],
        k: usize,
        filter: Option<&crate::core::metadata_filter::MetadataFilter>,
        metadata_map: &std::collections::HashMap<String, serde_json::Value>,
    ) -> Result<Vec<SearchResult>, HybridError> {
        // If no filter, use regular search
        if filter.is_none() {
            return self.search(query, k).await;
        }

        let filter = filter.unwrap();

        // Use k-oversampling: search for more results to account for filtering
        // Default multiplier of 3x (configurable in future)
        let k_oversample = k * 3;

        // Get oversampled results
        let candidates = self.search(query, k_oversample).await?;

        // Filter results by metadata
        let mut filtered_results = Vec::new();
        for result in candidates {
            let vector_id_str = result.vector_id.to_string();
            if let Some(metadata) = metadata_map.get(&vector_id_str) {
                if filter.matches(metadata) {
                    filtered_results.push(result);
                }
            }
        }

        // Truncate to k results (already sorted by distance from search)
        filtered_results.truncate(k);

        Ok(filtered_results)
    }

    pub async fn migrate_old_vectors(&self) -> Result<MigrationResult, HybridError> {
        let count = self
            .migrate_with_threshold(self.config.recent_threshold)
            .await?;
        Ok(MigrationResult {
            vectors_migrated: count,
        })
    }

    pub async fn migrate_specific_vectors(
        &self,
        vector_ids: &[VectorId],
    ) -> Result<MigrationResult, HybridError> {
        let mut migrated_count = 0;

        // Process in batches
        for batch in vector_ids.chunks(self.config.migration_batch_size) {
            let recent = self.recent_index.write().await;
            let mut historical = self.historical_index.write().await;

            for id in batch {
                // Get vector from recent index
                if let Some(node) = recent.get_node(id) {
                    let vector = node.vector().clone();

                    // Insert into historical
                    if historical.insert(id.clone(), vector).is_ok() {
                        // Remove from recent
                        // Note: HNSW doesn't have remove, so we'd need to track deleted nodes
                        migrated_count += 1;
                    }
                }
            }
        }

        // Update counts
        if migrated_count > 0 {
            let mut recent_count = self.recent_count.write().await;
            *recent_count = recent_count.saturating_sub(migrated_count);

            let mut historical_count = self.historical_count.write().await;
            *historical_count += migrated_count;
        }

        Ok(MigrationResult {
            vectors_migrated: migrated_count,
        })
    }

    pub async fn migrate_with_threshold(&self, threshold: Duration) -> Result<usize, HybridError> {
        let now = Utc::now();
        let mut migrated_count = 0;

        // Get vectors to migrate
        let timestamps = self.timestamps.read().await;
        let mut vectors_to_migrate = Vec::new();

        for (id, timestamp) in timestamps.iter() {
            let age = now
                .signed_duration_since(*timestamp)
                .to_std()
                .unwrap_or(Duration::from_secs(0));
            if age >= threshold {
                vectors_to_migrate.push(id.clone());
            }
        }
        drop(timestamps);

        // Migrate in batches
        for batch in vectors_to_migrate.chunks(self.config.migration_batch_size) {
            let recent = self.recent_index.write().await;
            let mut historical = self.historical_index.write().await;

            for id in batch {
                // Get vector from recent index
                if let Some(node) = recent.get_node(id) {
                    let vector = node.vector().clone();

                    // Insert into historical
                    if historical.insert(id.clone(), vector).is_ok() {
                        // Remove from recent
                        // Note: HNSW doesn't have remove, so we'd need to track deleted nodes
                        migrated_count += 1;
                    }
                }
            }
        }

        // Update counts
        if migrated_count > 0 {
            let mut recent_count = self.recent_count.write().await;
            *recent_count = recent_count.saturating_sub(migrated_count);

            let mut historical_count = self.historical_count.write().await;
            *historical_count += migrated_count;
        }

        Ok(migrated_count)
    }

    pub fn is_in_recent(&self, id: &VectorId) -> bool {
        // Check if the vector exists and is recent
        if let Ok(timestamps) = self.timestamps.try_read() {
            if let Some(timestamp) = timestamps.get(id) {
                let now = Utc::now();
                let age = now
                    .signed_duration_since(*timestamp)
                    .to_std()
                    .unwrap_or(Duration::from_secs(0));
                return age < self.config.recent_threshold;
            }
        }
        false
    }

    pub fn is_in_historical(&self, id: &VectorId) -> bool {
        // Check if the vector exists and is historical
        if let Ok(timestamps) = self.timestamps.try_read() {
            if let Some(timestamp) = timestamps.get(id) {
                let now = Utc::now();
                let age = now
                    .signed_duration_since(*timestamp)
                    .to_std()
                    .unwrap_or(Duration::from_secs(0));
                return age >= self.config.recent_threshold;
            }
        }
        false
    }

    pub async fn start_auto_migration(&self) -> Result<(), HybridError> {
        // For now, just trigger a migration immediately
        // In a real implementation, this would start a background task
        self.migrate_old_vectors().await?;
        Ok(())
    }

    pub async fn stop_auto_migration(&self) -> Result<(), HybridError> {
        // Stop auto migration background task
        // In a real implementation, this would cancel the background task
        Ok(())
    }

    pub async fn get_statistics(&self) -> HybridStats {
        let recent = self.recent_index.read().await;
        let historical = self.historical_index.read().await;
        let timestamps = self.timestamps.read().await;

        let recent_count = recent.node_count();
        let historical_count = historical.total_vectors();
        let total = recent_count + historical_count;

        let now = Utc::now();
        let avg_age_ms = if total > 0 {
            let total_age_ms: i64 = timestamps
                .values()
                .map(|ts| now.signed_duration_since(*ts).num_milliseconds())
                .sum();
            total_age_ms as f64 / total as f64
        } else {
            0.0
        };

        // Estimate memory usage
        let recent_memory = recent_count * 500; // Rough estimate: 500 bytes per HNSW node
        let historical_memory = historical_count * 100; // Rough estimate: 100 bytes per IVF vector

        HybridStats {
            recent_vectors: recent_count,
            historical_vectors: historical_count,
            total_vectors: total,
            avg_vector_age_ms: avg_age_ms,
            recent_index_memory: recent_memory,
            historical_index_memory: historical_memory,
            avg_query_time_ms: 0.0,
        }
    }

    pub fn get_stats(&self) -> HybridStats {
        // Synchronous stats - use try_read() for non-blocking access
        let recent_count = self.recent_count.try_read()
            .map(|c| *c)
            .unwrap_or(0);
        let historical_count = self.historical_count.try_read()
            .map(|c| *c)
            .unwrap_or(0);
        let total = recent_count + historical_count;

        // Calculate memory usage from indices
        let recent_memory = self.recent_index.try_read()
            .map(|index| index.estimate_memory_usage().total_bytes)
            .unwrap_or(0);
        let historical_memory = self.historical_index.try_read()
            .map(|index| index.estimate_memory_usage().total_bytes)
            .unwrap_or(0);

        HybridStats {
            recent_vectors: recent_count,
            historical_vectors: historical_count,
            total_vectors: total,
            avg_vector_age_ms: 0.0, // TODO: Calculate from timestamps
            recent_index_memory: recent_memory,
            historical_index_memory: historical_memory,
            avg_query_time_ms: 0.0, // TODO: Track query times
        }
    }

    pub async fn get_age_distribution(&self) -> Result<AgeDistribution, HybridError> {
        let timestamps = self.timestamps.read().await;
        let now = Utc::now();

        let mut under_1_hour = 0;
        let mut under_1_day = 0;
        let mut under_1_week = 0;
        let mut over_1_week = 0;

        for timestamp in timestamps.values() {
            let age = now
                .signed_duration_since(*timestamp)
                .to_std()
                .unwrap_or(Duration::from_secs(0));

            if age < Duration::from_secs(3600) {
                under_1_hour += 1;
            } else if age < Duration::from_secs(24 * 3600) {
                under_1_day += 1;
            } else if age < Duration::from_secs(7 * 24 * 3600) {
                under_1_week += 1;
            } else {
                over_1_week += 1;
            }
        }

        let total = under_1_hour + under_1_day + under_1_week + over_1_week;

        let buckets = vec![
            ("< 1 hour".to_string(), under_1_hour),
            ("< 1 day".to_string(), under_1_day),
            ("< 1 week".to_string(), under_1_week),
            ("> 1 week".to_string(), over_1_week),
        ];

        // Find newest and oldest timestamps
        let (newest, oldest) = if timestamps.is_empty() {
            (now, now)
        } else {
            let mut newest = timestamps.values().next().unwrap();
            let mut oldest = newest;
            for ts in timestamps.values() {
                if ts > newest {
                    newest = ts;
                }
                if ts < oldest {
                    oldest = ts;
                }
            }
            (*newest, *oldest)
        };

        Ok(AgeDistribution {
            under_1_hour,
            under_1_day,
            under_1_week,
            over_1_week,
            total_vectors: total,
            buckets,
            newest_timestamp: newest,
            oldest_timestamp: oldest,
        })
    }

    pub fn total_vectors(&self) -> usize {
        // Use try_read to avoid blocking
        let recent = self.recent_count.try_read().map(|c| *c).unwrap_or(0);
        let historical = self.historical_count.try_read().map(|c| *c).unwrap_or(0);
        recent + historical
    }

    pub fn recent_count(&self) -> usize {
        self.recent_count.try_read().map(|c| *c).unwrap_or(0)
    }

    pub fn historical_count(&self) -> usize {
        self.historical_count.try_read().map(|c| *c).unwrap_or(0)
    }

    /// Get timestamps (for persistence)
    pub async fn get_timestamps(&self) -> HashMap<VectorId, DateTime<Utc>> {
        self.timestamps.read().await.clone()
    }

    /// Get read guard to recent index (for persistence)
    pub async fn get_recent_index(&self) -> tokio::sync::RwLockReadGuard<'_, HNSWIndex> {
        self.recent_index.read().await
    }

    /// Get read guard to historical index (for persistence)
    pub async fn get_historical_index(&self) -> tokio::sync::RwLockReadGuard<'_, IVFIndex> {
        self.historical_index.read().await
    }

    /// Reconstruct HybridIndex from parts (for deserialization)
    pub fn from_parts(
        config: HybridConfig,
        recent_index: HNSWIndex,
        historical_index: IVFIndex,
        timestamps: HashMap<VectorId, DateTime<Utc>>,
        recent_count: usize,
        historical_count: usize,
    ) -> Result<Self, HybridError> {
        Ok(Self {
            config,
            recent_index: Arc::new(RwLock::new(recent_index)),
            historical_index: Arc::new(RwLock::new(historical_index)),
            timestamps: Arc::new(RwLock::new(timestamps)),
            initialized: true,
            recent_count: Arc::new(RwLock::new(recent_count)),
            historical_count: Arc::new(RwLock::new(historical_count)),
            chunk_loader: None,
        })
    }

    /// Reconstruct HybridIndex from parts with chunk loader (for chunked deserialization)
    pub fn from_parts_with_chunk_loader(
        config: HybridConfig,
        recent_index: HNSWIndex,
        historical_index: IVFIndex,
        timestamps: HashMap<VectorId, DateTime<Utc>>,
        recent_count: usize,
        historical_count: usize,
        chunk_loader: Option<Arc<ChunkLoader>>,
    ) -> Result<Self, HybridError> {
        Ok(Self {
            config,
            recent_index: Arc::new(RwLock::new(recent_index)),
            historical_index: Arc::new(RwLock::new(historical_index)),
            timestamps: Arc::new(RwLock::new(timestamps)),
            initialized: true,
            recent_count: Arc::new(RwLock::new(recent_count)),
            historical_count: Arc::new(RwLock::new(historical_count)),
            chunk_loader,
        })
    }

    /// Delete a vector from the index (soft deletion)
    pub async fn delete(&self, id: VectorId) -> Result<(), HybridError> {
        // Check if vector exists by looking up timestamp
        let timestamps = self.timestamps.read().await;
        let timestamp = timestamps
            .get(&id)
            .ok_or_else(|| HybridError::IVF(format!("Vector {:?} not found", id)))?;

        // Determine which index the vector is in based on its age
        let now = Utc::now();
        let age = now
            .signed_duration_since(*timestamp)
            .to_std()
            .unwrap_or(Duration::from_secs(0));

        drop(timestamps);

        // Delete from appropriate index
        if age < self.config.recent_threshold {
            // Delete from HNSW (recent)
            let mut recent = self.recent_index.write().await;
            recent
                .mark_deleted(&id)
                .map_err(|e| HybridError::HNSW(e.to_string()))?;
        } else {
            // Delete from IVF (historical)
            let mut historical = self.historical_index.write().await;
            historical
                .mark_deleted(&id)
                .map_err(|e| HybridError::IVF(e.to_string()))?;
        }

        Ok(())
    }

    /// Check if a vector is marked as deleted
    pub async fn is_deleted(&self, id: &VectorId) -> bool {
        // Check if vector exists in timestamps
        let timestamps = self.timestamps.read().await;
        if let Some(timestamp) = timestamps.get(id) {
            // Determine which index to check
            let now = Utc::now();
            let age = now
                .signed_duration_since(*timestamp)
                .to_std()
                .unwrap_or(Duration::from_secs(0));

            drop(timestamps);

            if age < self.config.recent_threshold {
                // Check HNSW
                let recent = self.recent_index.read().await;
                recent.is_deleted(id)
            } else {
                // Check IVF
                let historical = self.historical_index.read().await;
                historical.is_deleted(id)
            }
        } else {
            // Vector doesn't exist, so it's not deleted
            false
        }
    }

    /// Delete multiple vectors (batch operation)
    pub async fn batch_delete(&self, ids: &[VectorId]) -> Result<DeleteStats, HybridError> {
        let mut stats = DeleteStats {
            successful: 0,
            failed: 0,
            errors: Vec::new(),
        };

        for id in ids {
            match self.delete(id.clone()).await {
                Ok(_) => stats.successful += 1,
                Err(e) => {
                    stats.failed += 1;
                    stats.errors.push((id.clone(), e.to_string()));
                }
            }
        }

        Ok(stats)
    }

    /// Physically remove deleted vectors from both indices
    pub async fn vacuum(&self) -> Result<VacuumStats, HybridError> {
        // Vacuum HNSW index
        let mut recent = self.recent_index.write().await;
        let hnsw_removed = recent
            .vacuum()
            .map_err(|e| HybridError::HNSW(e.to_string()))?;
        drop(recent);

        // Vacuum IVF index
        let mut historical = self.historical_index.write().await;
        let ivf_removed = historical
            .vacuum()
            .map_err(|e| HybridError::IVF(e.to_string()))?;
        drop(historical);

        let total_removed = hnsw_removed + ivf_removed;

        Ok(VacuumStats {
            hnsw_removed,
            ivf_removed,
            total_removed,
        })
    }

    /// Get count of active (non-deleted) vectors
    pub async fn active_count(&self) -> usize {
        // Get active count from both indices
        let recent = self.recent_index.read().await;
        let recent_active = recent.active_count();
        drop(recent);

        let historical = self.historical_index.read().await;
        let historical_active = historical.active_count();
        drop(historical);

        recent_active + historical_active
    }

    /// Get deletion statistics (hnsw_deleted, ivf_deleted, total_deleted)
    pub async fn deletion_stats(&self) -> (usize, usize, usize) {
        // Get deleted count from HNSW index
        let recent = self.recent_index.read().await;
        let hnsw_nodes = recent.get_all_nodes();
        let hnsw_deleted = hnsw_nodes.iter().filter(|n| n.is_deleted()).count();
        drop(recent);

        // Get deleted count from IVF index
        let historical = self.historical_index.read().await;
        let ivf_deleted = historical.deleted.len();
        drop(historical);

        let total_deleted = hnsw_deleted + ivf_deleted;

        (hnsw_deleted, ivf_deleted, total_deleted)
    }

    /// Get all deleted vector IDs from both indices
    /// Used for persistence - save deleted vectors in manifest
    pub async fn get_deleted_vectors(&self) -> Vec<String> {
        let mut deleted_ids = Vec::new();

        // Get deleted vectors from HNSW index
        let recent = self.recent_index.read().await;
        let hnsw_nodes = recent.get_all_nodes();
        for node in hnsw_nodes {
            if node.is_deleted() {
                deleted_ids.push(node.id().to_string());
            }
        }
        drop(recent);

        // Get deleted vectors from IVF index
        let historical = self.historical_index.read().await;
        for deleted_id in historical.get_deleted_ids() {
            deleted_ids.push(deleted_id.to_string());
        }
        drop(historical);

        deleted_ids
    }
}
