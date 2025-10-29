// Copyright (c) 2025 Fabstir
// SPDX-License-Identifier: BUSL-1.1

/// LRU cache for vector chunks with metrics tracking
use crate::core::chunk::VectorChunk;
use lru::LruCache;
use std::num::NonZeroUsize;
use std::sync::{Arc, RwLock};

/// Cache metrics for monitoring performance
#[derive(Debug, Clone, Default)]
pub struct CacheMetrics {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
}

impl CacheMetrics {
    /// Create new metrics with all counters at zero
    pub fn new() -> Self {
        Self::default()
    }

    /// Get total number of requests (hits + misses)
    pub fn total_requests(&self) -> u64 {
        self.hits + self.misses
    }

    /// Calculate hit rate (hits / total_requests)
    pub fn hit_rate(&self) -> f64 {
        let total = self.total_requests();
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }

    /// Reset all metrics to zero
    pub fn reset(&mut self) {
        self.hits = 0;
        self.misses = 0;
        self.evictions = 0;
    }
}

/// Thread-safe LRU cache for vector chunks
pub struct ChunkCache {
    cache: Arc<RwLock<LruCache<String, VectorChunk>>>,
    metrics: Arc<RwLock<CacheMetrics>>,
    capacity: usize,
}

impl ChunkCache {
    /// Create a new chunk cache with specified capacity
    ///
    /// # Arguments
    /// * `capacity` - Maximum number of chunks to cache
    ///
    /// # Panics
    /// Panics if capacity is 0
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "Cache capacity must be greater than 0");

        let non_zero_capacity = NonZeroUsize::new(capacity)
            .expect("Capacity must be non-zero");

        Self {
            cache: Arc::new(RwLock::new(LruCache::new(non_zero_capacity))),
            metrics: Arc::new(RwLock::new(CacheMetrics::new())),
            capacity,
        }
    }

    /// Insert or update a chunk in the cache
    ///
    /// If the cache is at capacity, the least recently used chunk will be evicted.
    ///
    /// # Arguments
    /// * `key` - Unique identifier for the chunk
    /// * `chunk` - The vector chunk to cache
    pub fn put(&self, key: String, chunk: VectorChunk) {
        let mut cache = self.cache.write().unwrap();

        // Check if we're at capacity and will evict
        if cache.len() == self.capacity && !cache.contains(&key) {
            let mut metrics = self.metrics.write().unwrap();
            metrics.evictions += 1;
        }

        cache.put(key, chunk);
    }

    /// Retrieve a chunk from the cache
    ///
    /// This marks the chunk as recently used in the LRU ordering.
    ///
    /// # Arguments
    /// * `key` - The chunk identifier
    ///
    /// # Returns
    /// Some(chunk) if found, None otherwise
    pub fn get(&self, key: &str) -> Option<VectorChunk> {
        let mut cache = self.cache.write().unwrap();
        let mut metrics = self.metrics.write().unwrap();

        match cache.get(key) {
            Some(chunk) => {
                metrics.hits += 1;
                Some(chunk.clone())
            }
            None => {
                metrics.misses += 1;
                None
            }
        }
    }

    /// Check if a chunk exists in the cache without updating LRU
    ///
    /// # Arguments
    /// * `key` - The chunk identifier
    ///
    /// # Returns
    /// true if the chunk is in the cache, false otherwise
    pub fn contains(&self, key: &str) -> bool {
        let cache = self.cache.read().unwrap();
        cache.contains(key)
    }

    /// Remove all chunks from the cache
    ///
    /// This also resets the capacity to its original value but does NOT reset metrics.
    pub fn clear(&self) {
        let mut cache = self.cache.write().unwrap();
        cache.clear();
    }

    /// Get the number of chunks currently in the cache
    pub fn len(&self) -> usize {
        let cache = self.cache.read().unwrap();
        cache.len()
    }

    /// Check if the cache is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get the maximum capacity of the cache
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Get a snapshot of current cache metrics
    pub fn get_metrics(&self) -> CacheMetrics {
        let metrics = self.metrics.read().unwrap();
        metrics.clone()
    }

    /// Reset all metrics to zero
    pub fn reset_metrics(&self) {
        let mut metrics = self.metrics.write().unwrap();
        metrics.reset();
    }

    /// Get the cache hit rate
    pub fn hit_rate(&self) -> f64 {
        let metrics = self.metrics.read().unwrap();
        metrics.hit_rate()
    }
}

// Implement Clone for ChunkCache (clones the Arc, not the data)
impl Clone for ChunkCache {
    fn clone(&self) -> Self {
        Self {
            cache: Arc::clone(&self.cache),
            metrics: Arc::clone(&self.metrics),
            capacity: self.capacity,
        }
    }
}

// Debug implementation
impl std::fmt::Debug for ChunkCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChunkCache")
            .field("capacity", &self.capacity)
            .field("len", &self.len())
            .field("metrics", &self.get_metrics())
            .finish()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_creation() {
        let cache = ChunkCache::new(10);
        assert_eq!(cache.capacity(), 10);
        assert_eq!(cache.len(), 0);
        assert!(cache.is_empty());
    }

    #[test]
    #[should_panic(expected = "Cache capacity must be greater than 0")]
    fn test_cache_zero_capacity_panics() {
        let _ = ChunkCache::new(0);
    }

    #[test]
    fn test_metrics_initial_state() {
        let metrics = CacheMetrics::new();
        assert_eq!(metrics.hits, 0);
        assert_eq!(metrics.misses, 0);
        assert_eq!(metrics.evictions, 0);
        assert_eq!(metrics.total_requests(), 0);
        assert_eq!(metrics.hit_rate(), 0.0);
    }

    #[test]
    fn test_metrics_hit_rate_calculation() {
        let mut metrics = CacheMetrics::new();
        metrics.hits = 7;
        metrics.misses = 3;

        assert_eq!(metrics.total_requests(), 10);
        assert_eq!(metrics.hit_rate(), 0.7);
    }

    #[test]
    fn test_metrics_reset() {
        let mut metrics = CacheMetrics::new();
        metrics.hits = 10;
        metrics.misses = 5;
        metrics.evictions = 2;

        metrics.reset();

        assert_eq!(metrics.hits, 0);
        assert_eq!(metrics.misses, 0);
        assert_eq!(metrics.evictions, 0);
    }

    #[test]
    fn test_cache_clone() {
        let cache1 = ChunkCache::new(10);
        let chunk = VectorChunk::new("test".to_string(), 0, 9999);
        cache1.put("test".to_string(), chunk);

        let cache2 = cache1.clone();

        // Both caches share the same underlying data
        assert_eq!(cache2.len(), 1);
        assert!(cache2.contains("test"));
    }

    #[test]
    fn test_cache_debug() {
        let cache = ChunkCache::new(10);
        let debug_str = format!("{:?}", cache);
        assert!(debug_str.contains("ChunkCache"));
        assert!(debug_str.contains("capacity"));
    }
}
