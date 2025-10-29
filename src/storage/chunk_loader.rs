// Copyright (c) 2025 Fabstir
// SPDX-License-Identifier: BUSL-1.1

use std::collections::HashMap;
use std::error::Error;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{RwLock, Mutex};
use crate::core::storage::S5Storage;
use crate::core::chunk_cache::ChunkCache;
use crate::core::chunk::VectorChunk;

/// ChunkLoader handles loading vector chunks from S5 storage with caching,
/// retry logic, and request deduplication for parallel operations.
#[derive(Clone)]
pub struct ChunkLoader {
    storage: Arc<dyn S5Storage>,
    cache: Arc<ChunkCache>,
    /// Tracks in-flight requests to prevent duplicate loads
    in_flight: Arc<RwLock<HashMap<String, Arc<Mutex<()>>>>>,
}

impl ChunkLoader {
    /// Create a new ChunkLoader
    pub fn new(storage: Arc<dyn S5Storage>, cache: Arc<ChunkCache>) -> Self {
        Self {
            storage,
            cache,
            in_flight: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Load a single chunk from storage with caching and retry logic
    ///
    /// # Process
    /// 1. Check cache first
    /// 2. If not cached, load from S5 with retry logic
    /// 3. Deserialize CBOR data
    /// 4. Store in cache
    /// 5. Return chunk
    ///
    /// # Retry Logic
    /// - Max 3 attempts
    /// - Exponential backoff: 100ms, 200ms, 400ms
    pub async fn load_chunk(&self, chunk_path: &str) -> Result<VectorChunk, Box<dyn Error + Send + Sync>> {
        // Step 1: Check cache first
        if let Some(chunk) = self.cache.get(chunk_path) {
            return Ok(chunk);
        }

        // Step 2: Get or create in-flight lock for this path (deduplication)
        let lock = {
            let mut in_flight = self.in_flight.write().await;
            in_flight
                .entry(chunk_path.to_string())
                .or_insert_with(|| Arc::new(Mutex::new(())))
                .clone()
        };

        // Acquire the lock - if another task is loading, we wait
        let _guard = lock.lock().await;

        // Double-check cache after acquiring lock (another task may have loaded it)
        if let Some(chunk) = self.cache.get(chunk_path) {
            return Ok(chunk);
        }

        // Step 3: Load from S5 with retry logic
        let chunk_data = self.retry_load(chunk_path).await?;

        // Step 4: Deserialize CBOR data
        let chunk: VectorChunk = serde_cbor::from_slice(&chunk_data)
            .map_err(|e| format!("Failed to deserialize chunk '{}': {}", chunk_path, e))?;

        // Step 5: Store in cache
        self.cache.put(chunk_path.to_string(), chunk.clone());

        // Cleanup in-flight entry
        {
            let mut in_flight = self.in_flight.write().await;
            in_flight.remove(chunk_path);
        }

        Ok(chunk)
    }

    /// Load multiple chunks in parallel with deduplication
    ///
    /// # Process
    /// 1. Spawn parallel tasks for each chunk
    /// 2. Use load_chunk() which handles caching and deduplication
    /// 3. Collect all results
    /// 4. Return chunks in original order
    pub async fn load_chunks_parallel(
        &self,
        chunk_paths: Vec<&str>,
    ) -> Result<Vec<VectorChunk>, Box<dyn Error + Send + Sync>> {
        // Spawn tasks for parallel loading
        let mut tasks = Vec::new();

        for path in chunk_paths {
            let loader = self.clone();
            let path_owned = path.to_string();

            let task = tokio::spawn(async move {
                loader.load_chunk(&path_owned).await
            });

            tasks.push(task);
        }

        // Collect results
        let mut chunks = Vec::new();
        for task in tasks {
            let chunk = task.await
                .map_err(|e| format!("Parallel load task failed: {}", e))??;
            chunks.push(chunk);
        }

        Ok(chunks)
    }

    /// Retry logic with exponential backoff
    ///
    /// Attempts: 3 max
    /// Backoff: 100ms, 200ms, 400ms
    async fn retry_load(&self, path: &str) -> Result<Vec<u8>, Box<dyn Error + Send + Sync>> {
        const MAX_RETRIES: u32 = 3;
        const BASE_DELAY_MS: u64 = 100;

        let mut last_error = None;

        for attempt in 0..MAX_RETRIES {
            match self.storage.get(path).await {
                Ok(Some(data)) => {
                    if attempt > 0 {
                        eprintln!("Successfully loaded '{}' after {} retries", path, attempt);
                    }
                    return Ok(data);
                }
                Ok(None) => {
                    // Not found - don't retry
                    return Err(format!("Chunk not found: {}", path).into());
                }
                Err(e) => {
                    last_error = Some(e);

                    // Only wait if we have retries remaining
                    if attempt < MAX_RETRIES - 1 {
                        let delay_ms = BASE_DELAY_MS * (1 << attempt); // Exponential: 100, 200, 400
                        eprintln!(
                            "Load attempt {} failed for '{}', retrying in {}ms",
                            attempt + 1,
                            path,
                            delay_ms
                        );
                        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                    }
                }
            }
        }

        Err(format!(
            "Failed to load chunk '{}' after {} attempts: {}",
            path,
            MAX_RETRIES,
            last_error
                .map(|e| e.to_string())
                .unwrap_or_else(|| "Unknown error".to_string())
        )
        .into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::storage::MockS5Storage;
    use crate::core::types::VectorId;

    #[tokio::test]
    async fn test_chunk_loader_basic() {
        let storage = Arc::new(MockS5Storage::new());
        let cache = Arc::new(ChunkCache::new(100));
        let loader = ChunkLoader::new(storage.clone(), cache.clone());

        // Create test chunk
        let mut chunk = VectorChunk::new("test".to_string(), 0, 1);
        chunk.add_vector(VectorId::from_string("test_1"), vec![1.0, 2.0, 3.0, 4.0]);
        chunk.add_vector(VectorId::from_string("test_2"), vec![5.0, 6.0, 7.0, 8.0]);

        let chunk_data = serde_cbor::to_vec(&chunk).unwrap();
        storage.put("test/chunk.cbor", chunk_data).await.unwrap();

        // Load chunk
        let loaded = loader.load_chunk("test/chunk.cbor").await.unwrap();
        assert_eq!(loaded.vectors.len(), 2);

        // Verify cache hit on second load
        let loaded2 = loader.load_chunk("test/chunk.cbor").await.unwrap();
        assert_eq!(loaded2.vectors.len(), 2);
    }

    #[tokio::test]
    async fn test_chunk_loader_not_found() {
        let storage = Arc::new(MockS5Storage::new());
        let cache = Arc::new(ChunkCache::new(100));
        let loader = ChunkLoader::new(storage, cache);

        let result = loader.load_chunk("nonexistent.cbor").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[tokio::test]
    async fn test_parallel_loading() {
        let storage = Arc::new(MockS5Storage::new());
        let cache = Arc::new(ChunkCache::new(100));
        let loader = ChunkLoader::new(storage.clone(), cache);

        // Create multiple chunks
        for i in 0..5 {
            let chunk_id = format!("chunk_{}", i);
            let mut chunk = VectorChunk::new(chunk_id, 0, 0);
            let vec_id = format!("vec_{}", i);
            chunk.add_vector(VectorId::from_string(&vec_id), vec![i as f32; 4]);
            let chunk_data = serde_cbor::to_vec(&chunk).unwrap();
            storage.put(&format!("test/chunk_{}.cbor", i), chunk_data).await.unwrap();
        }

        // Load in parallel
        let paths = vec![
            "test/chunk_0.cbor",
            "test/chunk_1.cbor",
            "test/chunk_2.cbor",
            "test/chunk_3.cbor",
            "test/chunk_4.cbor",
        ];

        let chunks = loader.load_chunks_parallel(paths).await.unwrap();
        assert_eq!(chunks.len(), 5);
    }
}
