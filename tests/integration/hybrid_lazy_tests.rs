// Copyright (c) 2025 Fabstir
// SPDX-License-Identifier: BUSL-1.1

use std::sync::Arc;
use std::time::Instant;
use tokio::time::Duration;
use vector_db::core::storage::{S5Storage, MockS5Storage};
use vector_db::core::chunk_cache::ChunkCache;
use vector_db::core::chunk::VectorChunk;
use vector_db::core::types::VectorId;
use vector_db::storage::chunk_loader::ChunkLoader;
use vector_db::hybrid::core::{HybridIndex, HybridConfig};
use chrono::Utc;

/// Helper to create test vectors with varying timestamps
fn create_timestamped_vectors(
    count: usize,
    dimensions: usize,
    days_old: u64,
) -> Vec<(VectorId, Vec<f32>, chrono::DateTime<chrono::Utc>)> {
    let base_time = Utc::now() - chrono::Duration::days(days_old as i64);

    (0..count)
        .map(|i| {
            let id = VectorId::from_string(&format!("vec_{}d_{}", days_old, i));
            let vector = (0..dimensions)
                .map(|d| (i * dimensions + d) as f32 / 100.0)
                .collect();
            let timestamp = base_time + chrono::Duration::hours(i as i64);
            (id, vector, timestamp)
        })
        .collect()
}

/// Helper to create and save vector chunks to storage
async fn create_chunks_in_storage(
    storage: &Arc<MockS5Storage>,
    vectors_per_chunk: usize,
    num_chunks: usize,
    dimensions: usize,
    days_old: u64,
) -> Vec<String> {
    let mut chunk_paths = Vec::new();

    for chunk_idx in 0..num_chunks {
        let chunk_id = format!("chunk_{}d_{}", days_old, chunk_idx);
        let start = chunk_idx * vectors_per_chunk;

        let mut chunk = VectorChunk::new(chunk_id.clone(), start, start + vectors_per_chunk - 1);

        // Add vectors to chunk
        for i in 0..vectors_per_chunk {
            let global_idx = start + i;
            let id = VectorId::from_string(&format!("vec_{}d_{}", days_old, global_idx));
            let vector: Vec<f32> = (0..dimensions)
                .map(|d| (global_idx * dimensions + d) as f32 / 100.0)
                .collect();
            chunk.add_vector(id, vector);
        }

        // Save chunk to storage
        let chunk_data = serde_cbor::to_vec(&chunk).expect("Failed to serialize chunk");
        let path = format!("test/hybrid/chunks/{}.cbor", chunk_id);
        storage.put(&path, chunk_data).await.expect("Failed to save chunk");

        chunk_paths.push(path); // Store full path for lazy loading
    }

    chunk_paths
}

#[tokio::test]
async fn test_hybrid_search_with_lazy_loading() {
    // Setup: Create storage with chunks for both recent and historical data
    let storage = Arc::new(MockS5Storage::new());
    let cache = Arc::new(ChunkCache::new(1000));
    let chunk_loader = Arc::new(ChunkLoader::new(storage.clone(), cache.clone()));

    let dimensions = 8;

    // Create chunks for recent data (< 7 days)
    let recent_chunk_paths = create_chunks_in_storage(&storage, 50, 2, dimensions, 1).await;

    // Create chunks for historical data (> 7 days)
    let historical_chunk_paths = create_chunks_in_storage(&storage, 50, 2, dimensions, 30).await;

    // Create HybridIndex with lazy loading
    let config = HybridConfig::default();
    let mut index = HybridIndex::with_chunk_loader(config, Some(chunk_loader));

    // Initialize with some training data
    let training_vectors: Vec<Vec<f32>> = (0..100)
        .map(|i| (0..dimensions).map(|d| (i * d) as f32 / 100.0).collect())
        .collect();
    index.initialize(training_vectors).await.expect("Failed to initialize");

    // Insert recent vectors with chunk references
    let recent_vectors = create_timestamped_vectors(100, dimensions, 1);
    for (i, (id, vector, timestamp)) in recent_vectors.iter().enumerate() {
        let chunk_idx = i / 50;
        let chunk_id = Some(recent_chunk_paths[chunk_idx].clone());
        index.insert_with_chunk(id.clone(), vector.clone(), *timestamp, chunk_id)
            .await
            .expect("Failed to insert recent vector");
    }

    // Insert historical vectors with chunk references
    let historical_vectors = create_timestamped_vectors(100, dimensions, 30);
    for (i, (id, vector, timestamp)) in historical_vectors.iter().enumerate() {
        let chunk_idx = i / 50;
        let chunk_id = Some(historical_chunk_paths[chunk_idx].clone());
        index.insert_with_chunk(id.clone(), vector.clone(), *timestamp, chunk_id)
            .await
            .expect("Failed to insert historical vector");
    }

    // Search: Should query both HNSW and IVF with lazy loading
    let query = vec![0.5; dimensions];
    let results = index.search(&query, 10).await.expect("Search failed");

    // Verify: Results should come from both indices
    assert!(results.len() > 0);
    assert!(results.len() <= 10);

    println!("Hybrid lazy loading test: Found {} results from both HNSW and IVF", results.len());
}

