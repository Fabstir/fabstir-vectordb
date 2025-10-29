// Copyright (c) 2025 Fabstir
// SPDX-License-Identifier: BUSL-1.1

/// Unit tests for LRU chunk cache
use vector_db::core::chunk::VectorChunk;
use vector_db::core::chunk_cache::ChunkCache;
use vector_db::core::types::VectorId;
use std::sync::Arc;

// ============================================================================
// Basic Cache Operations
// ============================================================================

#[test]
fn test_cache_creation() {
    let cache = ChunkCache::new(10); // 10 chunks max

    assert_eq!(cache.capacity(), 10);
    assert_eq!(cache.len(), 0);
    assert!(cache.is_empty());
}

#[test]
fn test_cache_put_and_get() {
    let cache = ChunkCache::new(10);
    let chunk = VectorChunk::new("chunk-0".to_string(), 0, 9999);

    // Put chunk
    cache.put("chunk-0".to_string(), chunk.clone());

    // Get chunk
    let retrieved = cache.get("chunk-0");
    assert!(retrieved.is_some());

    let retrieved_chunk = retrieved.unwrap();
    assert_eq!(retrieved_chunk.chunk_id, "chunk-0");
    assert_eq!(cache.len(), 1);
}

#[test]
fn test_cache_get_nonexistent() {
    let cache = ChunkCache::new(10);

    let result = cache.get("nonexistent");
    assert!(result.is_none());
}

#[test]
fn test_cache_contains() {
    let cache = ChunkCache::new(10);
    let chunk = VectorChunk::new("chunk-0".to_string(), 0, 9999);

    assert!(!cache.contains("chunk-0"));

    cache.put("chunk-0".to_string(), chunk);

    assert!(cache.contains("chunk-0"));
}

// ============================================================================
// LRU Eviction Tests
// ============================================================================

#[test]
fn test_lru_eviction_on_capacity() {
    let cache = ChunkCache::new(3); // Max 3 chunks

    // Add 3 chunks
    for i in 0..3 {
        let chunk = VectorChunk::new(format!("chunk-{}", i), i * 10000, (i + 1) * 10000 - 1);
        cache.put(format!("chunk-{}", i), chunk);
    }

    assert_eq!(cache.len(), 3);

    // Add 4th chunk - should evict chunk-0 (least recently used)
    let chunk3 = VectorChunk::new("chunk-3".to_string(), 30000, 39999);
    cache.put("chunk-3".to_string(), chunk3);

    assert_eq!(cache.len(), 3);
    assert!(!cache.contains("chunk-0")); // Evicted
    assert!(cache.contains("chunk-1"));
    assert!(cache.contains("chunk-2"));
    assert!(cache.contains("chunk-3"));
}

#[test]
fn test_lru_access_order() {
    let cache = ChunkCache::new(3);

    // Add 3 chunks
    for i in 0..3 {
        let chunk = VectorChunk::new(format!("chunk-{}", i), i * 10000, (i + 1) * 10000 - 1);
        cache.put(format!("chunk-{}", i), chunk);
    }

    // Access chunk-0 to make it recently used
    let _ = cache.get("chunk-0");

    // Add chunk-3 - should evict chunk-1 (now least recently used)
    let chunk3 = VectorChunk::new("chunk-3".to_string(), 30000, 39999);
    cache.put("chunk-3".to_string(), chunk3);

    assert_eq!(cache.len(), 3);
    assert!(cache.contains("chunk-0")); // Still present (was accessed)
    assert!(!cache.contains("chunk-1")); // Evicted
    assert!(cache.contains("chunk-2"));
    assert!(cache.contains("chunk-3"));
}