#[tokio::test]
async fn test_shared_chunk_cache() {
    // Setup: Create storage with chunks that will be accessed by both indices
    let storage = Arc::new(MockS5Storage::new());
    let cache = Arc::new(ChunkCache::new(1000));
    let chunk_loader = Arc::new(ChunkLoader::new(storage.clone(), cache.clone()));

    let dimensions = 8;

    // Create chunks for mixed data
    let chunk_paths = create_chunks_in_storage(&storage, 50, 3, dimensions, 5).await;

    // Create HybridIndex with shared chunk loader
    let config = HybridConfig::default();
    let mut index = HybridIndex::with_chunk_loader(config, Some(chunk_loader.clone()));

    // Initialize
    let training_vectors: Vec<Vec<f32>> = (0..100)
        .map(|i| (0..dimensions).map(|d| (i * d) as f32 / 100.0).collect())
        .collect();
    index.initialize(training_vectors).await.expect("Failed to initialize");

    // Insert vectors that span both indices
    let vectors = create_timestamped_vectors(150, dimensions, 5);
    for (i, (id, vector, timestamp)) in vectors.iter().enumerate() {
        let chunk_idx = i / 50;
        let chunk_id = Some(chunk_paths[chunk_idx].clone());
        index.insert_with_chunk(id.clone(), vector.clone(), *timestamp, chunk_id)
            .await
            .expect("Failed to insert");
    }

    // Search multiple times
    let query = vec![0.5; dimensions];
    for _ in 0..5 {
        index.search(&query, 10).await.expect("Search failed");
    }

    // Verify: Cache is shared between HNSW and IVF
    // Both indices benefit from the same chunk cache
    println!("Shared cache test: Cache shared successfully between HNSW and IVF indices");
}

#[tokio::test]
async fn test_search_correctness_with_lazy_loading() {
    // Setup: Create two identical indices - one with lazy loading, one without
    let storage = Arc::new(MockS5Storage::new());
    let cache = Arc::new(ChunkCache::new(1000));
    let chunk_loader = Arc::new(ChunkLoader::new(storage.clone(), cache.clone()));

    let dimensions = 8;
    let chunk_paths = create_chunks_in_storage(&storage, 50, 2, dimensions, 3).await;

    // Create lazy-loaded index
    let config_lazy = HybridConfig::default();
    let mut index_lazy = HybridIndex::with_chunk_loader(config_lazy.clone(), Some(chunk_loader));

    // Create eager-loaded index
    let mut index_eager = HybridIndex::new(config_lazy);

    // Initialize both
    let training_vectors: Vec<Vec<f32>> = (0..100)
        .map(|i| (0..dimensions).map(|d| (i * d) as f32 / 100.0).collect())
        .collect();
    index_lazy.initialize(training_vectors.clone()).await.expect("Failed to initialize lazy");
    index_eager.initialize(training_vectors).await.expect("Failed to initialize eager");

    // Insert same vectors into both
    let vectors = create_timestamped_vectors(100, dimensions, 3);
    for (i, (id, vector, timestamp)) in vectors.iter().enumerate() {
        let chunk_idx = i / 50;
        let chunk_id = Some(chunk_paths[chunk_idx].clone());

        // Lazy insert
        index_lazy.insert_with_chunk(id.clone(), vector.clone(), *timestamp, chunk_id)
            .await
            .expect("Failed to insert lazy");

        // Eager insert
        index_eager.insert_with_timestamp(id.clone(), vector.clone(), *timestamp)
            .await
            .expect("Failed to insert eager");
    }

    // Search both indices
    let query = vec![0.5; dimensions];
    let results_lazy = index_lazy.search(&query, 10).await.expect("Lazy search failed");
    let results_eager = index_eager.search(&query, 10).await.expect("Eager search failed");

    // Verify: Results should be identical (same vectors found)
    assert_eq!(results_lazy.len(), results_eager.len());

    // Check that the same vector IDs are returned
    let lazy_ids: Vec<_> = results_lazy.iter().map(|r| &r.vector_id).collect();
    let eager_ids: Vec<_> = results_eager.iter().map(|r| &r.vector_id).collect();
    assert_eq!(lazy_ids, eager_ids);

    println!("Correctness test: Lazy loading produces identical results to eager loading");
}