#[test]
fn test_lru_multiple_accesses() {
    let cache = ChunkCache::new(3);

    // Add 3 chunks
    for i in 0..3 {
        let chunk = VectorChunk::new(format!("chunk-{}", i), i * 10000, (i + 1) * 10000 - 1);
        cache.put(format!("chunk-{}", i), chunk);
    }

    // Access in reverse order: 2, 1, 0
    let _ = cache.get("chunk-2");
    let _ = cache.get("chunk-1");
    let _ = cache.get("chunk-0");

    // Add new chunk - should evict chunk-2 (least recently accessed after the reordering)
    let chunk3 = VectorChunk::new("chunk-3".to_string(), 30000, 39999);
    cache.put("chunk-3".to_string(), chunk3);

    assert_eq!(cache.len(), 3);
    assert!(cache.contains("chunk-0")); // Most recently accessed
    assert!(cache.contains("chunk-1"));
    assert!(!cache.contains("chunk-2")); // Evicted (accessed first, so LRU)
    assert!(cache.contains("chunk-3"));
}

// ============================================================================
// Cache Metrics Tests
// ============================================================================

#[test]
fn test_cache_metrics_initial() {
    let cache = ChunkCache::new(10);
    let metrics = cache.get_metrics();

    assert_eq!(metrics.hits, 0);
    assert_eq!(metrics.misses, 0);
    assert_eq!(metrics.evictions, 0);
    assert_eq!(metrics.total_requests(), 0);
}

#[test]
fn test_cache_hit_tracking() {
    let cache = ChunkCache::new(10);
    let chunk = VectorChunk::new("chunk-0".to_string(), 0, 9999);

    cache.put("chunk-0".to_string(), chunk);

    // Hit
    let _ = cache.get("chunk-0");

    let metrics = cache.get_metrics();
    assert_eq!(metrics.hits, 1);
    assert_eq!(metrics.misses, 0);
}

#[test]
fn test_cache_miss_tracking() {
    let cache = ChunkCache::new(10);

    // Miss
    let _ = cache.get("nonexistent");

    let metrics = cache.get_metrics();
    assert_eq!(metrics.hits, 0);
    assert_eq!(metrics.misses, 1);
}

#[test]
fn test_cache_eviction_tracking() {
    let cache = ChunkCache::new(2); // Small cache

    // Add 3 chunks - should trigger 1 eviction
    for i in 0..3 {
        let chunk = VectorChunk::new(format!("chunk-{}", i), i * 10000, (i + 1) * 10000 - 1);
        cache.put(format!("chunk-{}", i), chunk);
    }

    let metrics = cache.get_metrics();
    assert_eq!(metrics.evictions, 1);
}

#[test]
fn test_cache_hit_rate() {
    let cache = ChunkCache::new(10);
    let chunk = VectorChunk::new("chunk-0".to_string(), 0, 9999);

    cache.put("chunk-0".to_string(), chunk);

    // 3 hits, 2 misses
    let _ = cache.get("chunk-0");
    let _ = cache.get("chunk-0");
    let _ = cache.get("chunk-0");
    let _ = cache.get("nonexistent-1");
    let _ = cache.get("nonexistent-2");

    let metrics = cache.get_metrics();
    assert_eq!(metrics.hits, 3);
    assert_eq!(metrics.misses, 2);
    assert_eq!(metrics.hit_rate(), 0.6); // 3 / 5
}

#[test]
fn test_cache_hit_rate_no_requests() {
    let cache = ChunkCache::new(10);
    let metrics = cache.get_metrics();

    assert_eq!(metrics.hit_rate(), 0.0);
}

// ============================================================================
// Cache Clear Tests
// ============================================================================

#[test]
fn test_cache_clear() {
    let cache = ChunkCache::new(10);

    // Add some chunks
    for i in 0..5 {
        let chunk = VectorChunk::new(format!("chunk-{}", i), i * 10000, (i + 1) * 10000 - 1);
        cache.put(format!("chunk-{}", i), chunk);
    }

    assert_eq!(cache.len(), 5);

    // Clear
    cache.clear();

    assert_eq!(cache.len(), 0);
    assert!(cache.is_empty());
}

#[test]
fn test_cache_clear_preserves_capacity() {
    let cache = ChunkCache::new(10);

    // Add chunks
    for i in 0..5 {
        let chunk = VectorChunk::new(format!("chunk-{}", i), i * 10000, (i + 1) * 10000 - 1);
        cache.put(format!("chunk-{}", i), chunk);
    }

    cache.clear();

    assert_eq!(cache.capacity(), 10); // Capacity unchanged
}

// ============================================================================
// Thread Safety Tests
// ============================================================================

#[test]
fn test_cache_concurrent_access() {
    use std::thread;

    let cache = Arc::new(ChunkCache::new(100));
    let mut handles = vec![];

    // Spawn 10 threads, each inserting 10 chunks
    for t in 0..10 {
        let cache_clone = Arc::clone(&cache);
        let handle = thread::spawn(move || {
            for i in 0..10 {
                let chunk_id = format!("chunk-t{}-{}", t, i);
                let chunk = VectorChunk::new(chunk_id.clone(), i * 10000, (i + 1) * 10000 - 1);
                cache_clone.put(chunk_id, chunk);
            }
        });
        handles.push(handle);
    }

    // Wait for all threads
    for handle in handles {
        handle.join().unwrap();
    }

    // Verify all chunks inserted
    assert_eq!(cache.len(), 100);
}

#[test]
fn test_cache_concurrent_read_write() {
    use std::thread;
    use std::time::Duration;

    let cache = Arc::new(ChunkCache::new(50));

    // Pre-populate cache
    for i in 0..50 {
        let chunk = VectorChunk::new(format!("chunk-{}", i), i * 10000, (i + 1) * 10000 - 1);
        cache.put(format!("chunk-{}", i), chunk);
    }

    let mut handles = vec![];

    // Spawn reader threads
    for _ in 0..5 {
        let cache_clone = Arc::clone(&cache);
        let handle = thread::spawn(move || {
            for _ in 0..100 {
                let chunk_id = format!("chunk-{}", rand::random::<usize>() % 50);
                let _ = cache_clone.get(&chunk_id);
                thread::sleep(Duration::from_micros(10));
            }
        });
        handles.push(handle);
    }

    // Spawn writer threads
    for t in 0..5 {
        let cache_clone = Arc::clone(&cache);
        let handle = thread::spawn(move || {
            for i in 0..20 {
                let chunk_id = format!("chunk-new-t{}-{}", t, i);
                let chunk = VectorChunk::new(chunk_id.clone(), i * 10000, (i + 1) * 10000 - 1);
                cache_clone.put(chunk_id, chunk);
                thread::sleep(Duration::from_micros(10));
            }
        });
        handles.push(handle);
    }

    // Wait for all threads
    for handle in handles {
        handle.join().unwrap();
    }

    // Cache should still be valid and at capacity
    assert_eq!(cache.len(), 50);
}

// ============================================================================
// Edge Cases
// ============================================================================

#[test]
fn test_cache_capacity_one() {
    let cache = ChunkCache::new(1);

    let chunk0 = VectorChunk::new("chunk-0".to_string(), 0, 9999);
    cache.put("chunk-0".to_string(), chunk0);

    assert_eq!(cache.len(), 1);

    // Add second chunk - should evict first
    let chunk1 = VectorChunk::new("chunk-1".to_string(), 10000, 19999);
    cache.put("chunk-1".to_string(), chunk1);

    assert_eq!(cache.len(), 1);
    assert!(!cache.contains("chunk-0"));
    assert!(cache.contains("chunk-1"));
}

#[test]
fn test_cache_update_existing() {
    let cache = ChunkCache::new(10);

    // Add chunk
    let mut chunk = VectorChunk::new("chunk-0".to_string(), 0, 9999);
    chunk.add_vector(VectorId::from_string("vec1"), vec![1.0, 2.0, 3.0]);
    cache.put("chunk-0".to_string(), chunk);

    // Update with new chunk (same ID)
    let mut chunk_updated = VectorChunk::new("chunk-0".to_string(), 0, 9999);
    chunk_updated.add_vector(VectorId::from_string("vec1"), vec![4.0, 5.0, 6.0]);
    chunk_updated.add_vector(VectorId::from_string("vec2"), vec![7.0, 8.0, 9.0]);
    cache.put("chunk-0".to_string(), chunk_updated);

    // Verify updated
    let retrieved = cache.get("chunk-0").unwrap();
    assert_eq!(retrieved.vectors.len(), 2);
    assert_eq!(cache.len(), 1); // Still only one entry
}