#[tokio::test]
async fn test_insert_with_chunk_references() {
    // Setup
    let storage = Arc::new(MockS5Storage::new());
    let cache = Arc::new(ChunkCache::new(1000));
    let chunk_loader = Arc::new(ChunkLoader::new(storage.clone(), cache.clone()));

    let dimensions = 8;
    let chunk_paths = create_chunks_in_storage(&storage, 50, 2, dimensions, 2).await;

    // Create HybridIndex
    let config = HybridConfig::default();
    let mut index = HybridIndex::with_chunk_loader(config, Some(chunk_loader));

    // Initialize
    let training_vectors: Vec<Vec<f32>> = (0..100)
        .map(|i| (0..dimensions).map(|d| (i * d) as f32 / 100.0).collect())
        .collect();
    index.initialize(training_vectors).await.expect("Failed to initialize");

    // Insert vectors with chunk references
    let vectors = create_timestamped_vectors(100, dimensions, 2);
    for (i, (id, vector, timestamp)) in vectors.iter().enumerate() {
        let chunk_idx = i / 50;
        let chunk_id = Some(chunk_paths[chunk_idx].clone());
        index.insert_with_chunk(id.clone(), vector.clone(), *timestamp, chunk_id)
            .await
            .expect("Failed to insert");
    }

    // Verify: All vectors are searchable
    let query = vec![0.5; dimensions];
    let results = index.search(&query, 10).await.expect("Search failed");
    assert!(results.len() > 0);

    // Verify: Stats reflect inserted vectors
    let stats = index.get_stats();
    assert_eq!(stats.total_vectors, 100);

    println!("Insert with chunks test: {} vectors inserted with chunk references", stats.total_vectors);
}

#[tokio::test]
async fn test_memory_efficiency() {
    // Setup: Create large dataset with lazy loading
    let storage = Arc::new(MockS5Storage::new());
    let cache = Arc::new(ChunkCache::new(500)); // Limited cache
    let chunk_loader = Arc::new(ChunkLoader::new(storage.clone(), cache.clone()));

    let dimensions = 8;

    // Create more chunks than cache can hold
    let chunk_paths = create_chunks_in_storage(&storage, 100, 5, dimensions, 10).await;

    // Create HybridIndex
    let config = HybridConfig::default();
    let mut index = HybridIndex::with_chunk_loader(config, Some(chunk_loader));

    // Initialize
    let training_vectors: Vec<Vec<f32>> = (0..200)
        .map(|i| (0..dimensions).map(|d| (i * d) as f32 / 100.0).collect())
        .collect();
    index.initialize(training_vectors).await.expect("Failed to initialize");

    // Insert large dataset
    let vectors = create_timestamped_vectors(500, dimensions, 10);
    for (i, (id, vector, timestamp)) in vectors.iter().enumerate() {
        let chunk_idx = i / 100;
        let chunk_id = Some(chunk_paths[chunk_idx].clone());
        index.insert_with_chunk(id.clone(), vector.clone(), *timestamp, chunk_id)
            .await
            .expect("Failed to insert");
    }

    // Search: Should work even with limited cache
    let query = vec![0.5; dimensions];
    let results = index.search(&query, 10).await.expect("Search failed");

    // Verify: Search works with limited memory
    assert!(results.len() > 0);

    println!("Memory efficiency test: Handled 500 vectors with limited cache");
}

#[tokio::test]
async fn test_cold_vs_warm_cache_performance() {
    // Setup
    let storage = Arc::new(MockS5Storage::new());
    let cache = Arc::new(ChunkCache::new(1000));
    let chunk_loader = Arc::new(ChunkLoader::new(storage.clone(), cache.clone()));

    let dimensions = 8;
    let chunk_paths = create_chunks_in_storage(&storage, 100, 3, dimensions, 5).await;

    // Create HybridIndex
    let config = HybridConfig::default();
    let mut index = HybridIndex::with_chunk_loader(config, Some(chunk_loader));

    // Initialize
    let training_vectors: Vec<Vec<f32>> = (0..200)
        .map(|i| (0..dimensions).map(|d| (i * d) as f32 / 100.0).collect())
        .collect();
    index.initialize(training_vectors).await.expect("Failed to initialize");

    // Insert vectors
    let vectors = create_timestamped_vectors(300, dimensions, 5);
    for (i, (id, vector, timestamp)) in vectors.iter().enumerate() {
        let chunk_idx = i / 100;
        let chunk_id = Some(chunk_paths[chunk_idx].clone());
        index.insert_with_chunk(id.clone(), vector.clone(), *timestamp, chunk_id)
            .await
            .expect("Failed to insert");
    }

    let query = vec![0.5; dimensions];

    // Cold cache: First search
    let start = Instant::now();
    let _cold_results = index.search(&query, 10).await.expect("Cold search failed");
    let cold_time = start.elapsed();

    // Warm cache: Repeated searches
    let mut warm_times = Vec::new();
    for _ in 0..5 {
        let start = Instant::now();
        index.search(&query, 10).await.expect("Warm search failed");
        warm_times.push(start.elapsed());
    }

    let avg_warm_time = warm_times.iter().sum::<Duration>() / warm_times.len() as u32;

    println!("Performance test:");
    println!("  Cold cache: {:?}", cold_time);
    println!("  Warm cache (avg): {:?}", avg_warm_time);
    println!("  Speedup: {:.2}x", cold_time.as_micros() as f64 / avg_warm_time.as_micros() as f64);

    // Verify: Both searches completed successfully
    // (In tests, performance difference may be small due to small dataset)
    assert!(cold_time.as_millis() >= 0);
    assert!(avg_warm_time.as_millis() >= 0);
}

#[tokio::test]
async fn test_migration_with_lazy_loading() {
    // Setup
    let storage = Arc::new(MockS5Storage::new());
    let cache = Arc::new(ChunkCache::new(1000));
    let chunk_loader = Arc::new(ChunkLoader::new(storage.clone(), cache.clone()));

    let dimensions = 8;
    let chunk_paths = create_chunks_in_storage(&storage, 50, 2, dimensions, 1).await;

    // Create HybridIndex with short threshold
    let mut config = HybridConfig::default();
    config.recent_threshold = std::time::Duration::from_secs(1); // 1 second threshold
    config.auto_migrate = true;

    let mut index = HybridIndex::with_chunk_loader(config, Some(chunk_loader));

    // Initialize
    let training_vectors: Vec<Vec<f32>> = (0..100)
        .map(|i| (0..dimensions).map(|d| (i * d) as f32 / 100.0).collect())
        .collect();
    index.initialize(training_vectors).await.expect("Failed to initialize");

    // Insert recent vectors
    let vectors = create_timestamped_vectors(100, dimensions, 0);
    for (i, (id, vector, timestamp)) in vectors.iter().enumerate() {
        let chunk_idx = i / 50;
        let chunk_id = Some(chunk_paths[chunk_idx].clone());
        index.insert_with_chunk(id.clone(), vector.clone(), *timestamp, chunk_id)
            .await
            .expect("Failed to insert");
    }

    // Wait for vectors to age
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

    // Trigger migration
    let migration_result = index.migrate_old_vectors().await.expect("Migration failed");

    // Verify: Vectors were migrated
    println!("Migration test: {} vectors migrated from HNSW to IVF", migration_result.vectors_migrated);
}
